//! ET-WCN Cooling Algorithm (ET-Weighted Chain Cooling)
//!
//! Ported from chen0430tw/hidrs `algorithms/et_cooling.py`.
//!
//! Replaces conventional exponential/linear temperature decay with a schedule
//! driven by the symmetric gap Δ(a,b) = (a−1)(b−1)−1, where
//!   • a encodes the "additive fluctuation" of energy (σ_E)
//!   • b encodes the "multiplicative convergence" (improvement monotonicity)
//!
//! Temperature formula:
//!   T(Δ, β₁) = T_max / (1 + max(0,Δ)/ι) × 1/(1 + 0.5·max(0, β₁ − β₁_target))
//!
//! β₁ = E − V + C  (first Betti number of the Weight Chain Network adjacency graph)
//!
//! Metropolis acceptance:
//!   accept if ΔE ≤ 0, else accept with probability exp(−ΔE / T)
//!
//! # URP integration
//!
//! Use `ETCoolingPolicy` as a drop-in replacement for `MultifactorPolicy`.
//! It optimises the partition→node assignment by minimising total route cost
//! plus a load-imbalance penalty.

use std::collections::{HashMap, HashSet};

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

use crate::cost::{node_score, route_cost};
use crate::ir::IRGraph;
use crate::node::Node;
use crate::policy::SchedulerPolicy;

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Information quantum ι
const IOTA: f64 = 1.0;

// ─────────────────────────────────────────────────────────────────────────────
// Primitives
// ─────────────────────────────────────────────────────────────────────────────

/// Symmetric gap  Δ(a, b) = (a − 1)(b − 1) − 1
pub fn symmetric_gap(a: f64, b: f64) -> f64 {
    (a - 1.0) * (b - 1.0) - 1.0
}

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

/// First Betti number  β₁ = E − V + C
/// where E = edge count, V = node count, C = connected-component count.
fn compute_beta1(adj: &HashMap<usize, HashSet<usize>>, n: usize) -> i64 {
    let mut edge_count = 0usize;
    for neighbors in adj.values() {
        edge_count += neighbors.len();
    }
    let e = edge_count / 2; // undirected

    // BFS component count
    let mut visited = vec![false; n];
    let mut components = 0usize;
    for start in 0..n {
        if !visited[start] {
            components += 1;
            let mut stack = vec![start];
            while let Some(node) = stack.pop() {
                if visited[node] { continue; }
                visited[node] = true;
                if let Some(nbrs) = adj.get(&node) {
                    for &nb in nbrs {
                        if !visited[nb] { stack.push(nb); }
                    }
                }
            }
        }
    }

    (e as i64) - (n as i64) + (components as i64)
}

// ─────────────────────────────────────────────────────────────────────────────
// Weight Chain Network
// ─────────────────────────────────────────────────────────────────────────────

/// Stores per-node embedding vectors and computes sigmoid-weighted edge strengths.
pub struct WeightChainNetwork {
    embeddings: Vec<Vec<f64>>,
    bias: f64,
    threshold: f64,
}

impl WeightChainNetwork {
    pub fn new(n_nodes: usize, dim: usize, bias: f64, threshold: f64, rng: &mut SmallRng) -> Self {
        let embeddings = (0..n_nodes)
            .map(|_| (0..dim).map(|_| rng.gen_range(-1.0_f64..1.0)).collect())
            .collect();
        Self { embeddings, bias, threshold }
    }

    /// f(eᵢ, eⱼ) = σ(⟨φ(eᵢ), φ(eⱼ)⟩ + b)
    pub fn edge_weight(&self, i: usize, j: usize) -> f64 {
        let ei = &self.embeddings[i];
        let ej = &self.embeddings[j];
        let dot: f64 = ei.iter().zip(ej).map(|(a, b)| a * b).sum();
        sigmoid(dot + self.bias)
    }

    pub fn adjacency(&self) -> HashMap<usize, HashSet<usize>> {
        let n = self.embeddings.len();
        let mut adj: HashMap<usize, HashSet<usize>> = HashMap::new();
        for i in 0..n {
            for j in (i + 1)..n {
                if self.edge_weight(i, j) >= self.threshold {
                    adj.entry(i).or_default().insert(j);
                    adj.entry(j).or_default().insert(i);
                }
            }
        }
        adj
    }

    pub fn beta1(&self) -> i64 {
        compute_beta1(&self.adjacency(), self.embeddings.len())
    }

