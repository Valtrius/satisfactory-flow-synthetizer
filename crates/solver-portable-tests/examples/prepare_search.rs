//! Compare native exact searches with the independent reference before browser tests.
use serde_json::json;
use solver_api::{Problem, RunOptions, SolveMode, SolveOutcome, SolveResult};
use solver_portable_tests::summarize;
use std::sync::atomic::AtomicBool;

fn main() {
    let mut cases = Vec::new();
    for (name, inputs, outputs, capacity, cap) in [
        ("mixed inputs", vec!["2", "3"], vec!["1", "4"], "5", 2),
        ("split and merge", vec!["3"], vec!["1", "2"], "3", 2),
        ("exact fraction", vec!["1/3"], vec!["1/6", "1/6"], "1/3", 2),
        (
            "wide exact fraction",
            vec!["9007199254740993/10000000000000000"],
            vec![
                "9007199254740993/20000000000000000",
                "9007199254740993/20000000000000000",
            ],
            "1",
            1,
        ),
        (
            "unsorted rational terminals",
            vec!["3/7", "2/7"],
            vec!["4/7", "1/7"],
            "5/7",
            2,
        ),
        (
            "equal outputs with surplus",
            vec!["2", "1"],
            vec!["1", "1"],
            "3",
            2,
        ),
        ("anonymous discards", vec!["6"], vec!["2"], "6", 2),
        (
            "equal terminal permutations",
            vec!["2", "2"],
            vec!["2", "2"],
            "2",
            2,
        ),
        ("direct surplus", vec!["1", "2"], vec!["1"], "2", 2),
        ("cyclic fifths", vec!["5"], vec!["2", "2", "1"], "6", 3),
        (
            "multiple optimal layouts",
            vec!["6"],
            vec!["3", "2", "1"],
            "1200",
            3,
        ),
        ("insufficient input", vec!["1"], vec!["2"], "3", 0),
        ("external capacity", vec!["3"], vec!["1"], "2", 0),
        ("node cap", vec!["3"], vec!["1", "2"], "3", 0),
    ] {
        let problem = Problem {
            inputs: inputs.iter().map(|s| s.parse().unwrap()).collect(),
            outputs: outputs.iter().map(|s| s.parse().unwrap()).collect(),
            max_link_rate: capacity.parse().unwrap(),
        };
        let options = RunOptions {
            mode: SolveMode::AllMinN,
            max_nodes: Some(cap),
            worker_count: 1,
        };
        let reference =
            solver_reference::solve_problem(&problem, &options, &AtomicBool::new(false)).unwrap();
        for mode in [SolveMode::OneMinNL, SolveMode::AllMinNL, SolveMode::AllMinN] {
            let options = RunOptions { mode, ..options };
            let minimum = match &reference.result {
                SolveResult::Optimal(result) => Some(result.link_count),
                _ => None,
            };
            let solutions = reference
                .solutions
                .iter()
                .filter(|solution| {
                    mode == SolveMode::AllMinN
                        || mode == SolveMode::AllMinNL && Some(solution.link_count) == minimum
                })
                .cloned()
                .collect();
            let expected = SolveOutcome::new(reference.result.clone(), mode, solutions);
            let native =
                solver_core::solve_problem(&problem, &options, &AtomicBool::new(false), &|_| {})
                    .unwrap();
            let summary = summarize(&problem, &native);
            assert_eq!(summary, summarize(&problem, &expected), "{name} {mode:?}");
            cases.push(json!({"name": format!("{name} {mode:?}"), "request": {"problem": problem, "options": options}, "expected": summary}));
        }
    }
    println!("{}", json!(cases));
}
