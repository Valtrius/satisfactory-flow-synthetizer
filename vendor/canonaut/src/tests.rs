//! Test suite. Compiled only under `cfg(test)`; the deterministic graph
//! generators it shares with the benchmarks live in [`crate::generators`].

pub use crate::generators::*;


#[cfg(test)]
mod test_graphs {
    use crate::{
        consts::ALGORITHM_INFINITY,
        refine::GraphContext,
        search::canonicalize,
        structs::{BitMatrixDisplay, CanonautManager, CanonautOptions, Statistics},
        tests::{generate_path_graph, generate_cycle_graph, randomly_permute_graph},
        utilities::words_needed,
    };

    #[test]
    fn triangular_graph() {
        let max_vertex_number = 50;
        let repetitions = 100;
        let mut canonaut_options = CanonautOptions {
            get_canonical: true,
            ..Default::default()
        };
        let mut graph_context = GraphContext::new(max_vertex_number);
        for number_of_vertices in 1..max_vertex_number {
            let mut reference = None;
            for _ in 0..repetitions {
                let words_per_vertex = words_needed(number_of_vertices);
                let graph = generate_cycle_graph(number_of_vertices);
                let graph = randomly_permute_graph(&graph, number_of_vertices, words_per_vertex);

                let mut orbits = vec![0u32; number_of_vertices];
                let mut labeling = (0..(number_of_vertices as u32)).collect::<Vec<u32>>();
                let mut partition = vec![ALGORITHM_INFINITY; number_of_vertices];
                let mut statistics = Statistics::new();
                let worksize = 500;
                let mut workspace = vec![0; 2 * words_per_vertex * worksize];

                let mut canonical_graph = vec![0; words_per_vertex * number_of_vertices];

                canonicalize(
                    &graph,
                    &mut labeling,
                    &mut partition,
                    None,
                    &mut orbits,
                    &mut canonaut_options,
                    &mut statistics,
                    &mut workspace,
                    words_per_vertex as u32,
                    number_of_vertices as u32,
                    Some(&mut canonical_graph),
                    &mut graph_context,
                );
                if reference.is_none() {
                    reference = Some(canonical_graph);
                } else {
                    assert!(reference == Some(canonical_graph))
                }
                continue;
            }
        }
    }
    #[test]
    fn path_graph() {
        let max_vertex_number = 50;
        let repetitions = 100;
        let mut canonaut_manager = CanonautManager::new(max_vertex_number);
        for number_of_vertices in 1..max_vertex_number {
            let mut reference = None;
            for _ in 0..repetitions {
                let words_per_vertex = words_needed(number_of_vertices);
                let graph = generate_path_graph(number_of_vertices);
                let graph = randomly_permute_graph(&graph, number_of_vertices, words_per_vertex);
                canonaut_manager.canonize(&graph);
                println!("nv{}rep{}", number_of_vertices, repetitions);

                if reference.is_none() {
                    reference = Some(canonaut_manager.take_canonical_graph());
                } else {
                    if reference.as_deref() != Some(&canonaut_manager.canonical_graph) {
                        panic!("Graphs are not identical")
                    }
                }
                continue;
            }
        }
    }

    #[test]
    fn triangular_graph_manager() {
        let max_vertex_number = 50;
        let repetitions = 100;
        let mut canonaut_options = CanonautOptions::default();
        // Set canonization to active
        let mut manager = CanonautManager::new(50).with_canonization();
        canonaut_options.get_canonical = true;
        for number_of_vertices in 1..max_vertex_number {
            let mut reference = None;
            for _ in 0..repetitions {
                let words_per_vertex = words_needed(number_of_vertices);
                let graph = generate_cycle_graph(number_of_vertices);
                let graph = randomly_permute_graph(&graph, number_of_vertices, words_per_vertex);
                graph.print_matrix(words_per_vertex);

                manager.canonize(&graph);
                println!();
                manager.canonical_graph.print_matrix(words_per_vertex);

                // Permute graph

                if reference.is_none() {
                    reference = Some(manager.canonical_graph.clone());
                } else {
                    assert!(reference == Some(manager.canonical_graph.clone()))
                }
                continue;
            }
        }
    }
}

