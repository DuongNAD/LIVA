use rayon::prelude::*;
use std::cmp::Ordering;
use crate::memory::graph::csr::CsrGraph;

/// Configuration parameters for HippoRAG Personalized PageRank.
#[derive(Debug, Clone)]
pub struct PprConfig {
    /// Damping factor (restart probability) alpha in (0.0, 1.0), standard default = 0.15.
    pub damping_factor: f32,
    /// Maximum power iteration iterations (default = 20).
    pub max_iterations: usize,
    /// Convergence tolerance epsilon for L1-norm residual (default = 1e-6).
    pub tolerance: f32,
    /// Parallel chunk size for Rayon partition (default = 512).
    pub chunk_size: usize,
}

impl Default for PprConfig {
    fn default() -> Self {
        Self {
            damping_factor: 0.15,
            max_iterations: 20,
            tolerance: 1e-6,
            chunk_size: 512,
        }
    }
}

/// Result of HippoRAG Personalized PageRank execution.
#[derive(Debug, Clone)]
pub struct PprResult {
    /// Stationary probability distribution over all nodes in the graph
    pub probabilities: Vec<f32>,
    /// Number of power iterations executed until convergence or cap
    pub iterations: usize,
    /// Final L1 residual norm ||pi_{t+1} - pi_t||_1
    pub residual: f32,
    /// Elapsed time in milliseconds
    pub elapsed_ms: f64,
}

/// HippoRAG Parallel Graph Activation Engine.
#[derive(Debug, Clone)]
pub struct HippoRagEngine {
    /// Forward graph representation
    pub graph: CsrGraph,
    /// Transposed graph representation for pull-based parallel SpMV
    pub transposed: CsrGraph,
    /// Default engine configuration
    pub config: PprConfig,
    /// Sink nodes (out-degree == 0) indices in the forward graph
    dangling_nodes: Vec<u32>,
}

impl HippoRagEngine {
    /// Initialize HippoRagEngine from a forward CsrGraph with default config.
    pub fn new(graph: CsrGraph) -> Self {
        Self::with_config(graph, PprConfig::default())
    }

    /// Initialize HippoRagEngine with custom configuration.
    pub fn with_config(graph: CsrGraph, config: PprConfig) -> Self {
        let transposed = graph.transpose();
        let mut dangling_nodes = Vec::new();
        for u in 0..graph.num_nodes {
            if graph.out_degree(u as u32) == 0 {
                dangling_nodes.push(u as u32);
            }
        }

        Self {
            graph,
            transposed,
            config,
            dangling_nodes,
        }
    }

    /// Access the underlying forward CsrGraph.
    #[inline(always)]
    pub fn graph(&self) -> &CsrGraph {
        &self.graph
    }

    /// Run Personalized PageRank given raw node index seeds and weights.
    pub fn run_ppr(
        &self,
        seed_indices: &[u32],
        seed_weights: &[f32],
    ) -> PprResult {
        self.run_ppr_configured(seed_indices, seed_weights, &self.config)
    }

    /// Run Personalized PageRank with explicit runtime configuration.
    pub fn run_ppr_configured(
        &self,
        seed_indices: &[u32],
        seed_weights: &[f32],
        config: &PprConfig,
    ) -> PprResult {
        let start_time = std::time::Instant::now();
        let n = self.graph.num_nodes;
        if n == 0 {
            return PprResult {
                probabilities: Vec::new(),
                iterations: 0,
                residual: 0.0,
                elapsed_ms: 0.0,
            };
        }

        // 1. Initialize Teleport / Personalization Distribution Vector p
        let mut p = vec![0.0f32; n];
        let mut total_weight = 0.0f32;

        for (&idx, &w) in seed_indices.iter().zip(seed_weights.iter()) {
            if (idx as usize) < n && w > 0.0 {
                p[idx as usize] += w;
                total_weight += w;
            }
        }

        if total_weight > 0.0 {
            let inv_total = 1.0 / total_weight;
            for val in p.iter_mut() {
                *val *= inv_total;
            }
        } else {
            // Uniform teleport distribution fallback
            let uniform = 1.0 / (n as f32);
            p.fill(uniform);
        }

        // 2. Setup double-buffered probability vectors
        let mut pi_curr = p.clone();
        let mut pi_next = vec![0.0f32; n];

        let alpha = config.damping_factor;
        let decay = 1.0 - alpha;
        let chunk_size = config.chunk_size.max(64);

        let trans_row_ptr = &self.transposed.row_ptr;
        let trans_col_indices = &self.transposed.col_indices;
        let trans_values = &self.transposed.values;

        let mut final_residual = 0.0f32;
        let mut completed_iterations = 0;

        // 3. Parallel Power Iteration Loop
        for iter in 0..config.max_iterations {
            completed_iterations = iter + 1;

            // Compute dangling node probability mass sum
            let dangling_mass: f32 = if self.dangling_nodes.is_empty() {
                0.0
            } else {
                self.dangling_nodes
                    .iter()
                    .map(|&u| pi_curr[u as usize])
                    .sum()
            };

            // Dynamic teleport factor conserving probability mass
            let teleport_base = alpha + decay * dangling_mass;

            // Single-pass parallel SpMV + L1 residual norm computation
            let l1_diff: f32 = pi_next
                .par_chunks_mut(chunk_size)
                .enumerate()
                .map(|(chunk_idx, chunk)| {
                    let chunk_start_node = chunk_idx * chunk_size;
                    let mut chunk_diff = 0.0f32;
                    for (offset, target_val) in chunk.iter_mut().enumerate() {
                        let v = chunk_start_node + offset;
                        let start_edge = trans_row_ptr[v] as usize;
                        let end_edge = trans_row_ptr[v + 1] as usize;

                        let cols = &trans_col_indices[start_edge..end_edge];
                        let vals = &trans_values[start_edge..end_edge];
                        let mut in_sum = 0.0f32;
                        for i in 0..cols.len() {
                            in_sum += vals[i] * pi_curr[cols[i] as usize];
                        }

                        let next_val = decay * in_sum + teleport_base * p[v];
                        chunk_diff += (next_val - pi_curr[v]).abs();
                        *target_val = next_val;
                    }
                    chunk_diff
                })
                .sum();

            final_residual = l1_diff;
            std::mem::swap(&mut pi_curr, &mut pi_next);

            if l1_diff < config.tolerance {
                break;
            }
        }

        let elapsed = start_time.elapsed().as_secs_f64() * 1000.0;

        PprResult {
            probabilities: pi_curr,
            iterations: completed_iterations,
            residual: final_residual,
            elapsed_ms: elapsed,
        }
    }

