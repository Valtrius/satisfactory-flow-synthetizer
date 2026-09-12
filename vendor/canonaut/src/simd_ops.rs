//! Portable inner-loop primitives for nauty's hot paths.
//!
//! All operations treat slices as flat arrays of machine-word (usize) elements.
//! Small sizes (len ≤ 4) are handled with inlined unrolled fast paths that
//! compile to straight-line code.  Larger sizes use plain scalar loops; with
//! target-cpu=native LLVM auto-vectorizes these — vcntq_u8 on aarch64 for
//! and_popcount, 128-bit NEON AND/OR for the rest.

/// Computes `sum_i popcount(left[i] & right[i])` over `word_count` words.
///
/// Fast paths for word_count ≤ 4 are inlined.  For word_count > 4 the plain loop lets LLVM
/// emit vcntq_u8 + horizontal reduction on aarch64 (same as C nauty's compiler).
#[inline(always)]
pub fn and_popcount(left: &[usize], right: &[usize], word_count: usize) -> u32 {
    match word_count {
        1 => (left[0] & right[0]).count_ones(),
        2 => (left[0] & right[0]).count_ones() + (left[1] & right[1]).count_ones(),
        3 => (left[0] & right[0]).count_ones() + (left[1] & right[1]).count_ones()
           + (left[2] & right[2]).count_ones(),
        4 => (left[0] & right[0]).count_ones() + (left[1] & right[1]).count_ones()
           + (left[2] & right[2]).count_ones() + (left[3] & right[3]).count_ones(),
        _ => {
            let mut popcount_sum = 0u32;
            for (&left_word, &right_word) in left[..word_count].iter().zip(right[..word_count].iter()) {
                popcount_sum += (left_word & right_word).count_ones();
            }
            popcount_sum
        }
    }
}

/// Computes `target[i] &= source[i]` for `word_count` words, in place.
#[inline(always)]
pub fn intersect_inplace(target: &mut [usize], source: &[usize], word_count: usize) {
    match word_count {
        1 => target[0] &= source[0],
        2 => { target[0] &= source[0]; target[1] &= source[1]; }
        3 => { target[0] &= source[0]; target[1] &= source[1]; target[2] &= source[2]; }
        4 => { target[0] &= source[0]; target[1] &= source[1]; target[2] &= source[2]; target[3] &= source[3]; }
        _ => {
            for (target_word, &source_word) in target[..word_count].iter_mut().zip(source[..word_count].iter()) {
                *target_word &= source_word;
            }
        }
    }
}

/// Returns `true` if `fixed_points` is **not** a subset of `other_fixed_points` over `word_count` words.
///
/// Equivalent to `any bit set in (fixed_points[i] & !other_fixed_points[i])`.
#[inline(always)]
pub fn andn_any(fixed_points: &[usize], other_fixed_points: &[usize], word_count: usize) -> bool {
    match word_count {
        1 => fixed_points[0] & !other_fixed_points[0] != 0,
        2 => (fixed_points[0] & !other_fixed_points[0]) | (fixed_points[1] & !other_fixed_points[1]) != 0,
        3 => (fixed_points[0] & !other_fixed_points[0]) | (fixed_points[1] & !other_fixed_points[1]) | (fixed_points[2] & !other_fixed_points[2]) != 0,
        4 => (fixed_points[0] & !other_fixed_points[0]) | (fixed_points[1] & !other_fixed_points[1]) | (fixed_points[2] & !other_fixed_points[2]) | (fixed_points[3] & !other_fixed_points[3]) != 0,
        _ => {
            let mut any_bits = 0usize;
            for (&fixed_word, &other_word) in fixed_points[..word_count].iter().zip(other_fixed_points[..word_count].iter()) {
                any_bits |= fixed_word & !other_word;
            }
            any_bits != 0
        }
    }
}

/// Computes `(any(mask_bits[i] & row[i]), any(mask_bits[i] & !row[i]))` over `word_count` words.
#[inline(always)]
pub fn or_reduce_masked(mask_bits: &[usize], row: &[usize], word_count: usize) -> (bool, bool) {
    match word_count {
        1 => ((mask_bits[0] & row[0]) != 0, (mask_bits[0] & !row[0]) != 0),
        2 => (
            (mask_bits[0] & row[0]) | (mask_bits[1] & row[1]) != 0,
            (mask_bits[0] & !row[0]) | (mask_bits[1] & !row[1]) != 0,
        ),
        3 => (
            (mask_bits[0] & row[0]) | (mask_bits[1] & row[1]) | (mask_bits[2] & row[2]) != 0,
            (mask_bits[0] & !row[0]) | (mask_bits[1] & !row[1]) | (mask_bits[2] & !row[2]) != 0,
        ),
        4 => (
            (mask_bits[0] & row[0]) | (mask_bits[1] & row[1]) | (mask_bits[2] & row[2]) | (mask_bits[3] & row[3]) != 0,
            (mask_bits[0] & !row[0]) | (mask_bits[1] & !row[1]) | (mask_bits[2] & !row[2]) | (mask_bits[3] & !row[3]) != 0,
        ),
        _ => {
            let mut any_and = 0usize;
            let mut any_andnot = 0usize;
            for (&mask_word, &row_word) in mask_bits[..word_count].iter().zip(row[..word_count].iter()) {
                any_and |= mask_word & row_word;
                any_andnot |= mask_word & !row_word;
            }
            (any_and != 0, any_andnot != 0)
        }
    }
}
