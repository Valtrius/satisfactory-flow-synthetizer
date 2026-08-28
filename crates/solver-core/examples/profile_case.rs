//! Profile a file-based exact problem through the same solver paths as the named examples.

mod profile_support;

fn main() {
    profile_support::run_file();
}

#[cfg(test)]
mod tests {
    use solver_api::Rational;

    #[test]
    fn cyclic_case_retains_exact_decimal_rates() {
        let data: serde_json::Value = serde_json::from_str(include_str!(
            "../../../benchmarks/custom/cases/cyclic10.json"
        ))
        .unwrap();
        let problem: solver_api::Problem = serde_json::from_value(data["problem"].clone()).unwrap();
        assert_eq!(
            problem.outputs,
            vec![
                "151/25".parse::<Rational>().unwrap(),
                "99/25".parse().unwrap()
            ]
        );
        assert_eq!(problem.max_link_rate, Rational::from(1200));
        let solver_core::problem::Preparation::Prepared(normalized) =
            solver_core::problem::prepare_problem(&problem).unwrap()
        else {
            panic!("cyclic benchmark must not be rejected before search");
        };
        let lower = solver_core::lower_bound::baseline_lower_bounds(&normalized).unwrap();
        assert_eq!(lower.combined_nodes, 11);
        let schedule: serde_json::Value = serde_json::from_str(include_str!(
            "../../../benchmarks/custom/diagnostic-screening.json"
        ))
        .unwrap();
        for job in schedule["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|job| job["Case"] == "cyclic10")
        {
            assert!(
                job["MaxNodes"].as_u64().unwrap() >= u64::from(lower.combined_nodes),
                "diagnostic cap must reach actual search"
            );
        }
    }
}
