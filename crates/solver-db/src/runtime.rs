//! Safe runtime construction from the persistent symbolic component catalog.
//!
//! The database can add macro transitions and feasible concrete incumbents. It
//! cannot remove primitive transitions or replace the live finite proof for a
//! concrete application key.

use solver_core::component_search::ApplicationComponentRuntime;

use crate::{ComponentDatabase, PrewarmOptions};

/// Live application runtime backed by one component database.
pub type DatabaseApplicationComponentRuntime<'a> =
    ApplicationComponentRuntime<'a, ComponentDatabase, ComponentDatabase>;

/// Observable result of loading the optional prewarmed symbolic catalog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComponentCatalogLoad {
    /// Persistence was explicitly disabled, so the runtime starts cold.
    Unavailable,
    /// The runtime imported every component from the cells verified in this read.
    Loaded {
        /// Finite cells induced by the requested prewarm bounds.
        expected_cells: u64,
        /// Complete cell ledgers that passed every digest and membership check.
        verified_cells: u64,
        /// Unique exact symbolic components inserted into the runtime catalog.
        components: u64,
    },
    /// An operational read failed. The returned runtime starts with an empty catalog.
    Failed {
        /// Diagnostic only. Search semantics do not depend on this message.
        message: String,
    },
}

impl ComponentDatabase {
    /// Creates a live application runtime and imports verified prewarmed macros.
    ///
    /// Corrupt or absent accelerator data becomes a catalog miss. If an
    /// operational failure interrupts loading, this method discards the partial
    /// load and returns an empty runtime. In every case the runtime still
    /// re-proves the best component for each concrete normalized boundary-rate
    /// and capacity key before publishing a complete work-table value.
    #[must_use]
    pub fn application_component_runtime(
        &self,
        options: PrewarmOptions,
    ) -> (
        DatabaseApplicationComponentRuntime<'_>,
        ComponentCatalogLoad,
    ) {
        let runtime = ApplicationComponentRuntime::new(self, self);
        if !self.is_enabled() {
            return (runtime, ComponentCatalogLoad::Unavailable);
        }

        let catalog = match self.load_verified_prewarm_catalog(options) {
            Ok(catalog) => catalog,
            Err(error) => {
                return (
                    runtime,
                    ComponentCatalogLoad::Failed {
                        message: error.to_string(),
                    },
                );
            }
        };
        let component_count = u64::try_from(catalog.components.len()).unwrap_or(u64::MAX);
        for component in catalog.components {
            runtime.insert_component(component);
        }
        (
            runtime,
            ComponentCatalogLoad::Loaded {
                expected_cells: catalog.expected_cells,
                verified_cells: catalog.verified_cells,
                components: component_count,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use rusqlite::params;
    use solver_api::{NodeProfile, OptimalSolution, Problem, Rational, SolveResult};
    use solver_core::{
        Preparation, SolveOptions, prepare_problem,
        search::{
            ProfileSearchResult, ProfileSearchStats, ProfileWitness,
            search_profile_with_component_resolver,
        },
        solve_with_component_resolver,
        solver::test_support::{TestRootSchedule, solve_with_component_resolver_and_root_schedule},
    };

    use super::*;
    use crate::{ComponentDatabaseConfig, DatabaseMode, PrewarmResult, PrewarmScheduling};

    fn rational(value: &str) -> Rational {
        value.parse().unwrap()
    }

    fn split_problem() -> Problem {
        Problem {
            inputs: vec![rational("2")],
            outputs: vec![rational("1"), rational("1")],
            max_link_rate: rational("2"),
        }
    }

    fn one_node_options() -> PrewarmOptions {
        PrewarmOptions {
            max_nodes: 1,
            max_boundary_ports: None,
            scheduling: PrewarmScheduling::Live,
        }
    }

    fn exhausted(result: ProfileSearchResult) -> (Option<ProfileWitness>, ProfileSearchStats) {
        match result {
            ProfileSearchResult::Exhausted {
                best_witness,
                stats,
                ..
            } => (best_witness, stats),
            result => panic!("profile search did not exhaust: {result:?}"),
        }
    }

    fn optimal(result: SolveResult) -> OptimalSolution {
        match result {
            SolveResult::Optimal(solution) => solution,
            result => panic!("outer solve did not prove an optimum: {result:?}"),
        }
    }

    #[test]
    fn exact_outer_witness_is_stable_across_workers_root_orders_and_sqlite_catalogs() {
        let problem = split_problem();
        let cancel = AtomicBool::new(false);
        let warm_database = ComponentDatabase::open(&ComponentDatabaseConfig {
            mode: DatabaseMode::InMemory,
            ..ComponentDatabaseConfig::default()
        })
        .unwrap();
        assert!(matches!(
            warm_database.prewarm(one_node_options()),
            PrewarmResult::CompleteThrough {
                completed_cells: 4,
                ..
            }
        ));

        let mut expected = None;
        for schedule in [
            TestRootSchedule::Canonical,
            TestRootSchedule::ReverseCanonical,
        ] {
            for worker_count in [1, 2, 4] {
                let options = SolveOptions {
                    max_nodes: Some(1),
                    worker_count,
                };

                // A fresh in-memory SQLite database gives every matrix point a
                // genuinely cold catalog, including an empty application table.
                let cold_database = ComponentDatabase::open(&ComponentDatabaseConfig {
                    mode: DatabaseMode::InMemory,
                    ..ComponentDatabaseConfig::default()
                })
                .unwrap();
                let (cold_runtime, cold_load) =
                    cold_database.application_component_runtime(one_node_options());
                assert_eq!(
                    cold_load,
                    ComponentCatalogLoad::Loaded {
                        expected_cells: 4,
                        verified_cells: 0,
                        components: 0,
                    }
                );
                let cold = optimal(
                    solve_with_component_resolver_and_root_schedule(
                        &problem,
                        &options,
                        &cancel,
                        &cold_runtime,
                        schedule,
                    )
                    .unwrap(),
                );

                let (warm_runtime, warm_load) =
                    warm_database.application_component_runtime(one_node_options());
                let ComponentCatalogLoad::Loaded {
                    expected_cells,
                    verified_cells,
                    components,
                } = warm_load
                else {
                    panic!("prewarmed SQLite catalog did not load");
                };
                assert_eq!((expected_cells, verified_cells), (4, 4));
                assert!(components > 0);
                let warm = optimal(
                    solve_with_component_resolver_and_root_schedule(
                        &problem,
                        &options,
                        &cancel,
                        &warm_runtime,
                        schedule,
                    )
                    .unwrap(),
                );

                assert_eq!(cold, warm);
                if let Some(expected) = &expected {
                    assert_eq!(&cold, expected);
                } else {
                    expected = Some(cold);
                }
            }
        }

        // Equivalent node/port labels have their own deeper state-level test:
        // `solver_core::search::tests::canonical_mrv_keeps_relabelled_state_cache_outcomes_equivalent`.
        let expected = expected.expect("the determinism matrix must produce a witness");
        assert_eq!((expected.node_count, expected.link_count), (1, 0));
    }

    #[test]
    fn real_sqlite_warm_catalog_matches_cold_outer_solve_and_is_used() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("runtime-components.sqlite3");
        let config = ComponentDatabaseConfig {
            mode: DatabaseMode::Path(path),
            ..ComponentDatabaseConfig::default()
        };
        let problem = split_problem();
        let cancel = AtomicBool::new(false);
        let options = SolveOptions {
            max_nodes: Some(1),
            ..SolveOptions::default()
        };

        let cold_result = {
            let database = ComponentDatabase::open(&config).unwrap();
            let (runtime, load) = database.application_component_runtime(one_node_options());
            assert_eq!(
                load,
                ComponentCatalogLoad::Loaded {
                    expected_cells: 4,
                    verified_cells: 0,
                    components: 0,
                }
            );
            solve_with_component_resolver(&problem, &options, &cancel, &runtime).unwrap()
        };

        {
            let database = ComponentDatabase::open(&config).unwrap();
            assert!(matches!(
                database.prewarm(one_node_options()),
                PrewarmResult::CompleteThrough {
                    completed_cells: 4,
                    ..
                }
            ));
        }

        let database = ComponentDatabase::open(&config).unwrap();
        let (runtime, load) = database.application_component_runtime(one_node_options());
        let ComponentCatalogLoad::Loaded {
            expected_cells,
            verified_cells,
            components,
        } = load
        else {
            panic!("prewarmed catalog did not load");
        };
        assert_eq!((expected_cells, verified_cells), (4, 4));
        assert!(components > 0);

        let warm_result =
            solve_with_component_resolver(&problem, &options, &cancel, &runtime).unwrap();
        assert_eq!(warm_result, cold_result);

        let Preparation::Prepared(normalized) = prepare_problem(&problem).unwrap() else {
            panic!("splitter problem must normalize");
        };
        let (witness, stats) = exhausted(search_profile_with_component_resolver(
            &normalized,
            NodeProfile {
                splitter2: 1,
                ..NodeProfile::default()
            },
            &cancel,
            &[],
            &runtime,
        ));
        assert!(witness.is_some());
        assert!(stats.instrumentation.component_hits > 0);
    }

    #[test]
    fn corrupt_prewarm_cell_is_a_catalog_miss_not_a_solver_dependency() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("corrupt-runtime-components.sqlite3");
        let config = ComponentDatabaseConfig {
            mode: DatabaseMode::Path(path),
            ..ComponentDatabaseConfig::default()
        };
        let problem = split_problem();
        let options = SolveOptions {
            max_nodes: Some(1),
            ..SolveOptions::default()
        };
        let cancel = AtomicBool::new(false);

        let expected = {
            let database = ComponentDatabase::open(&config).unwrap();
            let (runtime, _) = database.application_component_runtime(one_node_options());
            solve_with_component_resolver(&problem, &options, &cancel, &runtime).unwrap()
        };
        {
            let database = ComponentDatabase::open(&config).unwrap();
            assert!(matches!(
                database.prewarm(one_node_options()),
                PrewarmResult::CompleteThrough { .. }
            ));
            database
                .connection
                .as_ref()
                .unwrap()
                .lock()
                .unwrap()
                .execute(
                    "UPDATE prewarm_cells SET manifest_hash=?1
                     WHERE rowid=(SELECT MIN(rowid) FROM prewarm_cells)",
                    params![[0_u8; 32].as_slice()],
                )
                .unwrap();
        }

        let database = ComponentDatabase::open(&config).unwrap();
        let (runtime, load) = database.application_component_runtime(one_node_options());
        let ComponentCatalogLoad::Loaded {
            expected_cells,
            verified_cells,
            ..
        } = load
        else {
            panic!("corrupt accelerator should degrade to verified misses");
        };
        assert_eq!(expected_cells, 4);
        assert!(verified_cells < expected_cells);
        let actual = solve_with_component_resolver(&problem, &options, &cancel, &runtime).unwrap();
        assert_eq!(actual, expected);
        assert!(database.status().unwrap().quarantined_records > 0);
    }
}