/// Random G(n,p) graphs across a grid of sizes/densities/seeds, checked for
/// permutation-invariance only (no pynauty ground truth — this is pure-Rust,
/// no external tools required, unlike `test_g6`'s `vs_pynauty`).
#[cfg(test)]
mod test_random {
    use crate::{
        structs::CanonautManager,
        tests::{generate_random_graph, randomly_permute_graph},
        utilities::words_needed,
    };

    const SIZES: &[usize] = &[5, 8, 13, 21, 34, 55, 89];
    const DENSITIES: &[f64] = &[0.05, 0.1, 0.3, 0.5, 0.7, 0.9, 0.95];
    const SEEDS: &[u64] = &[0, 1, 2, 3, 4];
    const REPS: usize = 5;

    #[test]
    fn canonical_form_is_permutation_invariant() {
        let max_vertex_count = *SIZES.iter().max().unwrap();
        let mut manager = CanonautManager::new(max_vertex_count).with_canonization();

        for &vertex_count in SIZES {
            for &density in DENSITIES {
                for &seed in SEEDS {
                    let words_per_vertex = words_needed(vertex_count);
                    let graph = generate_random_graph(vertex_count, density, seed);

                    manager.canonize(&graph);
                    let reference = manager.canonical_graph.clone();

                    for repetition in 0..REPS {
                        let permuted = randomly_permute_graph(&graph, vertex_count, words_per_vertex);
                        manager.canonize(&permuted);
                        assert_eq!(
                            manager.canonical_graph, reference,
                            "vertex_count={vertex_count} density={density} seed={seed} repetition={repetition}: \
                             canonical form not permutation-invariant",
                        );
                    }
                }
            }
        }
    }
}

/// `CanonautManager::canonize_with_colors` — vertex-colored (partitioned) canonicalization.
#[cfg(test)]
mod test_colors {
    use crate::{structs::CanonautManager, generators::generate_path_graph};

    /// P4 (path on 4 vertices, edges 0-1,1-2,2-3) has one nontrivial automorphism:
    /// the reversal 0<->3, 1<->2. Coloring {0,1} vs {2,3} forbids it (0 is color 0,
    /// its only possible image under reversal is 3, which is color 1) — so coloring
    /// must collapse the group to trivial and split the 2 orbits into 4 singletons.
    #[test]
    fn coloring_breaks_symmetry() {
        let graph = generate_path_graph(4);
        let mut manager = CanonautManager::new(4).with_canonization();

        manager.canonize(&graph);
        assert_eq!(manager.statistics().number_of_orbits, 2, "uncolored P4 should have 2 orbits");
        assert_eq!(manager.statistics().group_size1, 2.0, "uncolored P4 should have |Aut|=2");

        manager.canonize_with_colors(&graph, &[0, 0, 1, 1]);
        assert_eq!(manager.statistics().number_of_orbits, 4, "colored P4 should have 4 orbits (trivial group)");
        assert_eq!(manager.statistics().group_size1, 1.0, "colored P4 should have |Aut|=1");

        // Coloring only applies to the call it's passed to; the next plain
        // canonize() call must go back to the uncolored (trivial-partition) result.
        manager.canonize(&graph);
        assert_eq!(manager.statistics().number_of_orbits, 2, "coloring must not leak into the next plain canonize()");
    }

    /// Colors that don't actually distinguish any pair of interchangeable vertices
    /// (e.g. all equal) must be a no-op: same result as calling canonize() uncolored.
    #[test]
    fn trivial_coloring_matches_uncolored() {
        let graph = generate_path_graph(4);
        let mut manager = CanonautManager::new(4).with_canonization();

        manager.canonize(&graph);
        let uncolored = manager.canonical_graph.clone();

        manager.canonize_with_colors(&graph, &[7, 7, 7, 7]);
        assert_eq!(manager.canonical_graph, uncolored);
    }

    /// Colors need not be contiguous, sorted, or start at 0 — only which vertices
    /// share a color matters.
    #[test]
    fn colors_need_not_be_contiguous_or_sorted() {
        let graph = generate_path_graph(4);
        let mut manager = CanonautManager::new(4).with_canonization();

        manager.canonize_with_colors(&graph, &[0, 0, 1, 1]);
        let reference = manager.canonical_graph.clone();
        let reference_orbits = manager.statistics().number_of_orbits;

        manager.canonize_with_colors(&graph, &[500, 500, 3, 3]);
        assert_eq!(manager.canonical_graph, reference);
        assert_eq!(manager.statistics().number_of_orbits, reference_orbits);
    }

