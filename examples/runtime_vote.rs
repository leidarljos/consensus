//! The seat voting on its own runtime. The ballots and the rows are
//! `runtime_vote` in the library; this prints the settle.
//!
//! ```console
//! $ cargo run --example runtime_vote
//! ```

fn main() {
    let out = ljos_consensus::runtime_vote();
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
