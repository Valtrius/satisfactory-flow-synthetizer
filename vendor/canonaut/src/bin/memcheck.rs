// Memory measurement harness for the paper's memory-usage figure.
//
// Runs `iters` canonize() calls on a fixed-size graph so external tools
// (e.g. `/usr/bin/time -l`, which reports "maximum resident set size") can
// read off peak RSS for one (family, n) data point per process invocation.
//
// A counting global allocator additionally reports canonaut's own heap use,
// split into the buffers allocated while the manager is being constructed and
// anything allocated during the timed loop.  Peak RSS is dominated by fixed
// process overhead at small n, so the steady-state allocation counter is the
// measurement that actually shows that `CanonautManager` reuses its buffers.
//
// Usage: memcheck <family> <n> [iters=2000]
//   family: complete | cycle | path | star | random | cfi | cfi_twisted
//         | hadamard | paley
//   For the hard families `n` is the construction parameter: the base vertex
//   count for cfi (graph order 10n), the Hadamard order (graph order 2n), and
//   the prime order for paley (graph order n).

use canonaut::generators::{
    generate_cfi_graph, generate_complete_graph, generate_cycle_graph, generate_hadamard_graph,
    generate_paley_graph, generate_path_graph, generate_random_graph, generate_star_graph,
};
use canonaut::structs::CanonautManager;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Global allocator that counts allocations and bytes handed out.
struct CountingAllocator;

static ALLOCATION_COUNT: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        let live = LIVE_BYTES.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
        PEAK_LIVE_BYTES.fetch_max(live, Ordering::Relaxed);
        // SAFETY: `layout` is forwarded unchanged to the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: `pointer`/`layout` come from a matching `alloc` call.
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let family = args.get(1).expect("usage: memcheck <family> <n> [iters]");
    let parameter: usize = args.get(2).expect("usage: memcheck <family> <n> [iters]").parse().unwrap();
    let iterations: usize = args.get(3).map(|arg| arg.parse().unwrap()).unwrap_or(2000);

    // Each arm yields the graph and its vertex count; for the hard families the
    // command-line argument is the construction parameter, not the order.
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
        other => panic!(
            "unknown family '{other}' (expected complete|cycle|path|star|random|cfi|cfi_twisted|hadamard|paley)"
        ),
    };

    let mut manager = CanonautManager::new(vertex_count).with_canonization();

    // One warm-up call so that any capacity growth triggered by the first
    // canonicalization is attributed to set-up rather than to the steady state.
    manager.canonize(black_box(&graph));

    let setup_allocations = ALLOCATION_COUNT.load(Ordering::Relaxed);
    let setup_bytes = ALLOCATED_BYTES.load(Ordering::Relaxed);

    let mut orbit_sum: u64 = 0;
    for _ in 0..iterations {
        manager.canonize(black_box(&graph));
        orbit_sum += manager.statistics().number_of_orbits as u64;
    }

    let steady_state_allocations = ALLOCATION_COUNT.load(Ordering::Relaxed) - setup_allocations;
    let steady_state_bytes = ALLOCATED_BYTES.load(Ordering::Relaxed) - setup_bytes;

    // `orbit_sum` is printed so the loop cannot be optimized away.
    println!(
        "family={family} n={vertex_count} iters={iterations} orbit_sum={orbit_sum} \
         setup_allocations={setup_allocations} setup_bytes={setup_bytes} \
         steady_state_allocations={steady_state_allocations} steady_state_bytes={steady_state_bytes} \
         peak_live_bytes={}",
        PEAK_LIVE_BYTES.load(Ordering::Relaxed)
    );
}
