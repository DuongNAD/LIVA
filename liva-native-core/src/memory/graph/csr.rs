use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Entity or Note representation in the Knowledge Graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub properties: serde_json::Value,
}

/// Directed or Bidirectional Relation in the Knowledge Graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub relation: String,
    pub weight: f32,
    pub obsolete: bool,
}

/// Compressed Sparse Row (CSR) Graph Representation.
/// Optimized for L3 cache alignment, SIMD, and zero dynamic memory allocation during SpMV traversal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsrGraph {
    /// Number of nodes in the graph |V|
    pub num_nodes: usize,
    /// CSR row pointer array of length num_nodes + 1
    pub row_ptr: Vec<u32>,
    /// Column indices array of length num_edges |E|
    pub col_indices: Vec<u32>,
    /// Row-normalized transition probabilities of length num_edges |E|
    pub values: Vec<f32>,
    /// Mapping from string node ID to continuous 0-based node index
    pub node_to_idx: HashMap<String, u32>,
    /// Reverse mapping from continuous 0-based node index to string node ID
    pub idx_to_node: Vec<String>,
}

impl CsrGraph {
    /// Create an empty CsrGraph.
    pub fn empty() -> Self {
        Self {
            num_nodes: 0,
            row_ptr: vec![0],
            col_indices: Vec::new(),
            values: Vec::new(),
            node_to_idx: HashMap::new(),
            idx_to_node: Vec::new(),
        }
    }

    /// Construct a CsrGraph with explicit CSR vectors and node mapping.
    pub fn new(
        num_nodes: usize,
        row_ptr: Vec<u32>,
        col_indices: Vec<u32>,
        values: Vec<f32>,
        node_to_idx: HashMap<String, u32>,
        idx_to_node: Vec<String>,
    ) -> Self {
        debug_assert_eq!(row_ptr.len(), num_nodes + 1);
        debug_assert_eq!(col_indices.len(), values.len());
        debug_assert_eq!(idx_to_node.len(), num_nodes);
        Self {
            num_nodes,
            row_ptr,
            col_indices,
            values,
            node_to_idx,
            idx_to_node,
        }
    }

    /// Construct CsrGraph from raw index-based edge tuples (source, target, weight).
    pub fn from_raw_edges(
        edges: &[(u32, u32, f32)],
        num_nodes: usize,
        bidirectional: bool,
    ) -> Self {
        let mut idx_to_node = Vec::with_capacity(num_nodes);
        let mut node_to_idx = HashMap::with_capacity(num_nodes);
        for i in 0..num_nodes {
            let id = i.to_string();
            idx_to_node.push(id.clone());
            node_to_idx.insert(id, i as u32);
        }

        Self::build_csr_internal(edges, num_nodes, node_to_idx, idx_to_node, bidirectional)
    }

    /// Construct CsrGraph from named edge tuples (&str, &str, f32).
    pub fn from_named_edges(
        edges: &[(&str, &str, f32)],
        bidirectional: bool,
    ) -> Self {
        let mut node_to_idx: HashMap<String, u32> = HashMap::new();
        let mut idx_to_node: Vec<String> = Vec::new();

        let mut get_or_insert = |name: &str| -> u32 {
            if let Some(&idx) = node_to_idx.get(name) {
                idx
            } else {
                let idx = idx_to_node.len() as u32;
                let s = name.to_string();
                node_to_idx.insert(s.clone(), idx);
                idx_to_node.push(s);
                idx
            }
        };

        let mut raw_edges: Vec<(u32, u32, f32)> = Vec::with_capacity(edges.len() * if bidirectional { 2 } else { 1 });
        for &(src, dst, w) in edges {
            if w <= 0.0 {
                continue;
            }
            let u = get_or_insert(src);
            let v = get_or_insert(dst);
            raw_edges.push((u, v, w));
        }

        let num_nodes = idx_to_node.len();
        Self::build_csr_internal(&raw_edges, num_nodes, node_to_idx, idx_to_node, bidirectional)
    }

