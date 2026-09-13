//! Parser for the graph6 encoding (Brendan McKay, nauty).
//!
//! graph6 encodes undirected simple graphs. Encoding:
//!   - n ≤ 62:       1 byte  = n + 63
//!   - n ≤ 258047:   byte 126, then 3 bytes each = 6-bit chunk + 63
//!   - larger n:     bytes 126,126, then 6 bytes each = 6-bit chunk + 63
//!
//! Adjacency: upper triangle (j=1..n-1, i=0..j-1), LSB-right packed 6 bits/byte,
//! each byte stored as value + 63.

use crate::structs::graph::{add_one_edge, DenseGraph};
use crate::utilities::words_needed;

#[derive(Debug, PartialEq)]
pub enum Graph6Error {
    EmptyInput,
    NotGraph6Format,
    InvalidByte(u8),
    UnexpectedEnd,
}

impl std::fmt::Display for Graph6Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "empty input"),
            Self::NotGraph6Format => write!(f, "not graph6 (sparse6/digraph6 not supported)"),
            Self::InvalidByte(b) => write!(f, "invalid graph6 byte: {b}"),
            Self::UnexpectedEnd => write!(f, "unexpected end of data"),
        }
    }
}

impl std::error::Error for Graph6Error {}

/// Parse one graph6 string (bytes, no trailing newline required) into a `DenseGraph`.
pub fn parse_graph6(input: &[u8]) -> Result<DenseGraph, Graph6Error> {
    // Strip optional `>>graph6<<` header
    let input = input.strip_prefix(b">>graph6<<").unwrap_or(input);

    // Strip trailing CR/LF
    let input = match input.iter().rposition(|&byte| byte != b'\n' && byte != b'\r') {
        Some(last) => &input[..=last],
        None => return Err(Graph6Error::EmptyInput),
    };

    if input.is_empty() {
        return Err(Graph6Error::EmptyInput);
    }

    // Reject sparse6 (':') and digraph6 ('&')
    match input[0] {
        b':' | b'&' => return Err(Graph6Error::NotGraph6Format),
        _ => {}
    }

    let mut byte_position = 0usize;

    // Decode vertex_count
    let vertex_count: usize = if input[byte_position] != 126 {
        let header_byte = input[byte_position];
        if header_byte < 63 {
            return Err(Graph6Error::InvalidByte(header_byte));
        }
        byte_position += 1;
        (header_byte - 63) as usize
    } else if byte_position + 1 < input.len() && input[byte_position + 1] != 126 {
        // 4-byte encoding: 126 + 3 data bytes
        if byte_position + 4 > input.len() {
            return Err(Graph6Error::UnexpectedEnd);
        }
        let decoded_count = ((input[byte_position + 1] as usize - 63) << 12)
            | ((input[byte_position + 2] as usize - 63) << 6)
            | (input[byte_position + 3] as usize - 63);
        byte_position += 4;
        decoded_count
    } else {
        // 8-byte encoding: 126,126 + 6 data bytes
        if byte_position + 8 > input.len() {
            return Err(Graph6Error::UnexpectedEnd);
        }
        let decoded_count = ((input[byte_position + 2] as usize - 63) << 30)
            | ((input[byte_position + 3] as usize - 63) << 24)
            | ((input[byte_position + 4] as usize - 63) << 18)
            | ((input[byte_position + 5] as usize - 63) << 12)
            | ((input[byte_position + 6] as usize - 63) << 6)
            | (input[byte_position + 7] as usize - 63);
        byte_position += 8;
        decoded_count
    };

    let words_per_vertex = words_needed(vertex_count) as u32;
    let mut adjacency = vec![0usize; vertex_count * words_per_vertex as usize];

    let adjacency_data = &input[byte_position..];
    let mut bit_index = 0usize;

    // Upper triangle: column from 1..vertex_count, row from 0..column
    for column in 1..vertex_count {
        for row in 0..column {
            let data_byte_index = bit_index / 6;
            let bit_offset_in_byte = 5 - (bit_index % 6); // MSB-first within each 6-bit group

            if data_byte_index < adjacency_data.len() {
                let raw_byte = adjacency_data[data_byte_index];
                if !(63..=126).contains(&raw_byte) {
                    return Err(Graph6Error::InvalidByte(raw_byte));
                }
                if ((raw_byte - 63) >> bit_offset_in_byte) & 1 == 1 {
                    add_one_edge(&mut adjacency, row, column, words_per_vertex);
                }
            }
            // Bytes beyond adjacency_data.len() are implicitly zero (no edge)

            bit_index += 1;
        }
    }

    Ok(DenseGraph {
        adjacency,
        number_of_vertices: vertex_count as u32,
        words_per_vertex,
        directed: false,
        colors: None,
    })
}