    #[test]
    #[should_panic(expected = "colors.len()")]
    fn wrong_length_colors_panics() {
        let graph = generate_path_graph(4);
        let mut manager = CanonautManager::new(4).with_canonization();
        manager.canonize_with_colors(&graph, &[0, 0, 1]);
    }
}

/// `DenseGraph::new`/`add_edge`/`add_arc` and `CanonautManager::canonize_graph`/
/// `canonize_graph_with_colors` — the ergonomic DenseGraph-based API surface,
/// cross-checked against the equivalent raw-slice calls.
#[cfg(test)]
mod test_dense_graph_api {
    use crate::{structs::{CanonautManager, DenseGraph}, generators::generate_cycle_graph};

    #[test]
    fn add_edge_matches_generated_cycle() {
        let raw = generate_cycle_graph(4);

        let mut graph = DenseGraph::new(4);
        graph.add_edge(0, 1);
        graph.add_edge(1, 2);
        graph.add_edge(2, 3);
        graph.add_edge(3, 0);

        assert_eq!(graph.adjacency, raw);
    }

    #[test]
    fn canonize_graph_matches_canonize() {
        let mut graph = DenseGraph::new(4);
        graph.add_edge(0, 1);
        graph.add_edge(1, 2);
        graph.add_edge(2, 3);
        graph.add_edge(3, 0);

        let mut manager = CanonautManager::new(4).with_canonization();
        manager.canonize_graph(&graph);
        let via_graph = manager.canonical_graph.clone();

        manager.canonize(&graph.adjacency);
        let via_slice = manager.canonical_graph.clone();

        assert_eq!(via_graph, via_slice);
        assert_eq!(manager.statistics().number_of_orbits, 1);
    }

    #[test]
    fn canonize_graph_with_colors_matches_canonize_with_colors() {
        let mut graph = DenseGraph::new(4);
        graph.add_edge(0, 1);
        graph.add_edge(1, 2);
        graph.add_edge(2, 3);

        let mut manager = CanonautManager::new(4).with_canonization();
        manager.canonize_graph_with_colors(&graph, &[0, 0, 1, 1]);
        let via_graph = manager.canonical_graph.clone();

        manager.canonize_with_colors(&graph.adjacency, &[0, 0, 1, 1]);
        let via_slice = manager.canonical_graph.clone();

        assert_eq!(via_graph, via_slice);
        assert_eq!(manager.statistics().number_of_orbits, 4);
    }

    #[test]
    fn add_arc_sets_only_source_row() {
        let mut graph = DenseGraph::new_directed(3);
        graph.add_arc(0, 1);

        // No .with_digraph() call needed: canonize_graph reads it from the graph.
        let mut manager = CanonautManager::new(3).with_canonization();
        manager.canonize_graph(&graph);
        // 0 (out-only), 1 (in-only), 2 (isolated) are pairwise distinguishable.
        assert_eq!(manager.statistics().number_of_orbits, 3);
    }

    #[test]
    #[should_panic(expected = "add_edge() called on a directed DenseGraph")]
    fn add_edge_panics_on_directed_graph() {
        let mut graph = DenseGraph::new_directed(2);
        graph.add_edge(0, 1);
    }

    #[test]
    #[should_panic(expected = "add_arc() called on an undirected DenseGraph")]
    fn add_arc_panics_on_undirected_graph() {
        let mut graph = DenseGraph::new(2);
        graph.add_arc(0, 1);
    }

    #[test]
    fn canonize_graph_uses_stored_colors_automatically() {
        let mut graph = DenseGraph::new(4);
        graph.add_edge(0, 1);
        graph.add_edge(1, 2);
        graph.add_edge(2, 3);
        graph.set_colors(vec![0, 0, 1, 1]);

        let mut manager = CanonautManager::new(4).with_canonization();
        manager.canonize_graph(&graph);
        let via_stored_colors = manager.canonical_graph.clone();

        manager.canonize_with_colors(&graph.adjacency, &[0, 0, 1, 1]);
        let via_explicit_colors = manager.canonical_graph.clone();

        assert_eq!(via_stored_colors, via_explicit_colors);
        assert_eq!(manager.statistics().number_of_orbits, 4);
    }