    /// Construct CsrGraph from SQLite database l3_nodes and l3_edges records.
    pub fn from_db_records(
        nodes: &[(String, String, String)], // (id, label, properties)
        edges: &[(String, String, String, f32, i32)], // (source, target, relation, weight, obsolete)
        bidirectional: bool,
    ) -> Self {
        let mut node_to_idx: HashMap<String, u32> = HashMap::with_capacity(nodes.len());
        let mut idx_to_node: Vec<String> = Vec::with_capacity(nodes.len());

        for (id, _label, _props) in nodes {
            if !node_to_idx.contains_key(id) {
                let idx = idx_to_node.len() as u32;
                node_to_idx.insert(id.clone(), idx);
                idx_to_node.push(id.clone());
            }
        }

        let mut raw_edges = Vec::new();
        for (src, dst, _rel, weight, obsolete) in edges {
            if *obsolete != 0 || *weight <= 0.0 {
                continue;
            }

            // Ensure source and target are indexed even if not present in node list
            let u = if let Some(&idx) = node_to_idx.get(src) {
                idx
            } else {
                let idx = idx_to_node.len() as u32;
                node_to_idx.insert(src.clone(), idx);
                idx_to_node.push(src.clone());
                idx
            };

            let v = if let Some(&idx) = node_to_idx.get(dst) {
                idx
            } else {
                let idx = idx_to_node.len() as u32;
                node_to_idx.insert(dst.clone(), idx);
                idx_to_node.push(dst.clone());
                idx
            };

            raw_edges.push((u, v, *weight));
        }

        let num_nodes = idx_to_node.len();
        Self::build_csr_internal(&raw_edges, num_nodes, node_to_idx, idx_to_node, bidirectional)
    }

