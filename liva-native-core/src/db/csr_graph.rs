//! In-Memory Compressed Sparse Row (CSR) Graph Cache for HippoRAG.
//!
//! Architectural Solution for L3 Knowledge Graph multi-hop retrieval under 10ms.
//! Maintains compact, cache-friendly flat arrays (`row_ptr`, `col_indices`, `weights`)
//! and executes 3-iteration Personalized PageRank (PPR) via Sparse Matrix-Vector (SpMV)
//! multiplication on CPU with zero disk I/O.

use rusqlite::Connection;
use std::collections::HashMap;

/// An edge representation during dynamic graph editing.
#[derive(Debug, Clone)]
pub struct DynamicEdge {
    pub target: usize,
    pub relation: String,
    pub weight: f32,
}

/// Compressed Sparse Row (CSR) Graph Cache in RAM.
#[derive(Debug, Clone)]
pub struct CsrGraph {
    /// Mapping from string node ID to continuous integer index.
    node_to_idx: HashMap<String, usize>,
    /// Mapping from continuous index to string node ID.
    idx_to_node: Vec<String>,
    /// Human-readable entity labels.
    node_labels: Vec<String>,
    /// Entity properties JSON string.
    node_properties: Vec<String>,

    /// Dynamic adjacency list for low-overhead modifications: node_idx -> list of outgoing edges.
    adjacency: Vec<Vec<DynamicEdge>>,

    /// CSR Row Pointers: length N + 1. `row_ptr[i]` to `row_ptr[i+1]` defines outgoing edges for node `i`.
    row_ptr: Vec<usize>,
    /// CSR Column Indices: length M (total edges). Destination node indices.
    col_indices: Vec<usize>,
    /// CSR Row-Normalized Transition Probabilities: length M. Sum for each non-empty row is 1.0.
    weights: Vec<f32>,
    /// CSR Relations for path explanation: length M.
    edge_relations: Vec<String>,

    /// Flag indicating whether CSR flat arrays need recompilation after graph mutations.
    dirty: bool,
}

impl Default for CsrGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl CsrGraph {
    /// Creates an empty CSR graph cache.
    pub fn new() -> Self {
        Self {
            node_to_idx: HashMap::new(),
            idx_to_node: Vec::new(),
            node_labels: Vec::new(),
            node_properties: Vec::new(),
            adjacency: Vec::new(),
            row_ptr: vec![0],
            col_indices: Vec::new(),
            weights: Vec::new(),
            edge_relations: Vec::new(),
            dirty: false,
        }
    }

