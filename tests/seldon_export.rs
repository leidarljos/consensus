//! Golden DeGroot inputs for one ballot set, and the CLI when `seldon` is absent.
//!
//! Layout read from seldon-code/seldon commit
//! `b4c479a295431f41ecfe121a752d0bcc3465333f` (main):
//! - `examples/DeGroot/conf.toml` and `src/config_parser.cpp` (`parse_config_file`)
//! - `include/network_generation.hpp` (`generate_from_file`) and `test/res/network.txt`
//! - `include/agent_io.hpp` (`agents_from_file`) and `include/agents/simple_agent.hpp`
//! - `src/main.cpp` (`seldon <config> -o <dir> -n <network> -a <opinions>`)

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ljos_consensus::seldon::{seldon_absent_line, write_seldon_inputs};
use ljos_consensus::Ballot;

// The settle defaults a seat gets with the flags omitted. `--self-weight`
// fills a diagonal the trust rows do not name. `--max-iter` and `--tol` are
// written as Seldon's `max_iterations` and `convergence`: the cap of the settles with no round bound, and `Opts::default().tol`.
const SELF_WEIGHT: f64 = 0.5;
const MAX_ITER: usize = 2048;
const TOL: f64 = 1.490116169344802e-8;

// One full share of attention each way. With the diagonal fill, a row is
// `0.5 + 1` and is divided by that sum. Seldon applies those weights as
// written (`DeGrootModel::iteration` does not renormalise a file network).
const TRUST_WEIGHT: f64 = 1.0;

const BALLOTS_JSON: &str = r#"[{"agent":"a","choice":"ship"},{"agent":"b","choice":"hold"}]"#;
const TRUST_JSON: &str =
    r#"[{"from":"a","to":"b","weight":1.0},{"from":"b","to":"a","weight":1.0}]"#;

// `convergence` is Display of `TOL`, which writes every digit and no exponent.
// `connections_per_agent` is the other voter (n - 1); the self loop is not
// part of that count.
const CONFIG: &str = "\
[simulation]
model = \"DeGroot\"
rng_seed = 1

[io]
n_output_agents = 1
print_progress = false
output_initial = true
start_output = 1
start_numbering_from = 0

[model]
max_iterations = 2048

[DeGroot]
convergence = 0.00000001490116169344802

[network]
number_of_agents = 2
connections_per_agent = 1
";

// Names sort as a, b. Each row listens to itself and the other.
// Weights are self `0.5/1.5`, other `1/1.5`.
const NETWORK: &str = "\
# idx_agent,n_neighbors_in,indices_neighbors_in[...],weights_in[...]
# agent 0 a
# agent 1 b
0, 2, 0, 1, 0.3333333333333333, 0.6666666666666666
1, 2, 0, 1, 0.6666666666666666, 0.3333333333333333
";

// Options sort as hold=0, ship=1. `a` voted ship, `b` voted hold.
const OPINIONS: &str = "\
# idx_agent, opinion
# agent 0 a
# agent 1 b
# option 0 hold
# option 1 ship
0, 1
1, 0
";

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "ljos-consensus-{name}-{}-{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn consensus_bin() -> PathBuf {
    // `cargo test` builds this bin beside the library. The filename has a
    // hyphen, and this Cargo does not pass `CARGO_BIN_EXE_*` through to the test.
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let target = std::env::var_os("CARGO_TARGET_DIR").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target"),
        PathBuf::from,
    );
    target.join(profile).join("ljos-consensus")
}

fn ballots() -> Vec<Ballot> {
    vec![
        Ballot {
            agent: "a".into(),
            choice: "ship".into(),
        },
        Ballot {
            agent: "b".into(),
            choice: "hold".into(),
        },
    ]
}

fn trust() -> Vec<(String, String, f64)> {
    vec![
        ("a".into(), "b".into(), TRUST_WEIGHT),
        ("b".into(), "a".into(), TRUST_WEIGHT),
    ]
}

fn toml_assign<'a>(text: &'a str, section: &str, key: &str) -> &'a str {
    let mut here = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            here = name == section;
            continue;
        }
        if !here || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((k, v)) = trimmed.split_once('=') else {
            continue;
        };
        if k.trim() == key {
            return v.trim();
        }
    }
    panic!("[{section}] {key} missing");
}

/// Data rows the way `generate_from_file` and `agents_from_file` read them:
/// `#` only when it is the first character, and the first blank line ends the file.
fn data_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            break;
        }
        if line.starts_with('#') {
            continue;
        }
        rows.push(line.split(',').map(|c| c.trim().to_string()).collect());
    }
    rows
}

