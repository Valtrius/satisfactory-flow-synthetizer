//! Small split/merge workload to expose fixed scheduling costs.
mod profile_support;

fn main() {
    profile_support::run(profile_support::ProfileCase {
        name: "2+3 -> 1+4",
        inputs: &[2, 3],
        outputs: &[1, 4],
        belt_rate: 5,
        default_timeout_seconds: 60,
        default_max_nodes: 2,
    });
}