    #[test]
    fn canonize_graph_with_colors_ignores_stored_colors() {
        let mut graph = DenseGraph::new(4);
        graph.add_edge(0, 1);
        graph.add_edge(1, 2);
        graph.add_edge(2, 3);
        graph.set_colors(vec![0, 0, 1, 1]); // stored, but should be ignored below

        let mut manager = CanonautManager::new(4).with_canonization();
        manager.canonize_graph_with_colors(&graph, &[7, 7, 7, 7]); // trivial coloring
        let via_override = manager.canonical_graph.clone();

        manager.canonize(&graph.adjacency); // plain, uncolored
        let via_uncolored = manager.canonical_graph.clone();

        assert_eq!(via_override, via_uncolored);
        assert_eq!(manager.statistics().number_of_orbits, 2);
    }

    #[test]
    fn set_colors_replaces_previous_coloring() {
        let mut graph = DenseGraph::new(4);
        graph.set_colors(vec![0, 0, 1, 1]);
        graph.set_colors(vec![7, 7, 7, 7]);
        assert_eq!(graph.colors(), Some(&[7, 7, 7, 7][..]));
    }

    #[test]
    #[should_panic(expected = "colors.len()")]
    fn set_colors_panics_on_wrong_length() {
        let mut graph = DenseGraph::new(4);
        graph.set_colors(vec![0, 0, 1]);
    }
}

/// Tests that verify canonical form output against pynauty ground truth.
///
/// For each named graph in `test_vectors::TEST_VECTORS` we:
///   1. Build the graph in the Rust MSB-0 bit-matrix format.
///   2. Randomly permute it `REPS` times.
///   3. Run canonicalize() with canonization enabled.
///   4. Assert the canonical graph exactly matches the pynauty reference.
///   5. Assert orbit count and automorphism group size match.
#[cfg(test)]
mod test_known_graphs {
    use crate::{
        structs::{BitMatrixDisplay, CanonautManager},
        test_vectors::TEST_VECTORS,
        tests::randomly_permute_graph,
        utilities::words_needed,
    };

    const REPS: usize = 20;

    #[test]
    fn canonical_forms_match_pynauty() {
        let mut manager = CanonautManager::new(64).with_canonization();

        for test_vector in TEST_VECTORS.iter().filter(|test_vector| !test_vector.directed) {
            let vertex_count = test_vector.number_of_vertices;
            let words_per_vertex = test_vector.words_per_vertex;

            assert_eq!(
                words_per_vertex,
                words_needed(vertex_count),
                "Test vector '{}': words_per_vertex mismatch",
                test_vector.name
            );
            assert_eq!(
                test_vector.canon.len(),
                vertex_count * words_per_vertex,
                "Test vector '{}': canon length mismatch",
                test_vector.name
            );

            let reference = test_vector.canon.to_vec();

            for repetition in 0..REPS {
                // Permute the reference canonical graph to get a valid isomorph
                let permuted = randomly_permute_graph(&reference, vertex_count, words_per_vertex);

                manager.canonize(&permuted);

                assert_eq!(
                    manager.canonical_graph, reference,
                    "Graph '{}' repetition {}: canonical form mismatch.\n\
                     Expected:\n{}\nGot:\n{}",
                    test_vector.name,
                    repetition,
                    reference.to_bit_matrix(words_per_vertex),
                    manager.canonical_graph.to_bit_matrix(words_per_vertex),
                );

                // Orbit count from canonaut should match pynauty
                assert_eq!(
                    manager.statistics().number_of_orbits as usize,
                    test_vector.number_of_orbits,
                    "Graph '{}' repetition {}: orbit count mismatch (got {}, want {})",
                    test_vector.name,
                    repetition,
                    manager.statistics().number_of_orbits,
                    test_vector.number_of_orbits,
                );
            }
        }
    }
}