    /// Run Personalized PageRank using named note/entity seeds with weights.
    pub fn run_ppr_by_names(
        &self,
        seeds: &[(&str, f32)],
    ) -> Vec<(String, f32)> {
        let mut seed_indices = Vec::with_capacity(seeds.len());
        let mut seed_weights = Vec::with_capacity(seeds.len());

        for &(name, weight) in seeds {
            if let Some(idx) = self.graph.node_index(name) {
                seed_indices.push(idx);
                seed_weights.push(weight);
            }
        }

        let res = self.run_ppr(&seed_indices, &seed_weights);
        let mut named_results = Vec::with_capacity(self.graph.num_nodes);

        for (idx, &prob) in res.probabilities.iter().enumerate() {
            if let Some(id) = self.graph.node_id(idx as u32) {
                named_results.push((id.to_string(), prob));
            }
        }

        named_results
    }

    /// Extract Top-K ranked nodes given a probability score vector.
    pub fn rank_top_k(
        &self,
        scores: &[f32],
        k: usize,
    ) -> Vec<(String, f32)> {
        let mut indexed: Vec<(u32, f32)> = scores
            .iter()
            .enumerate()
            .map(|(idx, &score)| (idx as u32, score))
            .collect();

        // Sort descending by score, tie-break ascending by index
        indexed.sort_unstable_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        indexed.truncate(k);

        indexed
            .into_iter()
            .map(|(idx, score)| {
                let name = self.graph.node_id(idx).unwrap_or("unknown").to_string();
                (name, score)
            })
            .collect()
    }

    /// Execute PPR and directly return Top-K ranked results with node IDs.
    pub fn run_ppr_top_k(
        &self,
        seeds: &[(&str, f32)],
        k: usize,
    ) -> Vec<(String, f32)> {
        let mut seed_indices = Vec::with_capacity(seeds.len());
        let mut seed_weights = Vec::with_capacity(seeds.len());

        for &(name, weight) in seeds {
            if let Some(idx) = self.graph.node_index(name) {
                seed_indices.push(idx);
                seed_weights.push(weight);
            }
        }

        let res = self.run_ppr(&seed_indices, &seed_weights);
        self.rank_top_k(&res.probabilities, k)
    }

    /// Generate a synthetic scale-free graph (Barabási-Albert / Power-Law inspired)
    /// with `num_nodes` vertices and ~`num_edges` edges for benchmarking and stress testing.
    pub fn generate_synthetic_graph(num_nodes: usize, num_edges: usize) -> CsrGraph {
        assert!(num_nodes > 0, "num_nodes must be positive");
        let avg_degree = (num_edges / num_nodes).max(1);

        let mut edges = Vec::with_capacity(num_edges);
        // Simple fast deterministic pseudo-random LCG
        let mut rng_state: u64 = 1337_2026_0901;
        let mut next_u32 = || -> u32 {
            rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (rng_state >> 32) as u32
        };

        // Ring backbone to ensure connectivity
        for i in 0..num_nodes {
            let next_node = ((i + 1) % num_nodes) as u32;
            edges.push((i as u32, next_node, 1.0f32));
        }

        // Power-law preferential attachment simulation
        for u in 0..num_nodes {
            for _ in 0..avg_degree {
                if edges.len() >= num_edges {
                    break;
                }
                // Target with preferential attachment bias to lower index hubs
                let r1 = (next_u32() as usize) % num_nodes;
                let r2 = (next_u32() as usize) % num_nodes;
                let target = (r1.min(r2)) as u32;
                if target != u as u32 {
                    edges.push((u as u32, target, 1.0f32));
                }
            }
        }

        CsrGraph::from_raw_edges(&edges, num_nodes, false)
    }
}