/// Parse all graphs from a graph6 file (one graph per line).
pub fn parse_graph6_file(contents: &[u8]) -> Vec<Result<DenseGraph, Graph6Error>> {
    contents
        .split(|&byte| byte == b'\n')
        .filter(|line| !line.is_empty() && *line != b"\r")
        .map(parse_graph6)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structs::graph::get_graph_row;
    use crate::structs::BitSetOpsMSB0;

    fn edge(graph: &DenseGraph, row: usize, column: usize) -> bool {
        get_graph_row(&graph.adjacency, row, graph.words_per_vertex).is_bit_set(column)
    }

    fn edge_count(graph: &DenseGraph) -> usize {
        let vertex_count = graph.number_of_vertices as usize;
        (0..vertex_count)
            .flat_map(|row| (row + 1..vertex_count).map(move |column| (row, column)))
            .filter(|&(row, column)| edge(graph, row, column))
            .count()
    }

    // --- n encoding ---

    #[test]
    fn empty_graph_n0() {
        // '?' = 63 = n + 63 → n=0
        let graph = parse_graph6(b"?").unwrap();
        assert_eq!(graph.number_of_vertices, 0);
        assert!(graph.adjacency.is_empty());
    }

    #[test]
    fn single_vertex_n1() {
        // '@' = 64 → n=1, no adjacency bits
        let graph = parse_graph6(b"@").unwrap();
        assert_eq!(graph.number_of_vertices, 1);
        assert_eq!(edge_count(&graph), 0);
    }

    // --- small well-known graphs ---

    #[test]
    fn k2_complete() {
        // 'A' → n=2; '_' = 95, 95-63=32=0b100000, bit5=1 → edge(0,1)
        let graph = parse_graph6(b"A_").unwrap();
        assert_eq!(graph.number_of_vertices, 2);
        assert!(edge(&graph, 0, 1));
        assert!(edge(&graph, 1, 0));
        assert_eq!(edge_count(&graph), 1);
    }

    #[test]
    fn empty_n2() {
        // 'A' → n=2; '?' = 63, bits=0 → no edges
        let graph = parse_graph6(b"A?").unwrap();
        assert_eq!(graph.number_of_vertices, 2);
        assert!(!edge(&graph, 0, 1));
        assert_eq!(edge_count(&graph), 0);
    }

    #[test]
    fn k3_complete() {
        // 'B' → n=3; 'w' = 119, 119-63=56=0b111000 → all 3 edges set
        let graph = parse_graph6(b"Bw").unwrap();
        assert_eq!(graph.number_of_vertices, 3);
        assert!(edge(&graph, 0, 1));
        assert!(edge(&graph, 0, 2));
        assert!(edge(&graph, 1, 2));
        assert_eq!(edge_count(&graph), 3);
    }

    #[test]
    fn path_p3() {
        // 'B' → n=3; 'g' = 103, 103-63=40=0b101000 → edges (0,1) and (1,2)
        let graph = parse_graph6(b"Bg").unwrap();
        assert_eq!(graph.number_of_vertices, 3);
        assert!(edge(&graph, 0, 1));
        assert!(!edge(&graph, 0, 2));
        assert!(edge(&graph, 1, 2));
        assert_eq!(edge_count(&graph), 2);
    }

    #[test]
    fn k4_complete() {
        // 'C' → n=4; '~' = 126, 126-63=63=0b111111 → all 6 edges
        let graph = parse_graph6(b"C~").unwrap();
        assert_eq!(graph.number_of_vertices, 4);
        for row in 0..4 {
            for column in 0..4 {
                assert_eq!(edge(&graph, row, column), row != column, "K4 edge({row},{column})");
            }
        }
        assert_eq!(edge_count(&graph), 6);
    }

    #[test]
    fn cycle_c4() {
        // 'C' → n=4; 'l' = 108, 108-63=45=0b101101
        // bits: (0,1)=1, (0,2)=0, (1,2)=1, (0,3)=1, (1,3)=0, (2,3)=1
        // edges: 0-1, 1-2, 0-3, 2-3  → 4-cycle 0-1-2-3-0
        let graph = parse_graph6(b"Cl").unwrap();
        assert_eq!(graph.number_of_vertices, 4);
        assert!(edge(&graph, 0, 1));
        assert!(!edge(&graph, 0, 2));
        assert!(edge(&graph, 1, 2));
        assert!(edge(&graph, 0, 3));
        assert!(!edge(&graph, 1, 3));
        assert!(edge(&graph, 2, 3));
        assert_eq!(edge_count(&graph), 4);
    }

    #[test]
    fn undirected_symmetry() {
        // Every parsed graph must have a symmetric adjacency matrix
        for g6_string in [b"A_".as_ref(), b"Bw", b"Bg", b"C~", b"Cl"] {
            let graph = parse_graph6(g6_string).unwrap();
            let vertex_count = graph.number_of_vertices as usize;
            for row in 0..vertex_count {
                for column in 0..vertex_count {
                    assert_eq!(
                        edge(&graph, row, column),
                        edge(&graph, column, row),
                        "symmetry failed for {:?} at ({row},{column})",
                        std::str::from_utf8(g6_string)
                    );
                }
            }
        }
    }

    #[test]
    fn header_stripped() {
        let with_header = b">>graph6<<A_";
        let without = b"A_";
        let parsed_with_header = parse_graph6(with_header).unwrap();
        let parsed_without_header = parse_graph6(without).unwrap();
        assert_eq!(parsed_with_header.number_of_vertices, parsed_without_header.number_of_vertices);
        assert_eq!(parsed_with_header.adjacency, parsed_without_header.adjacency);
    }

    #[test]
    fn trailing_newline_stripped() {
        let with_newline = parse_graph6(b"Bw\n").unwrap();
        let without_newline = parse_graph6(b"Bw").unwrap();
        assert_eq!(with_newline.adjacency, without_newline.adjacency);
    }

    #[test]
    fn multiline_file() {
        // Three graphs: n=2 empty, K2, K3
        let file_contents = b"A?\nA_\nBw\n";
        let results = parse_graph6_file(file_contents);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].as_ref().unwrap().number_of_vertices, 2);
        assert_eq!(edge_count(results[0].as_ref().unwrap()), 0);
        assert_eq!(edge_count(results[1].as_ref().unwrap()), 1);
        assert_eq!(edge_count(results[2].as_ref().unwrap()), 3);
    }

    // --- error cases ---

    #[test]
    fn err_empty() {
        assert_eq!(parse_graph6(b""), Err(Graph6Error::EmptyInput));
        assert_eq!(parse_graph6(b"\n"), Err(Graph6Error::EmptyInput));
    }

    #[test]
    fn err_sparse6_prefix() {
        assert_eq!(
            parse_graph6(b":Bw"),
            Err(Graph6Error::NotGraph6Format)
        );
    }

    #[test]
    fn err_digraph6_prefix() {
        assert_eq!(
            parse_graph6(b"&Bw"),
            Err(Graph6Error::NotGraph6Format)
        );
    }

    #[test]
    fn err_invalid_byte() {
        // 'B' → n=3; space (0x20=32) is < 63 → invalid adjacency byte
        assert_eq!(
            parse_graph6(b"B\x20"),
            Err(Graph6Error::InvalidByte(0x20))
        );
    }
}
