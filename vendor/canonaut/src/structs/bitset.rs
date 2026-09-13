use std::fmt::Write;

pub const WORDSIZE: usize = usize::BITS as usize;

/// Bitset operations using MSB-0 (most-significant-bit = index 0) ordering.
///
/// This matches nauty's internal convention: bit index `0` is stored in the
/// highest bit of word `0`, and bit index `64` is the highest bit of word `1`.
///
/// Implemented for `[usize]` (multi-word) and `usize` (single word).
pub trait BitSetOpsMSB0 {
    /// Returns `true` if bit `index` is set.
    fn is_bit_set(&self, index: usize) -> bool;

    /// Sets bit `index` to `1`.
    fn set_bit(&mut self, index: usize);

    /// Sets bit `index` to `0`.
    fn unset_bit(&mut self, index: usize);

    /// Returns the number of set bits (popcount).
    fn population_count(&self) -> u32;

    /// Sets all bits to `0`.
    fn clear(&mut self);

    /// Sets the first `limit` words to `0`; words beyond `limit` are untouched.
    fn clear_words(&mut self, limit: usize);

    /// Returns the index of the first set bit at or after `start_index`,
    /// or `None` if no such bit exists.
    fn next_set_bit(&self, start_index: usize) -> Option<usize>;

    /// Computes `self &= other` in place.  If `other` is shorter, excess words in
    /// `self` are zeroed.
    fn intersect_inplace(&mut self, other: &Self);

    /// Prints the bitset to stdout as a binary string with `_` word separators.
    fn print_bits(&self);

    /// Returns the bitset as a binary string with `_` word separators.
    fn to_bit_string(&self) -> String;
}

/// Display helpers for dense adjacency matrices stored as `[usize]`.
pub trait BitMatrixDisplay {
    /// Prints the adjacency matrix to stdout, one row per line, using `'0'`/`'1'` characters.
    fn print_matrix(&self, words_per_vertex: usize);
    /// Returns a multi-line string of the adjacency matrix (one row per line).
    fn to_bit_matrix(&self, words_per_vertex: usize) -> String;
}

// Implementation for any slice of usizes
impl BitSetOpsMSB0 for [usize] {
    #[inline(always)]
    fn is_bit_set(&self, index: usize) -> bool {
        let (word_index, mask) = split_index(index);
        // Use `get` to handle out-of-bounds gracefully
        match self.get(word_index) {
            Some(word) => (word & mask) != 0,
            None => false,
        }
    }

    #[inline(always)]
    fn set_bit(&mut self, index: usize) {
        let (word_index, mask) = split_index(index);
        if let Some(word) = self.get_mut(word_index) {
            *word |= mask;
        }
    }

    #[inline(always)]
    fn unset_bit(&mut self, index: usize) {
        let (word_index, mask) = split_index(index);
        if let Some(word) = self.get_mut(word_index) {
            *word &= !mask;
        }
    }

    fn clear(&mut self) {
        self.fill(0);
    }

    fn clear_words(&mut self, word_limit: usize) {
        if word_limit <= self.len() {
            self[..word_limit].fill(0);
        } else {
            self.fill(0);
        }
    }

    /// Finds the first set bit starting from `start_index` (scanning Left-to-Right)
    #[inline(always)]
    fn next_set_bit(&self, start_index: usize) -> Option<usize> {
        if start_index >= self.len() * usize::BITS as usize {
            return None;
        }

        let word_index = start_index / (usize::BITS as usize);
        let bit_offset = start_index % (usize::BITS as usize);

        // Check partial first word
        if let Some(&word) = self.get(word_index) {
            // MSB-0: shifting right by bit_offset clears bits before it.
            let mask = (!0usize) >> bit_offset;
            let masked_word = word & mask;

            if masked_word != 0 {
                return Some(
                    word_index * usize::BITS as usize + masked_word.leading_zeros() as usize,
                );
            }
        }

        // Scan remaining words
        for (word_offset, &word) in self.get(word_index + 1..)?.iter().enumerate() {
            if word != 0 {
                let found_word_index = word_index + 1 + word_offset;
                return Some(found_word_index * usize::BITS as usize + word.leading_zeros() as usize);
            }
        }

        None
    }

    fn print_bits(&self) {
        println!("{}", self.to_bit_string());
    }

    fn population_count(&self) -> u32 {
        self.iter().fold(0, |sum, subset| sum + subset.count_ones())
    }

    fn to_bit_string(&self) -> String {
        let capacity = self.len() * (WORDSIZE + 1); // Words + separators
        let mut result = String::with_capacity(capacity);

        for (word_position, &word) in self.iter().enumerate() {
            if word_position > 0 {
                result.push('_');
            }
            // Format as binary, padded with zeros to full width
            write!(&mut result, "{:0width$b}", word, width = WORDSIZE).unwrap();
        }
        result
    }