/// Second column is the incoming count; the next count columns are indices;
/// the rest are weights. The index in column 0 is ignored.
fn incoming(row: &[String]) -> (Vec<usize>, Vec<f64>) {
    let n: usize = row[1].parse().unwrap();
    let indices = row[2..2 + n]
        .iter()
        .map(|s| s.parse().unwrap())
        .collect::<Vec<_>>();
    let weights = row[2 + n..]
        .iter()
        .map(|s| s.parse().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(indices.len(), n, "{row:?}");
    assert_eq!(weights.len(), n, "{row:?}");
    (indices, weights)
}

fn assert_seldon_shape(config: &str, network: &str, opinions: &str) {
    assert_eq!(toml_assign(config, "simulation", "model"), "\"DeGroot\"");
    assert_eq!(toml_assign(config, "simulation", "rng_seed"), "1");
    assert_eq!(toml_assign(config, "model", "max_iterations"), "2048");
    assert_eq!(
        toml_assign(config, "DeGroot", "convergence"),
        "0.00000001490116169344802"
    );
    assert_eq!(toml_assign(config, "network", "number_of_agents"), "2");
    assert_eq!(toml_assign(config, "network", "connections_per_agent"), "1");
    assert_eq!(toml_assign(config, "io", "n_output_agents"), "1");
    assert_eq!(toml_assign(config, "io", "output_initial"), "true");

    // A blank line would drop every row after it.
    assert!(!network.lines().any(|l| l.is_empty()));
    assert!(!opinions.lines().any(|l| l.is_empty()));

    let net = data_rows(network);
    assert_eq!(net.len(), 2);
    let own = SELF_WEIGHT / (SELF_WEIGHT + TRUST_WEIGHT);
    let other = TRUST_WEIGHT / (SELF_WEIGHT + TRUST_WEIGHT);
    let (idx0, w0) = incoming(&net[0]);
    let (idx1, w1) = incoming(&net[1]);
    // The count includes the voter itself, as seldon's README describes the column.
    assert_eq!(idx0, vec![0, 1]);
    assert_eq!(idx1, vec![0, 1]);
    assert_eq!(w0, vec![own, other]);
    assert_eq!(w1, vec![other, own]);

    let ops = data_rows(opinions);
    assert_eq!(ops.len(), 2);
    // The text after the first comma is the scalar `std::stod` reads.
    let scalars: Vec<f64> = ops.iter().map(|row| row[1].parse().unwrap()).collect();
    assert_eq!(scalars, vec![1.0, 0.0]);
}

#[test]
fn golden_files_match_seldon_layout() {
    let dir = Scratch::new("golden");
    let job =
        write_seldon_inputs(dir.path(), &ballots(), &trust(), SELF_WEIGHT, MAX_ITER, TOL).unwrap();
    assert_eq!(job.agents, ["a", "b"]);
    assert_eq!(job.options, ["hold", "ship"]);

    let config = fs::read_to_string(dir.path().join("config.toml")).unwrap();
    let network = fs::read_to_string(dir.path().join("network.txt")).unwrap();
    let opinions = fs::read_to_string(dir.path().join("opinions.txt")).unwrap();
    assert_eq!(config, CONFIG);
    assert_eq!(network, NETWORK);
    assert_eq!(opinions, OPINIONS);
    assert_seldon_shape(&config, &network, &opinions);
}

#[test]
fn empty_path_writes_files_and_prints_one_line() {
    let root = Scratch::new("cli");
    let cwd = root.path().join("cwd");
    let out = root.path().join("out");
    fs::create_dir_all(&cwd).unwrap();

    let bin = consensus_bin();
    assert!(bin.is_file(), "missing {}", bin.display());
    let output = Command::new(&bin)
        .env("PATH", "")
        .current_dir(&cwd)
        .args([
            "settle",
            "--ballots",
            BALLOTS_JSON,
            "--trust",
            TRUST_JSON,
            "--self-weight",
        ])
        .arg(SELF_WEIGHT.to_string())
        .arg("--seldon")
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        output.status.success(),
        "status {:?}\nstdout {stdout}\nstderr {stderr}",
        output.status
    );
    assert_eq!(stderr, "");
    assert_eq!(stdout.lines().count(), 1, "{stdout:?}");

    let expected = format!(
        "seldon not on PATH; run: seldon {} -o {} -n {} -a {}\n",
        out.join("config.toml").display(),
        out.display(),
        out.join("network.txt").display(),
        out.join("opinions.txt").display(),
    );
    assert_eq!(stdout, expected);
    assert_eq!(seldon_absent_line(&out), expected.trim_end());

    let config = fs::read_to_string(out.join("config.toml")).unwrap();
    let network = fs::read_to_string(out.join("network.txt")).unwrap();
    let opinions = fs::read_to_string(out.join("opinions.txt")).unwrap();
    assert_eq!(config, CONFIG);
    assert_eq!(network, NETWORK);
    assert_eq!(opinions, OPINIONS);
    assert_seldon_shape(&config, &network, &opinions);
}
