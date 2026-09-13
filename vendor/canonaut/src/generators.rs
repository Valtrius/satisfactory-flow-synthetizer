//! Deterministic graph generators used by the test suite, the Criterion
//! benchmarks (`benches/bench.rs`), and the measurement binaries
//! (`src/bin/memcheck.rs`, `src/bin/profile.rs`).
//!
//! Every generator returns a flat MSB-0 bit-matrix (`n * m` `usize` words,
//! `m = ceil(n / 64)`) in the layout expected by
//! [`CanonautManager::canonize`](crate::structs::CanonautManager::canonize),
//! so the same buffer can be handed unchanged to the C reference
//! implementation in the comparison benchmarks.

use crate::structs::graph::add_one_edge;
use crate::utilities::words_needed;

pub fn generate_empty_graph(number_of_vertices: usize) -> Vec<usize> {
    vec![0; words_needed(number_of_vertices) * number_of_vertices]
}

pub fn generate_cycle_graph(number_of_vertices: usize) -> Vec<usize> {
    let mut graph = generate_empty_graph(number_of_vertices);
    if number_of_vertices == 1 {
        return graph;
    }
    for index in 0..number_of_vertices {
        add_one_edge(
            &mut graph,
            index,
            (index + 1) % number_of_vertices,
            words_needed(number_of_vertices) as u32,
        );
    }
    graph
}

pub fn generate_complete_graph(number_of_vertices: usize) -> Vec<usize> {
    let mut graph = generate_empty_graph(number_of_vertices);
    let words = words_needed(number_of_vertices) as u32;

    for row in 0..number_of_vertices {
        for column in (row + 1)..number_of_vertices {
            add_one_edge(&mut graph, row, column, words);
        }
    }
    graph
}

pub fn generate_path_graph(number_of_vertices: usize) -> Vec<usize> {
    let mut graph = generate_empty_graph(number_of_vertices);
    let words = words_needed(number_of_vertices) as u32;

    if number_of_vertices < 2 {
        return graph;
    }

    for index in 0..(number_of_vertices - 1) {
        add_one_edge(&mut graph, index, index + 1, words);
    }
    graph
}

pub fn generate_random_graph(number_of_vertices: usize, density: f64, seed: u64) -> Vec<usize> {
    use rand::{SeedableRng, rngs::SmallRng};
    let mut graph = generate_empty_graph(number_of_vertices);
    let words = words_needed(number_of_vertices) as u32;
    let mut random_generator = SmallRng::seed_from_u64(seed);
    for row in 0..number_of_vertices {
        for column in (row + 1)..number_of_vertices {
            use rand::Rng;
            if random_generator.random::<f64>() < density {
                add_one_edge(&mut graph, row, column, words);
            }
        }
    }
    graph
}

pub fn generate_star_graph(number_of_vertices: usize) -> Vec<usize> {
    let mut graph = generate_empty_graph(number_of_vertices);
    let words = words_needed(number_of_vertices) as u32;

    if number_of_vertices < 2 {
        return graph;
    }

    for index in 1..number_of_vertices {
        add_one_edge(&mut graph, 0, index, words);
    }
    graph
}

// ---------------------------------------------------------------------------
// Hard instances
//
// The families above are all solved with little or no backtracking.  The three
// constructions below are the standard stress cases for refinement-based
// canonicalization: individual refinement gains nothing on them, so runtime is
// dominated by the search tree rather than by the refinement inner loops.
// All three are generated deterministically here — no external data files.
// ---------------------------------------------------------------------------

/// Base graph for [`generate_cfi_graph`]: the 3-regular circulant on
/// `vertex_count` vertices (a cycle plus its long diagonals).  `vertex_count`
/// must be even and at least 4.
fn cubic_circulant_base(vertex_count: usize) -> Vec<Vec<usize>> {
    assert!(vertex_count >= 4 && vertex_count % 2 == 0, "base must be even and >= 4");
    let mut neighbors = vec![Vec::new(); vertex_count];
    let half = vertex_count / 2;
    for vertex in 0..vertex_count {
        let next = (vertex + 1) % vertex_count;
        if !neighbors[vertex].contains(&next) {
            neighbors[vertex].push(next);
            neighbors[next].push(vertex);
        }
        let opposite = (vertex + half) % vertex_count;
        if !neighbors[vertex].contains(&opposite) {
            neighbors[vertex].push(opposite);
            neighbors[opposite].push(vertex);
        }
    }
    for list in &mut neighbors {
        list.sort_unstable();
    }
    neighbors
}

