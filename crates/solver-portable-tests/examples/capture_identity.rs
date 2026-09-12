//! Record a review candidate. Never overwrite the pre-port identity fixture.
use serde_json::{Value, json};
use solver_api::{Problem, RunOptions, SolveMode};
use solver_portable_tests::{identity, permuted};
use std::sync::atomic::AtomicBool;

fn append(cases: &mut Vec<Value>, name: &str, input: &Value) {
    let expected = identity(input);
    for rotation in [0, 1, 7] {
        assert_eq!(identity(&permuted(input, rotation)), expected, "{name}");
    }
    cases.push(json!({"name":name,"input":input,"expected":expected}));
}

fn main() {
    assert_eq!(
        std::env::args().nth(1).as_deref(),
        Some("--record-reviewed-candidate"),
        "This records the current checkout, not the unpatched upstream baseline"
    );
    let mut cases = Vec::new();
    for size in [31, 32, 33, 63, 64, 65, 127, 128, 129] {
        for family in ["path", "cycle", "complete", "colored"] {
            if size > 65 && matches!(family, "complete" | "colored") {
                continue;
            }
            let mut edges = Vec::new();
            for left in 0..size {
                for right in left + 1..size {
                    if match family {
                        "path" => right == left + 1,
                        "cycle" => right == left + 1 || left == 0 && right == size - 1,
                        "complete" => true,
                        _ => (left * 61 + right * 13 + left * right) % 17 < 3,
                    } {
                        edges.push((left, right));
                    }
                }
            }
            let colors: Vec<u32> = (0..size)
                .map(|i| {
                    if family == "colored" {
                        u32::try_from(i % 4).unwrap()
                    } else {
                        0
                    }
                })
                .collect();
            append(
                &mut cases,
                &format!("{family}-{size}"),
                &json!({"kind":"raw","colors":colors,"edges":edges}),
            );
        }
    }
    for (index, (inputs, outputs, capacity, cap)) in [
        (vec!["2", "3"], vec!["1", "4"], "5", 2),
        (vec!["3"], vec!["1", "2"], "3", 2),
        (vec!["1/3"], vec!["1/6", "1/6"], "1/3", 2),
        (vec!["2", "1"], vec!["1", "1"], "3", 2),
        (vec!["6"], vec!["2"], "6", 2),
        (vec!["2", "2"], vec!["2", "2"], "2", 2),
        (vec!["1", "2"], vec!["1"], "2", 2),
        (vec!["5"], vec!["2", "2", "1"], "6", 3),
        (vec!["6"], vec!["3", "2", "1"], "1200", 3),
    ]
    .into_iter()
    .enumerate()
    {
        let problem = Problem {
            inputs: inputs.iter().map(|s| s.parse().unwrap()).collect(),
            outputs: outputs.iter().map(|s| s.parse().unwrap()).collect(),
            max_link_rate: capacity.parse().unwrap(),
        };
        let result = solver_reference::solve_problem(
            &problem,
            &RunOptions {
                mode: SolveMode::AllMinN,
                max_nodes: Some(cap),
                worker_count: 1,
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        for (solution_index, solution) in result.solutions.into_iter().enumerate() {
            append(
                &mut cases,
                &format!("physical-{index}-{solution_index}"),
                &json!({"kind":"physical","problem":problem,"graph":solution.graph}),
            );
        }
    }
    println!(
        "{}",
        json!({"baseline":"review candidate from the current checkout; not the pre-port registry baseline","cases":cases})
    );
}
