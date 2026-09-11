use serde::{Deserialize, Serialize};

/// High-performance Compressed Sparse Row (CSR) matrix representation.
///
/// Stores sparse directed graph edges in three contiguous arrays without pointer chasing,
/// maximizing CPU L1/L2 cache locality and SIMD vectorization capabilities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CsrMatrix {
    /// Pointers into `col_indices` and `weights` indicating the start of each row.
    /// Length is `num_rows + 1`.
    pub row_offsets: Vec<u32>,
    /// Column indices (destination node IDs) for each edge.
    pub col_indices: Vec<u32>,
    /// Edge weights / transition values corresponding to `col_indices`.
    pub weights: Vec<f32>,
    /// Total number of rows (source nodes).
    pub num_rows: usize,
    /// Total number of columns (destination nodes).
    pub num_cols: usize,
}

impl Default for CsrMatrix {
    fn default() -> Self {
        Self {
            row_offsets: vec![0],
            col_indices: Vec::new(),
            weights: Vec::new(),
            num_rows: 0,
            num_cols: 0,
        }
    }
}

impl CsrMatrix {
    /// Creates an empty CSR matrix with specified dimensions and zero edges.
    pub fn empty(num_rows: usize, num_cols: usize) -> Self {
        Self {
            row_offsets: vec![0; num_rows + 1],
            col_indices: Vec::new(),
            weights: Vec::new(),
            num_rows,
            num_cols,
        }
    }

    /// Creates a new `CsrMatrix` from a list of directed edges `(src, dst, weight)`.
    ///
    /// Automatically validates bounds, sorts edges by `(src, dst)`, and accumulates
    /// weights for duplicate parallel edges.
    pub fn from_edges(num_rows: usize, num_cols: usize, mut edges: Vec<(u32, u32, f32)>) -> Self {
        // Filter out out-of-bounds edges
        edges.retain(|&(src, dst, _)| (src as usize) < num_rows && (dst as usize) < num_cols);

        // Sort edges by source row first, then destination column
        edges.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

        let mut row_offsets = Vec::with_capacity(num_rows + 1);
        let mut col_indices = Vec::with_capacity(edges.len());
        let mut weights = Vec::with_capacity(edges.len());

        let mut current_edge_idx = 0;
        let mut deduplicated_edges: Vec<(u32, u32, f32)> = Vec::with_capacity(edges.len());

        // Deduplicate parallel edges by summing weights
        for edge in edges {
            if let Some(last) = deduplicated_edges.last_mut() {
                if last.0 == edge.0 && last.1 == edge.1 {
                    last.2 += edge.2;
                    continue;
                }
            }
            deduplicated_edges.push(edge);
        }

        for r in 0..num_rows {
            row_offsets.push(current_edge_idx as u32);
            while current_edge_idx < deduplicated_edges.len()
                && deduplicated_edges[current_edge_idx].0 as usize == r
            {
                col_indices.push(deduplicated_edges[current_edge_idx].1);
                weights.push(deduplicated_edges[current_edge_idx].2);
                current_edge_idx += 1;
            }
        }
        row_offsets.push(current_edge_idx as u32);

        Self {
            row_offsets,
            col_indices,
            weights,
            num_rows,
            num_cols,
        }
    }

    /// Returns the number of edges (non-zero entries) in the matrix.
    #[inline]
    pub fn num_edges(&self) -> usize {
        self.col_indices.len()
    }

    /// Returns the out-degree (number of outgoing edges) for a given row.
    #[inline]
    pub fn out_degree(&self, row: u32) -> usize {
        let r = row as usize;
        if r >= self.num_rows {
            return 0;
        }
        (self.row_offsets[r + 1] - self.row_offsets[r]) as usize
    }

    /// Returns a borrowed slice of target column indices (out-neighbors) for a given row.
    ///
    /// Performs zero heap allocations ($O(1)$ time).
    #[inline]
    pub fn neighbors(&self, row: u32) -> &[u32] {
        let r = row as usize;
        if r >= self.num_rows {
            return &[];
        }
        let start = self.row_offsets[r] as usize;
        let end = self.row_offsets[r + 1] as usize;
        &self.col_indices[start..end]
    }

    /// Returns a borrowed slice of edge weights for a given row.
    ///
    /// Performs zero heap allocations ($O(1)$ time).
    #[inline]
    pub fn weights(&self, row: u32) -> &[f32] {
        let r = row as usize;
        if r >= self.num_rows {
            return &[];
        }
        let start = self.row_offsets[r] as usize;
        let end = self.row_offsets[r + 1] as usize;
        &self.weights[start..end]
    }

    /// Returns both the out-neighbors and their corresponding weights simultaneously.
    #[inline]
    pub fn row_slice(&self, row: u32) -> (&[u32], &[f32]) {
        let r = row as usize;
        if r >= self.num_rows {
            return (&[], &[]);
        }
        let start = self.row_offsets[r] as usize;
        let end = self.row_offsets[r + 1] as usize;
        (&self.col_indices[start..end], &self.weights[start..end])
    }

