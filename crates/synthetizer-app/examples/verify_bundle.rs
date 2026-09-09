//! Packaging check: run beside the distributed backend with all external lookup paths disabled.
use solver_api::{Problem, RunOptions, SolveMode, SolveResult};
use std::{error::Error, sync::atomic::AtomicBool};

fn main() -> Result<(), Box<dyn Error>> {
    if std::env::var_os("SOLVER_CVC5").is_some()
        || std::env::var_os("PATH").is_some_and(|path| !path.is_empty())
    {
        return Err("Run through scripts/verify-release.ps1 with external lookup disabled".into());
    }
    let problem = Problem {
        inputs: vec!["2".parse()?],
        outputs: vec!["1".parse()?, "1".parse()?],
        max_link_rate: "2".parse()?,
    };
    let outcome = synthetizer_app::runtime::solve(
        &problem,
        &RunOptions {
            mode: SolveMode::OneMinNL,
            worker_count: 2,
            max_nodes: Some(1),
        },
        &AtomicBool::new(false),
        &|_| {},
    )?;
    let SolveResult::Optimal(solution) = outcome.result else {
        return Err("Bundled backend did not complete the exact smoke problem".into());
    };
    if (solution.node_count, solution.link_count) != (1, 0) {
        return Err("Bundled backend returned an incorrect optimum".into());
    }
    solver_validation::validate_solution(&problem, &solution.graph)?;
    println!("Packaged solver OK: N=1 L=0");
    Ok(())
}