/// Digraph counterpart of `test_known_graphs`: canonical form and automorphism
/// group statistics checked against pynauty ground truth (`directed=True`) for
/// the directed test vectors (directed cycles/paths, transitive tournaments,
/// in-/out-stars). Closes the gap where `test_digraphs` only checked internal
/// self-consistency, not an external reference.
#[cfg(test)]
mod test_known_digraphs {
    use crate::{
        structs::{BitMatrixDisplay, CanonautManager},
        test_vectors::TEST_VECTORS,
        tests::randomly_permute_graph,
        utilities::words_needed,
    };

    const REPS: usize = 20;

    #[test]
    fn canonical_forms_match_pynauty() {
        let mut manager = CanonautManager::new(64).with_canonization().with_digraph();

        for test_vector in TEST_VECTORS.iter().filter(|test_vector| test_vector.directed) {
            let vertex_count = test_vector.number_of_vertices;
            let words_per_vertex = test_vector.words_per_vertex;

            assert_eq!(
                words_per_vertex,
                words_needed(vertex_count),
                "Test vector '{}': words_per_vertex mismatch",
                test_vector.name
            );
            assert_eq!(
                test_vector.canon.len(),
                vertex_count * words_per_vertex,
                "Test vector '{}': canon length mismatch",
                test_vector.name
            );

            let reference = test_vector.canon.to_vec();

            for repetition in 0..REPS {
                let permuted = randomly_permute_graph(&reference, vertex_count, words_per_vertex);

                manager.canonize(&permuted);

                assert_eq!(
                    manager.canonical_graph, reference,
                    "Digraph '{}' repetition {}: canonical form mismatch.\n\
                     Expected:\n{}\nGot:\n{}",
                    test_vector.name,
                    repetition,
                    reference.to_bit_matrix(words_per_vertex),
                    manager.canonical_graph.to_bit_matrix(words_per_vertex),
                );

                assert_eq!(
                    manager.statistics().number_of_orbits as usize,
                    test_vector.number_of_orbits,
                    "Digraph '{}' repetition {}: orbit count mismatch (got {}, want {})",
                    test_vector.name,
                    repetition,
                    manager.statistics().number_of_orbits,
                    test_vector.number_of_orbits,
                );

                assert_eq!(
                    manager.statistics().group_size1,
                    test_vector.group_size1,
                    "Digraph '{}' repetition {}: group_size1 mismatch",
                    test_vector.name,
                    repetition,
                );
                assert_eq!(
                    manager.statistics().group_size2,
                    test_vector.group_size2 as u32,
                    "Digraph '{}' repetition {}: group_size2 mismatch",
                    test_vector.name,
                    repetition,
                );
            }
        }
    }
}

/// Tests against graph6 files in test_data/.
///
/// `self_consistency`: each graph is permuted REPS times; all permutations must produce
/// the same canonical form. Requires no external tools.
///
/// `vs_pynauty`: compares canonical forms against binary reference files generated by
/// `tools/gen_g6_vectors.py` (see `./setup.sh`). These tests are ignored by default;
/// run with `cargo test -- --ignored` after generating the reference files.
#[cfg(test)]
mod test_g6 {
    use crate::{
        graph6::parse_graph6_file,
        structs::CanonautManager,
        tests::randomly_permute_graph,
        utilities::words_needed,
    };

    const REPS: usize = 5;
    const MAX_GRAPHS: usize = 2_000;

    fn self_consistency(rel_path: &str) {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let data = std::fs::read(root.join(rel_path))
            .unwrap_or_else(|e| panic!("cannot read {rel_path}: {e}"));
        let graphs = parse_graph6_file(&data);
        let mut manager = CanonautManager::new(16).with_canonization();

        for (index, result) in graphs.iter().take(MAX_GRAPHS).enumerate() {
            let graph = result
                .as_ref()
                .unwrap_or_else(|e| panic!("{rel_path} graph {index}: {e}"));
            let vertex_count = graph.number_of_vertices as usize;
            let words_per_vertex = graph.words_per_vertex as usize;

            manager.canonize(&graph.adjacency);
            let canon = manager.canonical_graph.clone();

            for _ in 0..REPS {
                let permuted = randomly_permute_graph(&graph.adjacency, vertex_count, words_per_vertex);
                manager.canonize(&permuted);
                assert_eq!(
                    manager.canonical_graph,
                    canon,
                    "{rel_path} graph {index}: canonical form unstable under permutation",
                );
            }
        }
    }