/// Cai--F\"urer--Immerman graph over the 3-regular circulant on
/// `base_vertex_count` vertices, on `10 * base_vertex_count` vertices.
///
/// Each base vertex becomes a gadget of six *outer* vertices (a pair per
/// incident edge) and four *middle* vertices, one per even-size subset of the
/// three incident edges; each base edge joins the corresponding outer pairs in
/// parallel, or crossed when `twisted` is set.  The twisted and untwisted
/// graphs are non-isomorphic but indistinguishable by colour refinement, which
/// is what makes them expensive: every distinction has to be found by
/// branching.  See Cai, F\"urer and Immerman, *Combinatorica* 12 (1992) 389.
pub fn generate_cfi_graph(base_vertex_count: usize, twisted: bool) -> Vec<usize> {
    let base = cubic_circulant_base(base_vertex_count);
    let gadget_size = 10; // 6 outer + 4 middle
    let vertex_count = base_vertex_count * gadget_size;
    let mut graph = generate_empty_graph(vertex_count);
    let words = words_needed(vertex_count) as u32;

    // Gadget layout, per base vertex `v` at offset `10 * v`:
    //   outer(edge_index, bit) = 2 * edge_index + bit   (0..6)
    //   middle(subset_index)   = 6 + subset_index       (6..10)
    let outer = |base_vertex: usize, edge_index: usize, bit: usize| {
        gadget_size * base_vertex + 2 * edge_index + bit
    };
    let middle = |base_vertex: usize, subset_index: usize| gadget_size * base_vertex + 6 + subset_index;

    // Even-size subsets of {0,1,2}, as bitmasks.
    const EVEN_SUBSETS: [usize; 4] = [0b000, 0b011, 0b101, 0b110];

    for base_vertex in 0..base_vertex_count {
        for (subset_index, subset) in EVEN_SUBSETS.iter().enumerate() {
            for edge_index in 0..3 {
                let bit = (subset >> edge_index) & 1;
                add_one_edge(
                    &mut graph,
                    middle(base_vertex, subset_index),
                    outer(base_vertex, edge_index, bit),
                    words,
                );
            }
        }
    }

    // One designated base edge carries the twist; the rest are parallel.
    let mut twisted_edge_used = false;
    for base_vertex in 0..base_vertex_count {
        for (edge_index, &base_neighbor) in base[base_vertex].iter().enumerate() {
            if base_neighbor < base_vertex {
                continue; // handle each base edge once
            }
            let neighbor_edge_index = base[base_neighbor]
                .iter()
                .position(|&other| other == base_vertex)
                .expect("base adjacency is symmetric");
            let cross = twisted && !twisted_edge_used;
            twisted_edge_used |= cross;
            for bit in 0..2 {
                let other_bit = if cross { 1 - bit } else { bit };
                add_one_edge(
                    &mut graph,
                    outer(base_vertex, edge_index, bit),
                    outer(base_neighbor, neighbor_edge_index, other_bit),
                    words,
                );
            }
        }
    }

    graph
}

/// Bipartite graph of the Sylvester Hadamard matrix of order `order`
/// (a power of two), on `2 * order` vertices: row `i` is adjacent to column
/// `j` whenever `H[i][j] = +1`.
///
/// Hadamard graphs are regular, have a large automorphism group and are
/// refinement-blind, and appear as the `had-*` family in the benchmark sets
/// distributed with nauty/Traces and bliss.
pub fn generate_hadamard_graph(order: usize) -> Vec<usize> {
    assert!(order.is_power_of_two() && order >= 2, "Hadamard order must be a power of two");
    let vertex_count = 2 * order;
    let mut graph = generate_empty_graph(vertex_count);
    let words = words_needed(vertex_count) as u32;

    // Sylvester construction: H[i][j] = +1 iff popcount(i & j) is even.
    for row in 0..order {
        for column in 0..order {
            if (row & column).count_ones() % 2 == 0 {
                add_one_edge(&mut graph, row, order + column, words);
            }
        }
    }
    graph
}

/// Paley graph on `prime_order` vertices: `i ~ j` iff `i - j` is a non-zero
/// quadratic residue modulo `prime_order`, which must be a prime congruent to
/// 1 modulo 4.
///
/// Paley graphs are strongly regular, so every vertex looks identical to
/// colour refinement (all degrees, and all common-neighbour counts, coincide);
/// the entire canonical form has to be established by branching.  The
/// automorphism group has order `prime_order * (prime_order - 1) / 2`, which
/// makes the family useful as ground truth as well as a stress case.
pub fn generate_paley_graph(prime_order: usize) -> Vec<usize> {
    assert!(prime_order % 4 == 1, "Paley graphs need a prime order = 1 (mod 4)");
    let mut is_residue = vec![false; prime_order];
    for value in 1..prime_order {
        is_residue[(value * value) % prime_order] = true;
    }

    let mut graph = generate_empty_graph(prime_order);
    let words = words_needed(prime_order) as u32;
    for first in 0..prime_order {
        for second in (first + 1)..prime_order {
            if is_residue[(second - first) % prime_order] {
                add_one_edge(&mut graph, first, second, words);
            }
        }
    }
    graph
}