    fn intersect_inplace(&mut self, other: &[usize]) {
        let common_len = std::cmp::min(self.len(), other.len());

        // 1. Process common words
        for (self_word, &other_word) in self[..common_len].iter_mut().zip(&other[..common_len]) {
            *self_word &= other_word;
        }

        // // 2. Clear excess words if self is larger
        if self.len() > common_len {
            self[common_len..].fill(0);
        }
    }
}

impl BitMatrixDisplay for [usize] {
    fn print_matrix(&self, words_per_vertex: usize) {
        println!("{}", self.to_bit_matrix(words_per_vertex));
    }

    fn to_bit_matrix(&self, words_per_vertex: usize) -> String {
        if words_per_vertex == 0 || self.is_empty() {
            return String::new();
        }

        // Calculate the dimension of the square matrix (N x N)
        // self.len() is the total number of words.
        // N = Total Words / Words Per Vertex
        let num_vertices = self.len() / words_per_vertex;

        // Safety check to ensure the slice is consistent
        if self.len() % words_per_vertex != 0 {
            return format!(
                "Error: Slice length {} is not divisible by words_per_vertex {}",
                self.len(),
                words_per_vertex
            );
        }

        const BITS: usize = usize::BITS as usize;

        // Pre-allocate: (N bits + 1 newline) * N rows
        let mut result = String::with_capacity(num_vertices * (num_vertices + 1));

        for row in 0..num_vertices {
            // Newline between rows (but not before the first one)
            if row > 0 {
                result.push('\n');
            }

            // We iterate exactly N times for the columns to ignore padding bits
            for column in 0..num_vertices {
                // Optional: Visual separator every 64 bits for readability
                // (Only if it's not the start of the line)
                if column > 0 && column % BITS == 0 {
                    result.push('_');
                }

                // Calculate the location of the bit
                // 1. Jump to the start of the row (row * words_per_vertex)
                // 2. Add the word offset for the column (column / 64)
                let word_index = (row * words_per_vertex) + (column / BITS);

                // 3. Find the bit offset within that word (MSB-0)
                let bit_offset = column % BITS;
                let mask = 1usize << (BITS - 1 - bit_offset);

                let is_set = if let Some(&word) = self.get(word_index) {
                    (word & mask) != 0
                } else {
                    false
                };

                result.push(if is_set { '1' } else { '0' });
            }
        }

        result
    }
}

// Implementation for a single usize primitive
impl BitSetOpsMSB0 for usize {
    fn is_bit_set(&self, index: usize) -> bool {
        if index >= WORDSIZE {
            return false;
        }
        // MSB-0: Shift 1 to the position calculated from the LSB
        let mask = 1usize << (WORDSIZE - 1 - index);
        (*self & mask) != 0
    }

    fn set_bit(&mut self, index: usize) {
        if index < WORDSIZE {
            let mask = 1usize << (WORDSIZE - 1 - index);
            *self |= mask;
        }
    }

    fn unset_bit(&mut self, index: usize) {
        if index < WORDSIZE {
            let mask = 1usize << (WORDSIZE - 1 - index);
            *self &= !mask;
        }
    }

    fn population_count(&self) -> u32 {
        self.count_ones()
    }

    /// Sets the Bitset to 0 so it can be reused
    fn clear(&mut self) {
        *self = 0;
    }

    fn clear_words(&mut self, _word_limit: usize) {
        // Can't clear more than one words in a word
        *self = 0;
    }

    fn next_set_bit(&self, start_index: usize) -> Option<usize> {
        if start_index >= WORDSIZE {
            return None;
        }

        // Create a mask that keeps bits from start_index downwards.
        // Example: start_index = 2. We want to ignore bits 0 and 1.
        // (!0) >> 2 = 001111...
        let mask = (!0usize) >> start_index;
        let masked_word = *self & mask;

        if masked_word != 0 {
            // leading_zeros() returns the MSB-0 index directly
            Some(masked_word.leading_zeros() as usize)
        } else {
            None
        }
    }

    fn intersect_inplace(&mut self, other: &usize) {
        *self &= other
    }

    fn print_bits(&self) {
        println!("{}", self.to_bit_string());
    }

    fn to_bit_string(&self) -> String {
        format!("{:0width$b}", self, width = WORDSIZE)
    }
}

/// Converts a visual MSB-0 index into `(word_index, bitmask)`.
#[inline(always)]
fn split_index(index: usize) -> (usize, usize) {
    let word_index = index / (usize::BITS as usize);
    let bit_index = index % (usize::BITS as usize);

    // MSB-0 Logic:
    // If we want index 0 (Leftmost), we want the High Bit (1 << 63).
    // If we want index 63 (Rightmost), we want the Low Bit (1 << 0).
    // Shift = (63 - index)
    let shift = (usize::BITS as usize - 1) - bit_index;

    (word_index, 1 << shift)
}