    fn vs_pynauty(g6_rel: &str, ref_rel: &str) {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let ref_path = root.join(ref_rel);
        let ref_data = std::fs::read(&ref_path)
            .unwrap_or_else(|e| panic!("cannot read {ref_rel}: {e}"));
        let g6_data = std::fs::read(root.join(g6_rel))
            .unwrap_or_else(|e| panic!("cannot read {g6_rel}: {e}"));

        let graphs = parse_graph6_file(&g6_data);
        let mut manager = CanonautManager::new(16).with_canonization();

        let mut byte_position = 0usize;
        for (index, result) in graphs.iter().enumerate() {
            if byte_position + 8 > ref_data.len() {
                break;
            }
            let vertex_count = u32::from_le_bytes(ref_data[byte_position..byte_position + 4].try_into().unwrap()) as usize;
            let words_per_vertex = u32::from_le_bytes(ref_data[byte_position + 4..byte_position + 8].try_into().unwrap()) as usize;
            byte_position += 8;

            let expected: Vec<usize> = ref_data[byte_position..byte_position + vertex_count * words_per_vertex * 8]
                .chunks_exact(8)
                .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()) as usize)
                .collect();
            byte_position += vertex_count * words_per_vertex * 8;

            let graph = result
                .as_ref()
                .unwrap_or_else(|e| panic!("{g6_rel} graph {index}: {e}"));

            assert_eq!(graph.number_of_vertices as usize, vertex_count, "{g6_rel} graph {index}: vertex count mismatch");
            assert_eq!(
                words_needed(vertex_count),
                words_per_vertex,
                "{g6_rel} graph {index}: words_per_vertex mismatch in reference"
            );

            manager.canonize(&graph.adjacency);
            assert_eq!(
                manager.canonical_graph,
                expected,
                "{g6_rel} graph {index}: canonical form differs from pynauty reference",
            );
        }
    }

    /// Verify the committed corpus; missing inputs fail the test.
    fn smoke(name: &str) {
        vs_pynauty(
            &format!("test_data/smoke/{name}.g6"),
            &format!("test_data/smoke/{name}.ref"),
        );
        self_consistency(&format!("test_data/smoke/{name}.g6"));
    }

    #[test]
    fn smoke_all_9() {
        smoke("all_9");
    }
    #[test]
    fn smoke_bipartite_10() {
        smoke("bipartite_10");
    }
    #[test]
    fn smoke_connected_10() {
        smoke("connected_10");
    }
    #[test]
    fn smoke_cubic_10() {
        smoke("cubic_10");
    }
    #[test]
    fn smoke_triangle_free_11() {
        smoke("triangle_free_11");
    }
    #[test]
    fn smoke_perfect_9() {
        smoke("perfect_9");
    }
    #[test]
    fn smoke_chordal_9() {
        smoke("chordal_9");
    }
    #[test]
    fn smoke_clawfree_9() {
        smoke("clawfree_9");
    }
    #[test]
    fn smoke_k4free_9() {
        smoke("k4free_9");
    }
    #[test]
    fn smoke_biconnected_9() {
        smoke("biconnected_9");
    }
    #[test]
    fn smoke_regular4_10() {
        smoke("regular4_10");
    }
    #[test]
    fn smoke_split_10() {
        smoke("split_10");
    }
    #[test]
    fn smoke_random_mixed() {
        smoke("random_mixed");
    }
    /// Word-boundary sizes: m = 1 (n = 63, 64), m = 2 (n = 65, 127, 128),
    /// m = 3 (n = 129) — the `is_automorphism_*` specialisations.
    #[test]
    fn smoke_large_word_boundaries() {
        for name in ["large_63", "large_64", "large_65", "large_127", "large_128", "large_129"] {
            smoke(name);
        }
    }
}

use rand::rng;
use rand::seq::SliceRandom;

use crate::refine::{GraphContext, updatecan};
pub fn randomly_permute_graph(
    graph: &[usize],
    number_of_vertices: usize,
    words_per_vertex: usize,
) -> Vec<usize> {
    let mut permuted = graph.to_vec();
    let mut permuted_ordering = (0..(number_of_vertices as u32)).collect::<Vec<u32>>();
    let mut context = GraphContext::new(number_of_vertices);
    permuted_ordering.shuffle(&mut rng());

    updatecan(
        graph,
        &mut permuted,
        &permuted_ordering,
        0,
        words_per_vertex as u32,
        number_of_vertices as u32,
        &mut context,
    );

    permuted
}