    /// Creates a row-normalized copy of this matrix where each row's weights sum to 1.0.
    ///
    /// For rows with zero out-degree or zero total weight, weights remain unchanged.
    /// This produces the row-stochastic transition probability matrix $\mathbf{P} = \mathbf{D}^{-1}\mathbf{A}$.
    pub fn row_normalized(&self) -> Self {
        let mut normalized_weights = self.weights.clone();

        for r in 0..self.num_rows {
            let start = self.row_offsets[r] as usize;
            let end = self.row_offsets[r + 1] as usize;
            if start == end {
                continue;
            }

            let sum: f32 = normalized_weights[start..end].iter().sum();
            if sum > 0.0 {
                let inv_sum = 1.0 / sum;
                for w in &mut normalized_weights[start..end] {
                    *w *= inv_sum;
                }
            }
        }

        Self {
            row_offsets: self.row_offsets.clone(),
            col_indices: self.col_indices.clone(),
            weights: normalized_weights,
            num_rows: self.num_rows,
            num_cols: self.num_cols,
        }
    }

    /// Computes the transpose matrix $\mathbf{A}^T$, swapping source and destination.
    ///
    /// In the transposed matrix, out-edges correspond to the in-edges of the original graph,
    /// enabling backwards topological traversal and pull-based algorithms.
    pub fn transpose(&self) -> Self {
        let mut in_edges: Vec<(u32, u32, f32)> = Vec::with_capacity(self.num_edges());

        for r in 0..self.num_rows {
            let (cols, ws) = self.row_slice(r as u32);
            for (&c, &w) in cols.iter().zip(ws.iter()) {
                in_edges.push((c, r as u32, w));
            }
        }

        Self::from_edges(self.num_cols, self.num_rows, in_edges)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csr_construction_and_slicing() {
        // Graph with 4 nodes (0, 1, 2, 3)
        // 0 -> 1 (0.5), 0 -> 2 (1.5)
        // 1 -> 2 (2.0)
        // 2 -> (no out-edges)
        // 3 -> 0 (1.0), 3 -> 1 (3.0)
        let edges = vec![
            (0, 1, 0.5),
            (0, 2, 1.5),
            (1, 2, 2.0),
            (3, 0, 1.0),
            (3, 1, 3.0),
        ];

        let csr = CsrMatrix::from_edges(4, 4, edges);

        assert_eq!(csr.num_rows, 4);
        assert_eq!(csr.num_cols, 4);
        assert_eq!(csr.num_edges(), 5);

        // Node 0
        assert_eq!(csr.out_degree(0), 2);
        assert_eq!(csr.neighbors(0), &[1, 2]);
        assert_eq!(csr.weights(0), &[0.5, 1.5]);

        // Node 1
        assert_eq!(csr.out_degree(1), 1);
        assert_eq!(csr.neighbors(1), &[2]);
        assert_eq!(csr.weights(1), &[2.0]);

        // Node 2 (sink)
        assert_eq!(csr.out_degree(2), 0);
        assert_eq!(csr.neighbors(2), &[] as &[u32]);
        assert_eq!(csr.weights(2), &[] as &[f32]);

        // Node 3
        assert_eq!(csr.out_degree(3), 2);
        assert_eq!(csr.neighbors(3), &[0, 1]);
        assert_eq!(csr.weights(3), &[1.0, 3.0]);
    }

    #[test]
    fn test_duplicate_edge_accumulation() {
        let edges = vec![
            (0, 1, 1.0),
            (0, 1, 2.5), // Duplicate
            (1, 0, 4.0),
        ];

        let csr = CsrMatrix::from_edges(2, 2, edges);
        assert_eq!(csr.num_edges(), 2);
        assert_eq!(csr.neighbors(0), &[1]);
        assert_eq!(csr.weights(0), &[3.5]);
    }

    #[test]
    fn test_row_normalization() {
        let edges = vec![
            (0, 1, 2.0),
            (0, 2, 6.0), // Sum = 8.0 -> [0.25, 0.75]
            (1, 2, 5.0), // Sum = 5.0 -> [1.0]
        ];

        let csr = CsrMatrix::from_edges(3, 3, edges);
        let norm = csr.row_normalized();

        let w0 = norm.weights(0);
        assert_eq!(w0.len(), 2);
        assert!((w0[0] - 0.25).abs() < 1e-6);
        assert!((w0[1] - 0.75).abs() < 1e-6);

        let w1 = norm.weights(1);
        assert_eq!(w1.len(), 1);
        assert!((w1[0] - 1.0).abs() < 1e-6);

        assert_eq!(norm.weights(2), &[] as &[f32]);
    }

    #[test]
    fn test_transpose() {
        // 0 -> 1, 0 -> 2
        let edges = vec![(0, 1, 1.0), (0, 2, 2.0)];
        let csr = CsrMatrix::from_edges(3, 3, edges);
        let t = csr.transpose();

        // In transpose:
        // 1 -> 0 (1.0)
        // 2 -> 0 (2.0)
        // 0 -> none
        assert_eq!(t.neighbors(0), &[] as &[u32]);
        assert_eq!(t.neighbors(1), &[0]);
        assert_eq!(t.weights(1), &[1.0]);
        assert_eq!(t.neighbors(2), &[0]);
        assert_eq!(t.weights(2), &[2.0]);
    }
}
