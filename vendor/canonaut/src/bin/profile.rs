// Profiling harness: runs canonicalization in a tight loop so flamegraph
// captures representative samples. Exercises three workloads:
//   1. Complete K_64  — maximum symmetry, large search tree
//   2. Random G(64,0.5) — asymmetric, short search tree, cache-friendly
//   3. Batch of mixed 9-vertex graphs from the graph6 corpus
//
// Run via:
//   cargo flamegraph --profile flamegraph --bin profile

// Workloads are toggled in main() depending on what is being profiled, so
// whichever ones are currently commented out show up as dead code.
#![allow(dead_code)]

use canonaut::{
    graph6::parse_graph6_file,
    structs::CanonautManager,
    generators::{generate_complete_graph, generate_random_graph, generate_star_graph},
    utilities::words_needed,
};
use std::hint::black_box;

const ITERATIONS: usize = 50_000;
const BATCH_ITERATIONS: usize = 200;
const MAX_G6_GRAPHS: usize = 1_000;

fn profile_complete() {
    let vertex_count = 64;
    let graph = generate_complete_graph(vertex_count);
    let mut manager = CanonautManager::new(vertex_count).with_canonization();
    for _ in 0..ITERATIONS {
        manager.canonize(black_box(&graph));
    }
}

fn profile_random() {
    let vertex_count = 64;
    let graph = generate_random_graph(vertex_count, 0.5, 42);
    let mut manager = CanonautManager::new(vertex_count).with_canonization();
    for _ in 0..ITERATIONS {
        manager.canonize(black_box(&graph));
    }
}

fn profile_g6_batch() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let data = std::fs::read(root.join("test_data/all_9.g6")).expect("read all_9.g6");
    let graphs: Vec<_> = parse_graph6_file(&data)
        .into_iter()
        .take(MAX_G6_GRAPHS)
        .filter_map(Result::ok)
        .collect();

    let max_vertex_count = graphs
        .iter()
        .map(|graph| graph.number_of_vertices as usize)
        .max()
        .unwrap_or(16);
    let mut manager = CanonautManager::new(words_needed(max_vertex_count)).with_canonization();

    for _ in 0..BATCH_ITERATIONS {
        for graph in black_box(&graphs) {
            manager.canonize(&graph.adjacency);
        }
    }
}

fn profile_star() {
    let vertex_count = 64;
    let graph = generate_star_graph(vertex_count);
    let mut manager = CanonautManager::new(vertex_count).with_canonization();
    for _ in 0..ITERATIONS {
        manager.canonize(black_box(&graph));
    }
}

fn profile_complete_128() {
    let vertex_count = 128;
    let graph = generate_complete_graph(vertex_count);
    let mut manager = CanonautManager::new(vertex_count).with_canonization();
    for _ in 0..3_000 {
        manager.canonize(black_box(&graph));
    }
}

fn profile_random_512() {
    let vertex_count = 512;
    let graph = generate_random_graph(vertex_count, 0.5, 42);
    let mut manager = CanonautManager::new(vertex_count).with_canonization();
    for _ in 0..20_000 {
        manager.canonize(black_box(&graph));
    }
}

fn profile_random_large(vertex_count: usize, iterations: usize) {
    let graph = generate_random_graph(vertex_count, 0.5, 42);
    let mut manager = CanonautManager::new(vertex_count).with_canonization();
    for _ in 0..iterations {
        manager.canonize(black_box(&graph));
    }
}

fn profile_complete_large(vertex_count: usize, iterations: usize) {
    let graph = generate_complete_graph(vertex_count);
    let mut manager = CanonautManager::new(vertex_count).with_canonization();
    for _ in 0..iterations {
        manager.canonize(black_box(&graph));
    }
}

fn main() {
    // Large-n scaling profile. Complete graphs at n>=1024 are excluded:
    // the automorphism search tree blows up exponentially there (a known
    // worst case for any nauty-family implementation, not specific to
    // this port), and the paper's own benchmarks never go past 512 for
    // that family. Random graphs scale ~linearly and are the realistic
    // large-n workload, so iteration counts are sized for several seconds of
    // sampling each based on measured per-call cost (1024: ~1.5ms,
    // 2048: ~4.8ms from target/criterion/scaling).
    profile_random_large(1024, 2000);
    profile_random_large(2048, 600);
}