    /// Internal builder that sorts, deduplicates, and row-normalizes edges into CSR format.
    fn build_csr_internal(
        edges: &[(u32, u32, f32)],
        num_nodes: usize,
        node_to_idx: HashMap<String, u32>,
        idx_to_node: Vec<String>,
        bidirectional: bool,
    ) -> Self {
        if num_nodes == 0 {
            return Self::empty();
        }

        // Expand bidirectional edges and filter out invalid indices
        let mut all_edges: Vec<(u32, u32, f32)> = Vec::with_capacity(edges.len() * if bidirectional { 2 } else { 1 });
        for &(u, v, w) in edges {
            if (u as usize) < num_nodes && (v as usize) < num_nodes && w > 0.0 {
                all_edges.push((u, v, w));
                if bidirectional && u != v {
                    all_edges.push((v, u, w));
                }
            }
        }

        // Sort edges by source node, then by destination node
        all_edges.sort_unstable_by(|a, b| {
            a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1))
        });

        // Deduplicate multi-edges between the same (u, v) pair by summing weights
        let mut deduped_edges: Vec<(u32, u32, f32)> = Vec::with_capacity(all_edges.len());
        for edge in all_edges {
            if let Some(last) = deduped_edges.last_mut() {
                if last.0 == edge.0 && last.1 == edge.1 {
                    last.2 += edge.2;
                    continue;
                }
            }
            deduped_edges.push(edge);
        }

        // Count out-degree per node
        let mut out_degrees = vec![0u32; num_nodes];
        let mut row_sums = vec![0.0f32; num_nodes];

        for &(u, _, w) in &deduped_edges {
            out_degrees[u as usize] += 1;
            row_sums[u as usize] += w;
        }

        // Compute row_ptr via prefix sums
        let mut row_ptr = Vec::with_capacity(num_nodes + 1);
        row_ptr.push(0);
        let mut current_offset = 0u32;
        for &deg in &out_degrees {
            current_offset += deg;
            row_ptr.push(current_offset);
        }

        let num_edges = deduped_edges.len();
        let mut col_indices = Vec::with_capacity(num_edges);
        let mut values = Vec::with_capacity(num_edges);

        for (u, v, w) in deduped_edges {
            col_indices.push(v);
            let sum = row_sums[u as usize];
            let norm_weight = if sum > 0.0 { w / sum } else { 0.0 };
            values.push(norm_weight);
        }

        Self {
            num_nodes,
            row_ptr,
            col_indices,
            values,
            node_to_idx,
            idx_to_node,
        }
    }

    /// Compute the transposed CSR matrix (incoming edges).
    /// In the transposed graph, row `v` contains all sources `u` that have an edge `u -> v`.
    /// This enables pull-based SpMV during parallel PageRank power iterations without atomic contention.
    pub fn transpose(&self) -> CsrGraph {
        let n = self.num_nodes;
        if n == 0 {
            return Self::empty();
        }

        let mut in_degrees = vec![0u32; n];
        for &v in &self.col_indices {
            if (v as usize) < n {
                in_degrees[v as usize] += 1;
            }
        }

        let mut trans_row_ptr = Vec::with_capacity(n + 1);
        trans_row_ptr.push(0);
        let mut offset = 0u32;
        for &deg in &in_degrees {
            offset += deg;
            trans_row_ptr.push(offset);
        }

        let num_edges = self.col_indices.len();
        let mut trans_col_indices = vec![0u32; num_edges];
        let mut trans_values = vec![0.0f32; num_edges];
        let mut insert_cursor = trans_row_ptr[0..n].to_vec();

        for u in 0..n {
            let start = self.row_ptr[u] as usize;
            let end = self.row_ptr[u + 1] as usize;
            for edge_idx in start..end {
                let v = self.col_indices[edge_idx] as usize;
                let w = self.values[edge_idx]; // Transition prob P(u -> v)
                let pos = insert_cursor[v] as usize;
                trans_col_indices[pos] = u as u32;
                trans_values[pos] = w;
                insert_cursor[v] += 1;
            }
        }

        CsrGraph {
            num_nodes: n,
            row_ptr: trans_row_ptr,
            col_indices: trans_col_indices,
            values: trans_values,
            node_to_idx: self.node_to_idx.clone(),
            idx_to_node: self.idx_to_node.clone(),
        }
    }

    /// Total number of vertices in the graph |V|.
    #[inline(always)]
    pub fn node_count(&self) -> usize {
        self.num_nodes
    }

    /// Total number of directed edges in the graph |E|.
    #[inline(always)]
    pub fn edge_count(&self) -> usize {
        self.col_indices.len()
    }

    /// Lookup string node identifier by index.
    #[inline(always)]
    pub fn node_id(&self, idx: u32) -> Option<&str> {
        self.idx_to_node.get(idx as usize).map(|s| s.as_str())
    }

    /// Lookup node index by string node identifier.
    #[inline(always)]
    pub fn node_index(&self, id: &str) -> Option<u32> {
        self.node_to_idx.get(id).copied()
    }

    /// Out-degree of node u.
    #[inline(always)]
    pub fn out_degree(&self, u: u32) -> usize {
        let u = u as usize;
        if u < self.num_nodes {
            (self.row_ptr[u + 1] - self.row_ptr[u]) as usize
        } else {
            0
        }
    }

    /// Slice of outgoing target node indices and normalized transition weights for node u.
    #[inline(always)]
    pub fn out_neighbors(&self, u: u32) -> (&[u32], &[f32]) {
        let u = u as usize;
        if u < self.num_nodes {
            let start = self.row_ptr[u] as usize;
            let end = self.row_ptr[u + 1] as usize;
            (&self.col_indices[start..end], &self.values[start..end])
        } else {
            (&[], &[])
        }
    }

    /// Calculate total heap memory consumption in bytes.
    pub fn memory_usage_bytes(&self) -> usize {
        let row_ptr_bytes = self.row_ptr.len() * std::mem::size_of::<u32>();
        let col_bytes = self.col_indices.len() * std::mem::size_of::<u32>();
        let val_bytes = self.values.len() * std::mem::size_of::<f32>();
        let map_bytes = self.node_to_idx.len() * (std::mem::size_of::<String>() + std::mem::size_of::<u32>());
        let names_bytes: usize = self.idx_to_node.iter().map(|s| s.capacity()).sum();

        row_ptr_bytes + col_bytes + val_bytes + map_bytes + names_bytes + std::mem::size_of::<Self>()
    }
}
