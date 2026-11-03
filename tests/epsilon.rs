//! Bounded confidence on cases worked by hand: two blocs short of and at the
//! L1 gap between their ballots, and a chain that merges through a voter
//! split between the options.

use std::collections::BTreeMap;
use std::process::Command;

use ljos_consensus::{settle_bounded, Ballot, Outcome};

/// Every case is a mean of at most four voters over at most four rounds:
/// Higham's gamma_4 per round puts rounding under 4 * 4 * 2^-53, about
/// 2e-15. This sits far above that and far below the 29/1152 between the
/// chain's answer and its opening mean.
const ROUNDING: f64 = 1e-12;

/// `settle --epsilon`'s own defaults, so the library cases run as the CLI would: the cap of the settles with no round bound, and `Opts::default().tol`.
const MAX_ITER: usize = 2048;
const TOL: f64 = 1.490116169344802e-8;

const BLOCS: &str = r#"[
    {"agent": "a", "choice": "ship"},
    {"agent": "b", "choice": "ship"},
    {"agent": "c", "choice": "ship"},
    {"agent": "d", "choice": "hold"}
]"#;

fn settle_cli(ballots: &str, epsilon: f64) -> Outcome {
    let out = Command::new(env!("CARGO_BIN_EXE_ljos-consensus"))
        .args(["settle", "--ballots", ballots, "--epsilon"])
        .arg(epsilon.to_string())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn assert_near(got: &[f64], want: &[f64], out: &Outcome) {
    assert_eq!(got.len(), want.len(), "{out:?}");
    for (g, w) in got.iter().zip(want) {
        assert!(
            (g - w).abs() < ROUNDING,
            "{got:?} against {want:?}\n{out:?}"
        );
    }
}

fn assert_clusters(out: &Outcome, want: &[(&[&str], &[f64])]) {
    assert_eq!(out.engine, "bounded-confidence");
    assert_eq!(out.clusters.len(), want.len(), "{:?}", out.clusters);
    for (cluster, (agents, position)) in out.clusters.iter().zip(want) {
        assert_eq!(cluster.agents, *agents, "{:?}", out.clusters);
        assert_near(&cluster.position, position, out);
    }
}

/// Four voters, one with its unit split between `x` and `y`: the CLI holds
/// one ballot per voter, so only a library caller can open between corners.
fn chain() -> Vec<Ballot> {
    [("a", "x"), ("b", "x"), ("b", "y"), ("c", "y"), ("d", "y")]
        .into_iter()
        .map(|(agent, choice)| Ballot {
            agent: agent.into(),
            choice: choice.into(),
        })
        .collect()
}

/// Ballots for two options are 2 apart in L1. Short of that no voter hears
/// the other bloc, and its own already agrees with it, so the first round
/// moves no one.
#[test]
fn two_far_blocs_stay_where_they_voted() {
    for epsilon in [0.0, 1.0, 2.0_f64.next_down()] {
        let out = settle_cli(BLOCS, epsilon);
        assert_eq!(out.options, ["hold", "ship"]);
        assert_clusters(
            &out,
            &[(&["a", "b", "c"], &[0.0, 1.0]), (&["d"], &[1.0, 0.0])],
        );
        assert_near(&out.shares, &[0.25, 0.75], &out);
        assert_eq!(out.rounds, 1, "epsilon {epsilon}");
        assert!(out.settled, "{out:?}");
    }
}

/// The bound includes its edge: at exactly 2 every voter hears all four in
/// round one and moves to their mean, and round two moves no one.
#[test]
fn two_blocs_exactly_epsilon_apart_meet_on_the_count() {
    let out = settle_cli(BLOCS, 2.0);
    assert_clusters(&out, &[(&["a", "b", "c", "d"], &[0.25, 0.75])]);
    assert_near(&out.shares, &[0.25, 0.75], &out);
    assert_eq!(out.rounds, 2);
    assert!(out.settled, "{out:?}");
}

/// In the share of `x`, an L1 bound of 1 is 1/2, and b opens exactly that
/// far from a and from c. Each row's last column is who hears whom there,
/// which sets the next row:
///
/// | round | a        | b        | c, d     | heard                          |
/// |-------|----------|----------|----------|--------------------------------|
/// | 0     | 1        | 1/2      | 0        | a: a b; b: all; c, d: b c d    |
/// | 1     | 3/4      | 3/8      | 1/6      | a to c is 7/12: as before      |
/// | 2     | 9/16     | 35/96    | 17/72    | a to c is 47/144: all hear all |
/// | 3     | 403/1152 | 403/1152 | 403/1152 | all                            |
///
/// Round four moves no one. The opening mean is 3/8: the merged opinion
/// sits further toward the larger bloc than the count does.
#[test]
fn a_chain_merges_off_the_opening_mean() {
    let out = settle_bounded(&chain(), 1.0, &BTreeMap::new(), MAX_ITER, TOL);
    let merged = [403.0 / 1152.0, 749.0 / 1152.0];
    assert_clusters(&out, &[(&["a", "b", "c", "d"], &merged)]);
    assert_near(&out.shares, &merged, &out);
    assert_eq!(out.rounds, 4);
    assert!(out.settled, "{out:?}");
}

/// One step of binary64 short of the gap, b hears neither neighbour and the
/// chain never forms: three clusters where the voters opened.
#[test]
fn a_chain_just_short_of_its_gap_stays_apart() {
    let out = settle_bounded(
        &chain(),
        1.0_f64.next_down(),
        &BTreeMap::new(),
        MAX_ITER,
        TOL,
    );
    assert_clusters(
        &out,
        &[
            (&["a"], &[1.0, 0.0]),
            (&["b"], &[0.5, 0.5]),
            (&["c", "d"], &[0.0, 1.0]),
        ],
    );
    assert_near(&out.shares, &[0.375, 0.625], &out);
    assert_eq!(out.rounds, 1);
}