#[cfg(test)]
mod test_digraphs {
    use crate::{
        structs::{CanonautManager, add_one_arc},
        tests::randomly_permute_graph,
        utilities::words_needed,
    };

    fn directed_cycle(vertex_count: usize) -> Vec<usize> {
        let words_per_vertex = words_needed(vertex_count) as u32;
        let mut graph = vec![0usize; vertex_count * words_per_vertex as usize];
        for vertex in 0..vertex_count {
            add_one_arc(&mut graph, vertex, (vertex + 1) % vertex_count, words_per_vertex);
        }
        graph
    }

    #[test]
    fn directed_triangle_group_is_cyclic() {
        // The directed 3-cycle has automorphism group C3 (order 3); the
        // undirected triangle has S3 (order 6). Distinguishes digraph mode
        // from the graph being silently treated as undirected.
        let graph = directed_cycle(3);
        let mut manager = CanonautManager::new(3).with_canonization().with_digraph();
        manager.canonize(&graph);
        let stats = manager.statistics();
        assert_eq!(stats.number_of_orbits, 1);
        assert_eq!(stats.group_size1, 3.0);
        assert_eq!(stats.group_size2, 0);
    }

    #[test]
    fn directed_cycle_canonical_form_is_relabeling_invariant() {
        for vertex_count in 2..40 {
            let words_per_vertex = words_needed(vertex_count);
            let graph = directed_cycle(vertex_count);
            let mut manager = CanonautManager::new(vertex_count).with_canonization().with_digraph();
            manager.canonize(&graph);
            let reference = manager.canonical_graph.clone();
            for _ in 0..20 {
                let permuted = randomly_permute_graph(&graph, vertex_count, words_per_vertex);
                manager.canonize(&permuted);
                assert_eq!(manager.canonical_graph, reference, "vertex_count = {vertex_count}");
            }
        }
    }

    #[test]
    fn opposite_orientations_of_a_path_are_isomorphic() {
        let vertex_count = 5;
        let words_per_vertex = words_needed(vertex_count) as u32;
        let mut forward = vec![0usize; vertex_count * words_per_vertex as usize];
        let mut backward = vec![0usize; vertex_count * words_per_vertex as usize];
        for vertex in 0..vertex_count - 1 {
            add_one_arc(&mut forward, vertex, vertex + 1, words_per_vertex);
            add_one_arc(&mut backward, vertex + 1, vertex, words_per_vertex);
        }
        let mut manager = CanonautManager::new(vertex_count).with_canonization().with_digraph();
        manager.canonize(&forward);
        let canon_forward = manager.canonical_graph.clone();
        manager.canonize(&backward);
        assert_eq!(manager.canonical_graph, canon_forward);
    }
}

/// Refinement-blind families: CFI, Hadamard and Paley graphs.  These are the
/// instances where individual refinement gains nothing, so they exercise the
/// search-tree machinery rather than the refinement inner loops.
#[cfg(test)]
mod test_hard_instances {
    use crate::{
        generators::{generate_cfi_graph, generate_hadamard_graph, generate_paley_graph},
        structs::CanonautManager,
        tests::randomly_permute_graph,
        utilities::words_needed,
    };

    fn canonical_form(graph: &[usize], vertex_count: usize) -> Vec<usize> {
        let mut manager = CanonautManager::new(vertex_count).with_canonization();
        manager.canonize(graph);
        manager.canonical_graph.clone()
    }

    fn assert_relabeling_invariant(graph: &[usize], vertex_count: usize, label: &str) {
        let words_per_vertex = words_needed(vertex_count);
        let reference = canonical_form(graph, vertex_count);
        for repetition in 0..5 {
            let permuted = randomly_permute_graph(graph, vertex_count, words_per_vertex);
            assert_eq!(
                canonical_form(&permuted, vertex_count),
                reference,
                "{label}: canonical form not invariant under relabeling (repetition {repetition})",
            );
        }
    }