    /// Exponential moving-average update on the least-similar node's embedding.
    pub fn update(&mut self, solution_embedding: &[f64], alpha: f64) {
        let n = self.embeddings.len();
        // Find most dissimilar node (lowest dot product with solution)
        let worst = (0..n)
            .map(|i| {
                let dot: f64 = self.embeddings[i].iter()
                    .zip(solution_embedding)
                    .map(|(a, b)| a * b)
                    .sum();
                (i, dot)
            })
            .min_by(|x, y| x.1.partial_cmp(&y.1).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0);

        let emb = &mut self.embeddings[worst];
        for (e, s) in emb.iter_mut().zip(solution_embedding) {
            *e = (1.0 - alpha) * *e + alpha * s;
        }
    }

    /// Suggest a neighbour index weighted by edge strength from `from`.
    pub fn weighted_neighbor(&self, from: usize, rng: &mut SmallRng) -> Option<usize> {
        let n = self.embeddings.len();
        let weights: Vec<(usize, f64)> = (0..n)
            .filter(|&j| j != from)
            .map(|j| (j, self.edge_weight(from, j)))
            .collect();
        let total: f64 = weights.iter().map(|(_, w)| w).sum();
        if total < 1e-12 { return None; }
        let mut pick = rng.gen_range(0.0..total);
        for (j, w) in &weights {
            pick -= w;
            if pick <= 0.0 { return Some(*j); }
        }
        weights.last().map(|(j, _)| *j)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ET Cooling Scheduler
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum CoolingPhase {
    Symmetric,   // Δ ≈ 0, max exploration
    Breaking,    // 0 < Δ < Δ_crit
    Crystallized, // Δ ≥ Δ_crit, converging
}

pub struct ETCoolingScheduler {
    pub t_max: f64,
    pub t_min: f64,
    pub delta_crit: f64,
    pub beta1_target: i64,
    pub temperature: f64,
    pub phase: CoolingPhase,
    stagnation: usize,
    energy_history: Vec<f64>,
}

impl ETCoolingScheduler {
    pub fn new(t_max: f64, t_min: f64, delta_crit: f64, beta1_target: i64) -> Self {
        Self {
            t_max, t_min, delta_crit, beta1_target,
            temperature: t_max,
            phase: CoolingPhase::Symmetric,
            stagnation: 0,
            energy_history: Vec::new(),
        }
    }

    /// T(Δ, β₁) = T_max / (1 + max(0,Δ)/ι) × 1/(1 + 0.5·max(0, β₁ − β₁_target))
    pub fn compute_temperature(&self, delta: f64, beta1: i64) -> f64 {
        let t_delta = self.t_max / (1.0 + delta.max(0.0) / IOTA);
        let excess = (beta1 - self.beta1_target).max(0) as f64;
        let beta1_factor = 1.0 / (1.0 + 0.5 * excess);
        (t_delta * beta1_factor).clamp(self.t_min, self.t_max)
    }

    /// Metropolis acceptance criterion
    pub fn should_accept(&self, e_old: f64, e_new: f64, rng: &mut SmallRng) -> bool {
        let de = e_new - e_old;
        if de <= 0.0 { return true; }
        if self.temperature < 1e-12 { return false; }
        rng.gen_range(0.0_f64..1.0) < (-de / self.temperature).exp()
    }

    /// Advance state: update temperature, phase, stagnation counter.
    pub fn step(&mut self, delta: f64, beta1: i64, energy: f64) {
        self.temperature = self.compute_temperature(delta, beta1);

        self.phase = if delta.abs() < 0.1 {
            CoolingPhase::Symmetric
        } else if delta < self.delta_crit {
            CoolingPhase::Breaking
        } else {
            CoolingPhase::Crystallized
        };

        // Track stagnation
        if let Some(&prev) = self.energy_history.last() {
            if (energy - prev).abs() < 1e-6 {
                self.stagnation += 1;
            } else {
                self.stagnation = 0;
            }
        }
        self.energy_history.push(energy);
        if self.energy_history.len() > 50 { self.energy_history.remove(0); }
    }

    pub fn should_reheat(&self, threshold: usize) -> bool {
        self.phase == CoolingPhase::Breaking && self.stagnation >= threshold
    }

    pub fn reheat(&mut self) {
        self.temperature = self.t_max * 0.6;
        self.phase = CoolingPhase::Symmetric;
        self.stagnation = 0;
    }

    pub fn is_converged(&self, patience: usize) -> bool {
        self.phase == CoolingPhase::Crystallized && self.stagnation >= patience
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ETWCNCooling — main optimizer
// ─────────────────────────────────────────────────────────────────────────────

pub struct ETCoolingResult {
    pub best_solution: Vec<usize>,
    pub best_energy: f64,
    pub epochs: usize,
    pub final_phase: CoolingPhase,
    pub final_temperature: f64,
}

pub struct ETWCNCooling {
    pub dim: usize,
    pub t_max: f64,
    pub t_min: f64,
    pub delta_crit: f64,
    pub beta1_target: i64,
    pub wcn_bias: f64,
    pub wcn_threshold: f64,
    pub stagnation_threshold: usize,
    pub patience: usize,
    rng: SmallRng,
}

impl ETWCNCooling {
    pub fn new(dim: usize) -> Self {
        Self {
            dim,
            t_max: 1.0,
            t_min: 0.01,
            delta_crit: 5.0,
            beta1_target: 0,
            wcn_bias: -1.0,
            wcn_threshold: 0.5,
            stagnation_threshold: 30,
            patience: 20,
            rng: SmallRng::from_entropy(),
        }
    }

    pub fn with_seed(mut self, seed: u64) -> Self {
        self.rng = SmallRng::seed_from_u64(seed);
        self
    }

    pub fn with_temperature(mut self, t_max: f64, t_min: f64) -> Self {
        self.t_max = t_max;
        self.t_min = t_min;
        self
    }

    /// Map solution history → (a, b) for symmetric gap computation.
    /// a = 2 + σ_E (energy std-dev encodes additive fluctuation)
    /// b = 2 + convergence × σ_E × 2 (improvement monotonicity)
    fn solution_to_ab(&self, energy_history: &[f64], stagnation: usize) -> (f64, f64) {
        if energy_history.len() < 2 {
            return (1.0, 1.0);
        }
        let mean = energy_history.iter().sum::<f64>() / energy_history.len() as f64;
        let var = energy_history.iter().map(|e| (e - mean).powi(2)).sum::<f64>()
            / energy_history.len() as f64;
        let sigma = var.sqrt();

        let improvements = energy_history.windows(2)
            .filter(|w| w[1] < w[0])
            .count() as f64;
        let total = (energy_history.len() - 1) as f64;
        let convergence = if total > 0.0 { 1.0 - improvements / total } else { 0.0 };

        let a = 2.0 + sigma;
        let b = 2.0 + convergence * sigma * 2.0 + stagnation as f64 * 0.05;
        (a, b)
    }

    /// Optimize a discrete solution (Vec<usize> with `n_choices` options per dimension).
    /// `energy_fn(solution) → f64` — lower is better.
    pub fn optimize<F>(
        &mut self,
        initial: Vec<usize>,
        n_choices: usize,
        energy_fn: F,
        max_epochs: usize,
    ) -> ETCoolingResult
    where
        F: Fn(&[usize]) -> f64,
    {
        let n_wcn_nodes = (self.dim * 2).max(8);
        let mut wcn = WeightChainNetwork::new(
            n_wcn_nodes, self.dim,
            self.wcn_bias, self.wcn_threshold,
            &mut self.rng,
        );
        let mut scheduler = ETCoolingScheduler::new(
            self.t_max, self.t_min, self.delta_crit, self.beta1_target,
        );

        let mut current = initial.clone();
        let mut current_energy = energy_fn(&current);
        let mut best = current.clone();
        let mut best_energy = current_energy;
        let mut energy_history: Vec<f64> = vec![current_energy];

        for epoch in 0..max_epochs {
            // ── 1. Compute Δ from energy history ──────────────────────────────
            let (a, b) = self.solution_to_ab(&energy_history, scheduler.stagnation);
            let delta = symmetric_gap(a, b);

            // ── 2. β₁ from WCN topology ───────────────────────────────────────
            let beta1 = wcn.beta1();

            // ── 3. Update temperature ─────────────────────────────────────────
            scheduler.step(delta, beta1, current_energy);

            // ── 4. Generate neighbour ─────────────────────────────────────────
            let neighbor = self.generate_neighbor(&current, n_choices, &wcn);

            // ── 5. Evaluate ───────────────────────────────────────────────────
            let neighbor_energy = energy_fn(&neighbor);

            // ── 6. Metropolis accept/reject ───────────────────────────────────
            if scheduler.should_accept(current_energy, neighbor_energy, &mut self.rng) {
                // Update WCN with accepted solution embedding
                let emb: Vec<f64> = neighbor.iter()
                    .map(|&x| x as f64 / n_choices.max(1) as f64)
                    .collect();
                wcn.update(&emb, 0.3);
                current = neighbor;
                current_energy = neighbor_energy;
            }

            energy_history.push(current_energy);
            if energy_history.len() > 50 { energy_history.remove(0); }

            // ── 7. Track best ─────────────────────────────────────────────────
            if current_energy < best_energy {
                best_energy = current_energy;
                best = current.clone();
            }

            // ── 8. Reheat if stagnant in breaking phase ───────────────────────
            if scheduler.should_reheat(self.stagnation_threshold) {
                scheduler.reheat();
            }

            // ── 9. Convergence check ──────────────────────────────────────────
            if epoch > 50 && scheduler.is_converged(self.patience) {
                return ETCoolingResult {
                    best_solution: best,
                    best_energy,
                    epochs: epoch + 1,
                    final_phase: scheduler.phase,
                    final_temperature: scheduler.temperature,
                };
            }
        }

        ETCoolingResult {
            best_solution: best,
            best_energy,
            epochs: max_epochs,
            final_phase: scheduler.phase,
            final_temperature: scheduler.temperature,
        }
    }

    /// Generate a neighbour by flipping one randomly-chosen dimension.
    /// Uses WCN to bias the flip toward less-represented choices.
    fn generate_neighbor(
        &mut self,
        current: &[usize],
        n_choices: usize,
        wcn: &WeightChainNetwork,
    ) -> Vec<usize> {
        let mut neighbor = current.to_vec();
        let pos = self.rng.gen_range(0..current.len());

        // Try WCN-guided suggestion first
        let wcn_node = self.rng.gen_range(0..wcn.embeddings.len());
        let new_val = if let Some(nb) = wcn.weighted_neighbor(wcn_node, &mut self.rng) {
            nb % n_choices
        } else {
            self.rng.gen_range(0..n_choices)
        };

        // Ensure we actually change something
        if new_val != current[pos] {
            neighbor[pos] = new_val;
        } else {
            neighbor[pos] = (current[pos] + 1) % n_choices;
        }
        neighbor
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ETCoolingPolicy — SchedulerPolicy backed by ETWCNCooling
// ─────────────────────────────────────────────────────────────────────────────

/// Scheduling policy that uses the ET-WCN cooling algorithm to find an
/// optimal partition→node assignment by minimising:
///   energy = Σ route_cost(src_node, dst_node) for each inter-partition edge
///            + imbalance_weight × load_imbalance_penalty
///
/// Falls back to `MultifactorPolicy`-style tag/zone scoring for the initial
/// solution and for tag eligibility checks.
pub struct ETCoolingPolicy {
    pub max_epochs: usize,
    pub t_max: f64,
    pub t_min: f64,
    pub imbalance_weight: f64,
    pub seed: Option<u64>,
}

impl ETCoolingPolicy {
    pub fn new() -> Self {
        Self {
            max_epochs: 300,
            t_max: 2.0,
            t_min: 0.01,
            imbalance_weight: 0.5,
            seed: None,
        }
    }

    /// Optimise the full partition binding for a graph in one shot.
    /// Returns `HashMap<partition_id → node_id>`.
    pub fn optimise_binding(
        &self,
        graph: &IRGraph,
        partitions: &HashMap<String, String>,
        nodes: &HashMap<String, Node>,
    ) -> HashMap<String, String> {
        // Collect unique partitions and eligible nodes per partition
        let partition_ids: Vec<String> = {
            let mut set: std::collections::BTreeSet<String> = Default::default();
            for p in partitions.values() { set.insert(p.clone()); }
            set.into_iter().collect()
        };

        // Stable node ordering — used by both `eligible` and the SA result lookup.
        let node_ids: Vec<String> = {
            let mut v: Vec<String> = nodes.keys().cloned().collect();
            v.sort();  // deterministic order regardless of HashMap iteration
            v
        };
        let n_nodes = node_ids.len();

        // For each partition, find eligible node *indices* into node_ids
        // (must match required_tag; if no tag constraint, all nodes eligible).
        let eligible: Vec<Vec<usize>> = partition_ids.iter().map(|pid| {
            let tags: HashSet<String> = graph.blocks.iter()
                .filter(|b| partitions.get(&b.block_id).map(|p| p == pid).unwrap_or(false))
                .flat_map(|b| std::iter::once(b.required_tag.clone()))
                .filter(|t| !t.is_empty())
                .collect();

            node_ids.iter().enumerate()
                .filter(|(_, nid)| {
                    let node = &nodes[*nid];
                    tags.is_empty() || tags.iter().all(|t| node.has_tag(t))
                })
                .map(|(i, _)| i)
                .collect()
        }).collect();
        if n_nodes == 0 || partition_ids.is_empty() {
            return HashMap::new();
        }

        // Initial solution: greedy tag+zone scoring (same as MultifactorPolicy)
        let initial: Vec<usize> = partition_ids.iter().enumerate().map(|(pi, pid)| {
            let tags: HashSet<String> = graph.blocks.iter()
                .filter(|b| partitions.get(&b.block_id).map(|p| p == pid).unwrap_or(false))
                .flat_map(|b| std::iter::once(b.required_tag.clone()))
                .filter(|t| !t.is_empty())
                .collect();
            let zone = graph.blocks.iter()
                .find(|b| partitions.get(&b.block_id).map(|p| p == pid).unwrap_or(false))
                .map(|b| b.preferred_zone.as_str())
                .unwrap_or("");

            let best_ni = eligible[pi].iter()
                .max_by(|&&a, &&b| {
                    let na = &nodes[&node_ids[a]];
                    let nb = &nodes[&node_ids[b]];
                    let sa: f32 = tags.iter().map(|t| node_score(t, zone, None, na)).sum();
                    let sb: f32 = tags.iter().map(|t| node_score(t, zone, None, nb)).sum();
                    sa.partial_cmp(&sb).unwrap()
                })
                .copied();
            best_ni.unwrap_or(0)
        }).collect();

        // Energy function: total cross-node route cost + load imbalance
        let imbalance_w = self.imbalance_weight;
        let energy_fn = |sol: &[usize]| -> f64 {
            // Route cost: for each IRGraph edge, cost of routing from src-partition-node to dst-partition-node
            let partition_node: HashMap<&str, &str> = partition_ids.iter().enumerate()
                .map(|(i, pid)| (pid.as_str(), node_ids[sol[i]].as_str()))
                .collect();

            let route_cost_sum: f64 = graph.edges.iter().map(|e| {
                let sp = partitions.get(&e.src_block).map(|p| p.as_str());
                let dp = partitions.get(&e.dst_block).map(|p| p.as_str());
                if let (Some(sp), Some(dp)) = (sp, dp) {
                    if sp == dp { return 0.0; }
                    let sn = partition_node.get(sp).and_then(|nid| nodes.get(*nid));
                    let dn = partition_node.get(dp).and_then(|nid| nodes.get(*nid));
                    if let (Some(sn), Some(dn)) = (sn, dn) {
                        return route_cost(sn, dn) as f64;
                    }
                }
                0.0
            }).sum();

            // Load imbalance: std-dev of blocks-per-node
            let mut load = vec![0usize; n_nodes];
            for (i, _pid) in partition_ids.iter().enumerate() {
                load[sol[i]] += 1;
            }
            let mean = load.iter().sum::<usize>() as f64 / n_nodes as f64;
            let imbalance = load.iter()
                .map(|&l| (l as f64 - mean).powi(2))
                .sum::<f64>()
                .sqrt();

            route_cost_sum + imbalance_w * imbalance
        };

        // Wrap the energy function: penalise any solution that violates eligible
        // constraints with a large cost so SA never accepts such moves.
        let energy_fn_constrained = |sol: &[usize]| -> f64 {
            for (pi, &ni) in sol.iter().enumerate() {
                if !eligible[pi].is_empty() && !eligible[pi].contains(&ni) {
                    return f64::MAX / 2.0;
                }
            }
            energy_fn(sol)
        };

        // Run ET-WCN cooling
        let mut optimizer = ETWCNCooling::new(partition_ids.len())
            .with_temperature(self.t_max, self.t_min);
        if let Some(seed) = self.seed {
            optimizer = optimizer.with_seed(seed);
        }

        let result = optimizer.optimize(initial, n_nodes, energy_fn_constrained, self.max_epochs);

        // Project result back to eligible nodes (safety net in case SA drifted).
        let final_sol: Vec<usize> = result.best_solution.iter().enumerate().map(|(pi, &ni)| {
            if eligible[pi].is_empty() || eligible[pi].contains(&ni) {
                ni
            } else {
                // Fallback: pick the highest-scoring eligible node.
                eligible[pi][0]
            }
        }).collect();

        // Build output map
        partition_ids.iter().enumerate()
            .map(|(i, pid)| (pid.clone(), node_ids[final_sol[i]].clone()))
            .collect()
    }
}

/// Per-partition greedy fallback used by `select_partition_node`.
impl SchedulerPolicy for ETCoolingPolicy {
    fn select_partition_node(
        &self,
        required_tags: &HashSet<String>,
        preferred_zone: &str,
        inertia_key: Option<&str>,
        nodes: &HashMap<String, Node>,
    ) -> String {
        nodes.values()
            .map(|n| {
                let score: f32 = required_tags.iter()
                    .map(|t| node_score(t, preferred_zone, inertia_key, n))
                    .sum();
                (score, n.node_id.clone())
            })
            .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
            .expect("no nodes available")
            .1
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symmetric_gap() {
        // Δ(a,b) = (a-1)(b-1) - 1
        // Δ(1,1) = 0*0 - 1 = -1
        assert!((symmetric_gap(1.0, 1.0) - (-1.0)).abs() < 1e-10);
        // Δ(2,2) = 1*1 - 1 = 0  ← symmetric mirror point
        assert!((symmetric_gap(2.0, 2.0)).abs() < 1e-10);
        // Δ(3,3) = 2*2 - 1 = 3
        assert!((symmetric_gap(3.0, 3.0) - 3.0).abs() < 1e-10);
        // Δ(2,3) = 1*2 - 1 = 1
        assert!((symmetric_gap(2.0, 3.0) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_compute_beta1_tree() {
        // A path graph 0-1-2 is a tree: E=2, V=3, C=1 → β₁ = 0
        let mut adj: HashMap<usize, HashSet<usize>> = HashMap::new();
        adj.entry(0).or_default().insert(1);
        adj.entry(1).or_default().insert(0);
        adj.entry(1).or_default().insert(2);
        adj.entry(2).or_default().insert(1);
        assert_eq!(compute_beta1(&adj, 3), 0);
    }

    #[test]
    fn test_compute_beta1_cycle() {
        // Triangle 0-1-2-0: E=3, V=3, C=1 → β₁ = 1
        let mut adj: HashMap<usize, HashSet<usize>> = HashMap::new();
        for (a, b) in [(0,1),(1,2),(2,0)] {
            adj.entry(a).or_default().insert(b);
            adj.entry(b).or_default().insert(a);
        }
        assert_eq!(compute_beta1(&adj, 3), 1);
    }

    #[test]
    fn test_et_scheduler_temperature() {
        let sched = ETCoolingScheduler::new(1.0, 0.01, 5.0, 0);
        // Δ=0 → T = T_max
        let t = sched.compute_temperature(0.0, 0);
        assert!((t - 1.0).abs() < 1e-6);
        // Δ>0 → T < T_max
        let t2 = sched.compute_temperature(1.0, 0);
        assert!(t2 < t);
        // β₁>target → T reduced further
        let t3 = sched.compute_temperature(1.0, 2);
        assert!(t3 < t2);
    }

    #[test]
    fn test_metropolis_always_accepts_improvement() {
        let sched = ETCoolingScheduler::new(1.0, 0.01, 5.0, 0);
        let mut rng = SmallRng::seed_from_u64(42);
        // ΔE < 0 → always accept
        for _ in 0..100 {
            assert!(sched.should_accept(10.0, 5.0, &mut rng));
        }
    }

    #[test]
    fn test_optimize_minimises_energy() {
        // Optimise: find x ∈ {0..9} that minimises |x - 7|²
        let mut opt = ETWCNCooling::new(1).with_seed(0);
        let result = opt.optimize(vec![0], 10, |s| (s[0] as f64 - 7.0).powi(2), 500);
        assert_eq!(result.best_solution[0], 7,
            "expected 7, got {}", result.best_solution[0]);
        assert!(result.best_energy < 1.0);
    }

    #[test]
    fn test_optimize_2d() {
        // Minimise (x-3)² + (y-5)²  over x,y ∈ {0..9}
        let mut opt = ETWCNCooling::new(2).with_seed(1);
        let result = opt.optimize(vec![0, 0], 10,
            |s| (s[0] as f64 - 3.0).powi(2) + (s[1] as f64 - 5.0).powi(2),
            1000);
        assert_eq!(result.best_solution[0], 3);
        assert_eq!(result.best_solution[1], 5);
    }
}
