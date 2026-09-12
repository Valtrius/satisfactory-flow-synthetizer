// Side-by-side search statistics for canonaut and C nauty on the same graph.
//
// Wall-clock comparisons say which implementation is faster; this says whether
// the difference comes from the search (different numbers of tree nodes) or
// from the per-node cost (same tree, different speed).
//
// Build with the same feature as the comparison benchmarks:
//   cargo run --release --features c-nauty-bench --bin compare_stats -- <family> <n> [iters]

use canonaut::generators::{
    generate_cfi_graph, generate_complete_graph, generate_cycle_graph, generate_hadamard_graph,
    generate_paley_graph, generate_path_graph, generate_random_graph, generate_star_graph,
};
use canonaut::structs::CanonautManager;
use canonaut::utilities::words_needed;
use std::time::Instant;

unsafe extern "C" {
    fn c_nauty_dense_canonize_stats(
        graph: *mut u64,
        labeling: *mut i32,
        partition: *mut i32,
        orbits: *mut i32,
        canonical_graph: *mut u64,
        words_per_vertex: i32,
        vertex_count: i32,
        group_size1: *mut f64,
        group_size2: *mut i32,
        num_orbits: *mut i32,
        num_generators: *mut i32,
        num_nodes: *mut i64,
        num_bad_leaves: *mut i64,
        max_level: *mut i32,
        canon_updates: *mut i64,
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let family = args.get(1).expect("usage: compare_stats <family> <n> [iters]");
    let parameter: usize = args.get(2).expect("usage: compare_stats <family> <n> [iters]").parse().unwrap();
    let iterations: usize = args.get(3).map(|a| a.parse().unwrap()).unwrap_or(3);

    let (graph, vertex_count) = match family.as_str() {
        "cfi" => (generate_cfi_graph(parameter, false), 10 * parameter),
        "cfi_twisted" => (generate_cfi_graph(parameter, true), 10 * parameter),
        "hadamard" => (generate_hadamard_graph(parameter), 2 * parameter),
        "paley" => (generate_paley_graph(parameter), parameter),
        "complete" => (generate_complete_graph(parameter), parameter),
        "cycle" => (generate_cycle_graph(parameter), parameter),
        "path" => (generate_path_graph(parameter), parameter),
        "star" => (generate_star_graph(parameter), parameter),
        "random" => (generate_random_graph(parameter, 0.5, 42), parameter),
        other => panic!("unknown family '{other}'"),
    };
    let words_per_vertex = words_needed(vertex_count);

    // canonaut
    let mut manager = CanonautManager::new(vertex_count).with_canonization();
    manager.canonize(&graph);
    let rust_statistics = *manager.statistics();
    let start = Instant::now();
    for _ in 0..iterations {
        manager.canonize(&graph);
    }
    let rust_time = start.elapsed().as_secs_f64() / iterations as f64;

    // C nauty
    let mut labeling = vec![0i32; vertex_count];
    let mut partition = vec![0i32; vertex_count];
    let mut orbits = vec![0i32; vertex_count];
    let mut canonical_graph = vec![0u64; vertex_count * words_per_vertex];
    let (mut group_size1, mut group_size2) = (0f64, 0i32);
    let (mut num_orbits, mut num_generators) = (0i32, 0i32);
    let (mut num_nodes, mut num_bad_leaves, mut canon_updates) = (0i64, 0i64, 0i64);
    let mut max_level = 0i32;

    let mut call_c_nauty = || unsafe {
        c_nauty_dense_canonize_stats(
            graph.as_ptr() as *mut u64,
            labeling.as_mut_ptr(),
            partition.as_mut_ptr(),
            orbits.as_mut_ptr(),
            canonical_graph.as_mut_ptr(),
            words_per_vertex as i32,
            vertex_count as i32,
            &mut group_size1,
            &mut group_size2,
            &mut num_orbits,
            &mut num_generators,
            &mut num_nodes,
            &mut num_bad_leaves,
            &mut max_level,
            &mut canon_updates,
        )
    };
    call_c_nauty();
    let start = Instant::now();
    for _ in 0..iterations {
        call_c_nauty();
    }
    let c_time = start.elapsed().as_secs_f64() / iterations as f64;

    println!("{family}({parameter}): n={vertex_count}, m={words_per_vertex}");
    println!(
        "  {:>12} {:>12} {:>12} {:>10} {:>10} {:>10} {:>12}",
        "impl", "time_ms", "nodes", "bad_leaves", "max_level", "gens", "canon_updates"
    );
    println!(
        "  {:>12} {:>12.3} {:>12} {:>10} {:>10} {:>10} {:>12}",
        "canonaut",
        rust_time * 1e3,
        rust_statistics.number_of_nodes,
        rust_statistics.number_of_bad_leaves,
        rust_statistics.maximum_search_depth,
        rust_statistics.number_of_generators,
        rust_statistics.canonical_updates,
    );
    println!(
        "  {:>12} {:>12.3} {:>12} {:>10} {:>10} {:>10} {:>12}",
        "nauty", c_time * 1e3, num_nodes, num_bad_leaves, max_level, num_generators, canon_updates,
    );
    println!(
        "  speedup {:.2}x   |Aut| canonaut {:.4}e{}, nauty {:.4}e{}   orbits {} vs {}",
        c_time / rust_time,
        rust_statistics.group_size1,
        rust_statistics.group_size2,
        group_size1,
        group_size2,
        rust_statistics.number_of_orbits,
        num_orbits,
    );
}