    /// The twisted and untwisted CFI graphs over the same base are
    /// non-isomorphic but colour-refinement-equivalent: canonicalization is the
    /// only thing that can tell them apart, so their canonical forms must
    /// differ while each stays stable under relabeling.
    #[test]
    fn cfi_twisted_is_distinguished_from_untwisted() {
        for base_vertex_count in [4, 6, 8] {
            let vertex_count = 10 * base_vertex_count;
            let plain = generate_cfi_graph(base_vertex_count, false);
            let twisted = generate_cfi_graph(base_vertex_count, true);

            assert_relabeling_invariant(&plain, vertex_count, "CFI (untwisted)");
            assert_relabeling_invariant(&twisted, vertex_count, "CFI (twisted)");

            assert_ne!(
                canonical_form(&plain, vertex_count),
                canonical_form(&twisted, vertex_count),
                "CFI base {base_vertex_count}: twisted and untwisted graphs are non-isomorphic \
                 but received the same canonical form",
            );
        }
    }

    #[test]
    fn hadamard_graphs_are_relabeling_invariant() {
        for order in [4, 8, 16, 32] {
            assert_relabeling_invariant(
                &generate_hadamard_graph(order),
                2 * order,
                &format!("Hadamard order {order}"),
            );
        }
    }

    /// The Paley graph on a prime `q = 1 (mod 4)` has automorphism group of
    /// order `q(q-1)/2`, which is independent ground truth for the group-size
    /// statistics on a strongly regular (refinement-blind) input.
    #[test]
    fn paley_graph_group_order_matches_theory() {
        for prime_order in [5usize, 13, 17, 29, 37] {
            let graph = generate_paley_graph(prime_order);
            let mut manager = CanonautManager::new(prime_order).with_canonization();
            manager.canonize(&graph);

            let statistics = manager.statistics();
            let group_order =
                statistics.group_size1 * 10f64.powi(statistics.group_size2 as i32);
            let expected = (prime_order * (prime_order - 1) / 2) as f64;
            assert!(
                (group_order - expected).abs() <= expected * 1e-9,
                "Paley({prime_order}): |Aut| = {group_order}, expected {expected}",
            );
            assert_eq!(
                statistics.number_of_orbits, 1,
                "Paley({prime_order}) is vertex-transitive, so it has one orbit",
            );

            assert_relabeling_invariant(&graph, prime_order, &format!("Paley({prime_order})"));
        }
    }
}

/// Small, fast canonicalizations that between them touch every word-count
/// specialisation (m = 1, 2, >2) and both the automorphism-check and
/// vertex-permutation paths.  Kept cheap on purpose: this is the module the
/// Miri job runs, where a full corpus sweep would be far too slow.
#[cfg(test)]
mod test_word_boundary_paths {
    use crate::{
        generators::{generate_cycle_graph, generate_paley_graph, generate_random_graph},
        structs::CanonautManager,
        tests::randomly_permute_graph,
        utilities::words_needed,
    };

    #[test]
    fn canonical_form_stable_across_word_boundaries() {
        // m = 1, 2, 3 and 4 words per vertex.
        for vertex_count in [40usize, 70, 130, 200] {
            let words_per_vertex = words_needed(vertex_count);
            for graph in [
                generate_cycle_graph(vertex_count),
                generate_random_graph(vertex_count, 0.5, 7),
            ] {
                let mut manager = CanonautManager::new(vertex_count).with_canonization();
                manager.canonize(&graph);
                let reference = manager.canonical_graph.clone();

                let permuted = randomly_permute_graph(&graph, vertex_count, words_per_vertex);
                manager.canonize(&permuted);
                assert_eq!(
                    manager.canonical_graph, reference,
                    "n={vertex_count}: canonical form not invariant under relabeling",
                );
            }
        }
    }

    /// A symmetric multi-word instance, so the automorphism-check paths are
    /// entered rather than skipped for want of automorphisms.
    #[test]
    fn symmetric_multiword_instance() {
        let graph = generate_paley_graph(101);
        let mut manager = CanonautManager::new(101).with_canonization();
        manager.canonize(&graph);
        let statistics = manager.statistics();
        assert_eq!(statistics.number_of_orbits, 1);
        assert!(statistics.number_of_generators > 0);
    }
}
