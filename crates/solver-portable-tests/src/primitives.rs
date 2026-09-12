use canonaut::{
    generators::generate_cycle_graph,
    refine::is_automorphism,
    rng::RngState,
    structs::{BitSetOpsMSB0, schreier_arena::SchreierSims},
    support::{fmperm_safe, mark_partition_fixed_points, permute_set},
    utilities::{next_element, words_needed},
};
use serde_json::json;
use wasm_bindgen::prelude::*;

fn scan_checks(size: usize, source: &[usize], selected: &[usize]) {
    let word_count = u32::try_from(source.len()).unwrap();
    let mut cursor = -1;
    let mut actual = Vec::new();
    while let Some(next) = next_element(source, word_count, cursor) {
        assert!(next > cursor);
        actual.push(usize::try_from(next).unwrap());
        cursor = next;
    }
    assert_eq!(actual, selected);
    for start in 0..=size {
        assert_eq!(
            source.next_set_bit(start),
            selected.iter().copied().find(|&i| i >= start)
        );
    }
}

fn permutation_checks(size: usize) {
    let words = words_needed(size);
    let word_count = u32::try_from(words).unwrap();
    let vertex_count = u32::try_from(size).unwrap();
    let mut source = vec![0usize; words];
    let selected: Vec<_> = (0..size).filter(|i| i % 3 == 0 || *i == size - 1).collect();
    for &bit in &selected {
        source.set_bit(bit);
    }
    scan_checks(size, &source, &selected);
    let permutation: Vec<_> = (0..size)
        .map(|i| u32::try_from((i + 7) % size).unwrap())
        .collect();
    let mut target = vec![0usize; words];
    permute_set(&source, &mut target, word_count, &permutation);
    for bit in 0..size {
        assert_eq!(target.is_bit_set((bit + 7) % size), selected.contains(&bit));
    }
    let mut fixed = vec![0usize; words];
    let mut minima = fixed.clone();
    fmperm_safe(
        &permutation,
        &mut fixed,
        &mut minima,
        word_count,
        vertex_count,
    );
    let mut visited = vec![false; size];
    let mut expected_minima = vec![false; size];
    for start in 0..size {
        if visited[start] {
            continue;
        }
        expected_minima[start] = true;
        let mut at = start;
        while !visited[at] {
            visited[at] = true;
            at = permutation[at] as usize;
        }
    }
    for bit in 0..size {
        assert_eq!(fixed.is_bit_set(bit), permutation[bit] as usize == bit);
        assert_eq!(minima.is_bit_set(bit), expected_minima[bit]);
    }
    let labels: Vec<_> = (0..vertex_count).rev().collect();
    mark_partition_fixed_points(
        &labels,
        &vec![0; size],
        0,
        &mut fixed,
        &mut minima,
        word_count,
        vertex_count,
    );
    assert!((0..size).all(|bit| fixed.is_bit_set(bit) && minima.is_bit_set(bit)));
    let mut partition = vec![1; size];
    partition[size - 1] = 0;
    mark_partition_fixed_points(
        &labels,
        &partition,
        0,
        &mut fixed,
        &mut minima,
        word_count,
        vertex_count,
    );
    assert!((0..size).all(|bit| !fixed.is_bit_set(bit) && minima.is_bit_set(bit) == (bit == 0)));

    let cycle = generate_cycle_graph(size);
    assert!(is_automorphism(
        &cycle,
        &permutation,
        false,
        word_count,
        vertex_count
    ));
    let mut not_automorphism: Vec<_> = (0..vertex_count).collect();
    not_automorphism.swap(0, 1);
    assert!(!is_automorphism(
        &cycle,
        &not_automorphism,
        false,
        word_count,
        vertex_count
    ));
    let mut work_a = vec![0; size];
    let mut work_b = vec![0; size];
    let mut scratch = vec![0; words];
    for power in [0, 1, 5, 6, 19, 20, 131] {
        let mut actual: Vec<_> = (0..vertex_count).collect();
        SchreierSims::apply_perm(
            &mut actual,
            &permutation,
            power,
            &mut work_a,
            &mut work_b,
            &mut scratch,
        );
        for (index, &mapped) in actual.iter().enumerate() {
            assert_eq!(mapped as usize, (index + 7 * power as usize) % size);
        }
    }
}

/// Exercise all word-width specializations and full 64-bit random arithmetic.
/// # Panics
/// Panics when a trusted primitive differs from its direct reference operation.
#[wasm_bindgen]
pub fn qualify_primitives() -> String {
    let sizes = [31, 32, 33, 63, 64, 65, 127, 128, 129];
    for size in sizes {
        permutation_checks(size);
    }
    let mut generator = RngState::new();
    let expected = [
        8_932_985_056_925_012_148,
        5_710_300_428_094_272_059,
        18_342_510_866_933_518_593,
        14_303_636_270_573_868_250,
        542_381_058_189_297_533,
    ];
    for value in expected {
        assert_eq!(generator.next_random(), value);
    }
    json!({"sizes":sizes,"wordBits":usize::BITS,"rng":expected.map(|value|value.to_string())})
        .to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn word_boundaries_and_rng_match_direct_operations() {
        super::qualify_primitives();
    }
}
