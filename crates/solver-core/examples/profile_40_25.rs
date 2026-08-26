//! Compare Custom and Z3 enumeration on the cyclic 40 + 25 case.

mod profile_support;

fn main() {
    profile_support::run(profile_support::ProfileCase {
        name: "40+25",
        inputs: &[65],
        outputs: &[40, 25],
        belt_rate: 1200,
        default_timeout_seconds: 180,
        default_max_nodes: 6,
    });
}