    /// Returns true if the graph has been mutated and requires CSR compilation.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.idx_to_node.len()
    }

    /// Number of directed edges in the graph.
    pub fn edge_count(&self) -> usize {
        if self.dirty {
            self.adjacency.iter().map(|edges| edges.len()).sum()
        } else {
            self.col_indices.len()
        }
    }

    /// Returns the node index if it exists.
    pub fn get_node_index(&self, id: &str) -> Option<usize> {
        self.node_to_idx.get(id).copied()
    }

    /// Returns the node ID for a given index.
    pub fn get_node_id(&self, idx: usize) -> Option<&str> {
        self.idx_to_node.get(idx).map(|s| s.as_str())
    }

    /// Returns the node label for a given index.
    pub fn get_node_label(&self, idx: usize) -> Option<&str> {
        self.node_labels.get(idx).map(|s| s.as_str())
    }

    /// Returns the node properties JSON for a given index.
    pub fn get_node_properties(&self, idx: usize) -> Option<&str> {
        self.node_properties.get(idx).map(|s| s.as_str())
    }

    /// Adds or updates a node in the graph cache.
    pub fn add_node(&mut self, id: String, label: String, properties: String) -> usize {
        if let Some(&idx) = self.node_to_idx.get(&id) {
            self.node_labels[idx] = label;
            self.node_properties[idx] = properties;
            idx
        } else {
            let idx = self.idx_to_node.len();
            self.node_to_idx.insert(id.clone(), idx);
            self.idx_to_node.push(id);
            self.node_labels.push(label);
            self.node_properties.push(properties);
            self.adjacency.push(Vec::new());
            self.dirty = true;
            idx
        }
    }

    /// Adds a directed edge from `source` to `target` with a given relation and weight.
    /// If `bidirectional` is true, an edge in the opposite direction is also added.
    pub fn add_edge(
        &mut self,
        source: &str,
        target: &str,
        relation: &str,
        weight: f32,
        bidirectional: bool,
    ) {
        let u = match self.node_to_idx.get(source) {
            Some(&idx) => idx,
            None => self.add_node(source.to_string(), source.to_string(), "{}".to_string()),
        };
        let v = match self.node_to_idx.get(target) {
            Some(&idx) => idx,
            None => self.add_node(target.to_string(), target.to_string(), "{}".to_string()),
        };

        // Add u -> v
        self.add_or_update_adjacency(u, v, relation, weight);

        // Add v -> u if bidirectional
        if bidirectional {
            self.add_or_update_adjacency(v, u, relation, weight);
        }

        self.dirty = true;
    }

    fn add_or_update_adjacency(&mut self, u: usize, v: usize, relation: &str, weight: f32) {
        if let Some(existing) = self.adjacency[u]
            .iter_mut()
            .find(|e| e.target == v && e.relation == relation)
        {
            existing.weight = weight;
        } else {
            self.adjacency[u].push(DynamicEdge {
                target: v,
                relation: relation.to_string(),
                weight,
            });
        }
    }

    /// Recompiles dynamic adjacency list into flat CSR representation.
    /// Normalizes row weights so each row sums to 1.0 (row-stochastic transition probability).
    pub fn compile_csr(&mut self) {
        let n = self.idx_to_node.len();
        let total_edges: usize = self.adjacency.iter().map(|edges| edges.len()).sum();

        let mut row_ptr = Vec::with_capacity(n + 1);
        let mut col_indices = Vec::with_capacity(total_edges);
        let mut weights = Vec::with_capacity(total_edges);
        let mut edge_relations = Vec::with_capacity(total_edges);

        let mut current_offset = 0;
        row_ptr.push(0);

        for u in 0..n {
            let edges = &self.adjacency[u];
            let count = edges.len();

            if count == 0 {
                row_ptr.push(current_offset);
                continue;
            }

            // Calculate total raw weight for row normalization
            let raw_sum: f32 = edges.iter().map(|e| e.weight.max(0.0)).sum();

            for edge in edges {
                col_indices.push(edge.target);
                edge_relations.push(edge.relation.clone());

                let norm_weight = if raw_sum > 0.0 {
                    edge.weight.max(0.0) / raw_sum
                } else {
                    1.0 / count as f32
                };
                weights.push(norm_weight);
            }

            current_offset += count;
            row_ptr.push(current_offset);
        }

        self.row_ptr = row_ptr;
        self.col_indices = col_indices;
        self.weights = weights;
        self.edge_relations = edge_relations;
        self.dirty = false;
    }

    /// Loads graph structure from SQLite `l3_nodes` and `l3_edges` tables.
    pub fn from_db(conn: &Connection) -> Result<Self, rusqlite::Error> {
        let mut graph = Self::new();

        // 1. Load nodes
        let mut node_stmt = conn.prepare("SELECT id, label, properties FROM l3_nodes")?;
        let node_rows = node_stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        for row in node_rows {
            let (id, label, properties) = row?;
            graph.add_node(id, label, properties);
        }

        // 2. Load active edges (obsolete = 0)
        let mut edge_stmt = conn
            .prepare("SELECT source, target, relation, weight FROM l3_edges WHERE obsolete = 0")?;
        let edge_rows = edge_stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, f64>(3)? as f32,
            ))
        })?;

        for row in edge_rows {
            let (source, target, relation, weight) = row?;
            // By default, treat knowledge relations bidirectionally for HippoRAG multi-hop spreading
            graph.add_edge(&source, &target, &relation, weight, true);
        }

        // 3. Compile into contiguous flat CSR arrays
        graph.compile_csr();

        Ok(graph)
    }

    /// Executes HippoRAG Personalized PageRank (PPR) using 3 iterations of SpMV.
    ///
    /// Mathematical formulation:
    /// $$\mathbf{p}^{(k+1)} = (1 - d)\mathbf{s} + d \mathbf{M} \mathbf{p}^{(k)}$$
    ///
    /// - `seeds`: Initial preference nodes and their activation weights `[(&node_id, weight)]`.
    /// - `iterations`: Number of SpMV power iterations (default: 3).
    /// - `damping`: Damping factor $d$ (default: 0.85).
    /// - `top_k`: Number of top-ranked entities to return.
    ///
    /// Returns sorted pairs of `(node_id, score)` in descending order of activation.
    pub fn personalized_pagerank(
        &mut self,
        seeds: &[(&str, f32)],
        iterations: usize,
        damping: f32,
        top_k: usize,
    ) -> Vec<(String, f32)> {
        if self.dirty {
            self.compile_csr();
        }

        let n = self.idx_to_node.len();
        if n == 0 || seeds.is_empty() {
            return Vec::new();
        }

        // 1. Build and normalize seed preference vector s
        let mut s = vec![0.0f32; n];
        let mut seed_sum = 0.0f32;
        for &(seed_id, raw_weight) in seeds {
            if let Some(&idx) = self.node_to_idx.get(seed_id) {
                let w = raw_weight.max(0.0);
                s[idx] += w;
                seed_sum += w;
            }
        }

        if seed_sum <= 0.0 {
            return Vec::new();
        }

        for val in s.iter_mut() {
            *val /= seed_sum;
        }

        // 2. Power iteration: p^(0) = s
        let mut p = s.clone();
        let mut next_p = vec![0.0f32; n];
        let restart_factor = 1.0 - damping;

        for _ in 0..iterations {
            // Base restart distribution: (1 - d) * s
            for i in 0..n {
                next_p[i] = restart_factor * s[i];
            }

            // SpMV: for each source node u, distribute its probability mass p[u] across outgoing edges
            for (u, &p_u) in p.iter().enumerate().take(n) {
                if p_u <= 1e-7 {
                    continue;
                }

                let start = self.row_ptr[u];
                let end = self.row_ptr[u + 1];

                for e in start..end {
                    let v = self.col_indices[e];
                    let w = self.weights[e];
                    next_p[v] += damping * p_u * w;
                }
            }

            std::mem::swap(&mut p, &mut next_p);
        }

        // 3. Rank entities by activation score
        let mut results: Vec<(String, f32)> = p
            .into_iter()
            .enumerate()
            .filter(|(_, score)| *score > 1e-6)
            .map(|(idx, score)| (self.idx_to_node[idx].clone(), score))
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        if results.len() > top_k {
            results.truncate(top_k);
        }

        results
    }

    /// Read-only version of Personalized PageRank.
    ///
    /// If the CSR arrays are compiled (`!self.dirty`), executes via contiguous flat CSR SpMV.
    /// If the graph is dirty (`self.dirty`), gracefully falls back to dynamic adjacency traversal
    /// ensuring non-empty, accurate multi-hop retrieval without requiring an exclusive write lock.
    pub fn personalized_pagerank_readonly(
        &self,
        seeds: &[(&str, f32)],
        iterations: usize,
        damping: f32,
        top_k: usize,
    ) -> Vec<(String, f32)> {
        let n = self.idx_to_node.len();
        if n == 0 || seeds.is_empty() {
            return Vec::new();
        }

        // 1. Build and normalize seed preference vector s
        let mut s = vec![0.0f32; n];
        let mut seed_sum = 0.0f32;
        for &(seed_id, raw_weight) in seeds {
            if let Some(&idx) = self.node_to_idx.get(seed_id) {
                let w = raw_weight.max(0.0);
                s[idx] += w;
                seed_sum += w;
            }
        }

        if seed_sum <= 0.0 {
            return Vec::new();
        }

        for val in s.iter_mut() {
            *val /= seed_sum;
        }

        // 2. Power iteration: p^(0) = s
        let mut p = s.clone();
        let mut next_p = vec![0.0f32; n];
        let restart_factor = 1.0 - damping;

        for _ in 0..iterations {
            for i in 0..n {
                next_p[i] = restart_factor * s[i];
            }

            if !self.dirty {
                // High-performance contiguous flat CSR SpMV path
                for (u, &p_u) in p.iter().enumerate().take(n) {
                    if p_u <= 1e-7 {
                        continue;
                    }

                    let start = self.row_ptr[u];
                    let end = self.row_ptr[u + 1];

                    for e in start..end {
                        let v = self.col_indices[e];
                        let w = self.weights[e];
                        next_p[v] += damping * p_u * w;
                    }
                }
            } else {
                // Graceful fallback: dynamic adjacency list traversal when uncompiled/dirty
                for (u, &p_u) in p.iter().enumerate().take(n) {
                    if p_u <= 1e-7 {
                        continue;
                    }

                    let edges = &self.adjacency[u];
                    if edges.is_empty() {
                        continue;
                    }

                    let raw_sum: f32 = edges.iter().map(|e| e.weight.max(0.0)).sum();
                    let count = edges.len();

                    for edge in edges {
                        let norm_weight = if raw_sum > 0.0 {
                            edge.weight.max(0.0) / raw_sum
                        } else {
                            1.0 / count as f32
                        };
                        next_p[edge.target] += damping * p_u * norm_weight;
                    }
                }
            }

            std::mem::swap(&mut p, &mut next_p);
        }

        // 3. Rank entities by activation score
        let mut results: Vec<(String, f32)> = p
            .into_iter()
            .enumerate()
            .filter(|(_, score)| *score > 1e-6)
            .map(|(idx, score)| (self.idx_to_node[idx].clone(), score))
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        if results.len() > top_k {
            results.truncate(top_k);
        }

        results
    }

    /// Finds seed nodes in the graph that match words or phrases from the query text.
    pub fn find_seed_nodes(&self, query: &str, max_seeds: usize) -> Vec<(String, f32)> {
        let query_lower = query.to_lowercase();
        let mut matched = Vec::new();

        for (idx, id) in self.idx_to_node.iter().enumerate() {
            let label = &self.node_labels[idx];
            let id_lower = id.to_lowercase();
            let label_lower = label.to_lowercase();

            // Match if query contains node id or label (word boundary or substring for compound words)
            if query_lower.contains(&id_lower) || query_lower.contains(&label_lower) {
                // Heuristic score: length of match
                let weight = 1.0f32 + (label.len() as f32 * 0.05);
                matched.push((id.clone(), weight));
            }
        }

        matched.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        if matched.len() > max_seeds {
            matched.truncate(max_seeds);
        }

        matched
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csr_compilation_and_row_stochastic_normalization() {
        let mut graph = CsrGraph::new();
        graph.add_node("A".into(), "Node A".into(), "{}".into());
        graph.add_node("B".into(), "Node B".into(), "{}".into());
        graph.add_node("C".into(), "Node C".into(), "{}".into());

        // A -> B (weight 1.0), A -> C (weight 3.0)
        graph.add_edge("A", "B", "connects", 1.0, false);
        graph.add_edge("A", "C", "connects", 3.0, false);

        graph.compile_csr();

        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 2);

        // Row 0 (A) should have 2 edges, normalized to 1/4 (0.25) and 3/4 (0.75)
        let a_idx = graph.get_node_index("A").unwrap();
        let start = graph.row_ptr[a_idx];
        let end = graph.row_ptr[a_idx + 1];
        assert_eq!(end - start, 2);

        let w1 = graph.weights[start];
        let w2 = graph.weights[start + 1];
        assert!((w1 + w2 - 1.0).abs() < 1e-5);
        assert!((w1 - 0.25).abs() < 1e-5);
        assert!((w2 - 0.75).abs() < 1e-5);
    }

    #[test]
    fn test_hipporag_multi_hop_activation() {
        let mut graph = CsrGraph::new();
        // Construct chain: A -> B -> C and isolated node D
        graph.add_edge("A", "B", "leads_to", 1.0, true);
        graph.add_edge("B", "C", "leads_to", 1.0, true);
        graph.add_node("D".into(), "Node D".into(), "{}".into());
        graph.compile_csr();

        // Activate seed A
        let seeds = [("A", 1.0f32)];
        let ppr = graph.personalized_pagerank(&seeds, 3, 0.85, 4);

        assert!(!ppr.is_empty());
        let node_scores: HashMap<String, f32> = ppr.into_iter().collect();
        // A, B, C must be activated
        assert!(node_scores.contains_key("A"));
        assert!(node_scores.contains_key("B"));
        assert!(node_scores.contains_key("C"));

        // Multi-hop C (2 hops away from A) must have non-zero score
        assert!(node_scores["C"] > 0.0);
        assert!(node_scores["B"] > 0.0);
        assert!(node_scores["A"] > 0.0);

        // Isolated node D must not be activated
        assert!(!node_scores.contains_key("D"));
    }

    #[test]
    fn test_find_seed_nodes() {
        let mut graph = CsrGraph::new();
        graph.add_node("paris".into(), "Paris".into(), "{}".into());
        graph.add_node("tokyo".into(), "Tokyo".into(), "{}".into());
        graph.add_node("eiffel".into(), "Eiffel Tower".into(), "{}".into());

        let seeds = graph.find_seed_nodes("Tôi muốn đi du lịch Paris ngắm Eiffel Tower", 5);
        assert_eq!(seeds.len(), 2);
        let ids: Vec<String> = seeds.into_iter().map(|(id, _)| id).collect();
        assert!(ids.contains(&"paris".to_string()));
        assert!(ids.contains(&"eiffel".to_string()));
    }

    #[test]
    fn test_csr_graph_dirty_fallback_and_compilation() {
        let mut graph = CsrGraph::new();
        assert!(!graph.is_dirty());

        graph.add_edge("Alice", "Bob", "collaborates_with", 1.0, true);
        graph.add_edge("Bob", "Charlie", "manages", 1.0, true);

        // Graph is dirty because compile_csr was not called
        assert!(graph.is_dirty());
        assert_eq!(graph.edge_count(), 4); // Bidirectional

        // Read-only PPR must execute via adjacency fallback and succeed
        let seeds = [("Alice", 1.0f32)];
        let ppr_dirty = graph.personalized_pagerank_readonly(&seeds, 3, 0.85, 3);
        assert!(
            !ppr_dirty.is_empty(),
            "Dirty fallback must return active nodes"
        );
        let dirty_map: std::collections::HashMap<String, f32> = ppr_dirty.into_iter().collect();
        assert!(
            dirty_map.contains_key("Charlie"),
            "Charlie must be reached across 2 hops"
        );

        // Compile CSR
        graph.compile_csr();
        assert!(!graph.is_dirty());

        // Clean flat CSR PPR must match results
        let ppr_clean = graph.personalized_pagerank_readonly(&seeds, 3, 0.85, 3);
        let clean_map: std::collections::HashMap<String, f32> = ppr_clean.into_iter().collect();
        assert_eq!(dirty_map.len(), clean_map.len());
        for (k, v) in dirty_map {
            let clean_v = clean_map.get(&k).expect("clean map must contain key");
            assert!(
                (clean_v - v).abs() < 1e-4,
                "Scores between dirty and clean must match"
            );
        }
    }
}
