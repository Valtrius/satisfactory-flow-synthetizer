//! Compare Custom and Z3 enumeration on the acyclic 7 + 6 + 5 + 4 + 2 case.

mod profile_support;

fn main() {
    profile_support::run(profile_support::ProfileCase {
        name: "7+6+5+4+2",
        inputs: &[24],
        outputs: &[7, 6, 5, 4, 2],
        belt_rate: 1200,
        default_timeout_seconds: 180,
        default_max_nodes: 12,
    });
}
