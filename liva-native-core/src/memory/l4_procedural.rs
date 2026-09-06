use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub const DEFAULT_ALPHA_PRIOR: f64 = 2.0;
pub const DEFAULT_BETA_PRIOR: f64 = 1.0;
pub const LAMBDA_PENALTY: f64 = 0.35;
pub const FAILURE_SATURATION_MU: f64 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FailureType {
    SemanticIssue,
    Crash,
    Timeout,
    Uninvoked,
}

impl FailureType {
    pub fn weight(&self) -> f64 {
        match self {
            FailureType::SemanticIssue => 1.0,
            FailureType::Crash => 1.0,
            FailureType::Timeout => 0.5,
            FailureType::Uninvoked => 0.25,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            FailureType::SemanticIssue => "semantic_issue",
            FailureType::Crash => "crash",
            FailureType::Timeout => "timeout",
            FailureType::Uninvoked => "uninvoked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ExecutionOutcome {
    Success,
    Failure {
        failure_type: FailureType,
        severity: f64,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProceduralSkill {
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub code_or_prompt: String,
    pub alpha: f64,
    pub beta: f64,
    pub success_count: u32,
    pub failure_count: u32,
    pub failure_tallies: HashMap<String, u32>,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub last_used_at: i64,
}

impl ProceduralSkill {
    pub fn new(skill_id: String, name: String, description: String, code_or_prompt: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        Self {
            skill_id,
            name,
            description,
            code_or_prompt,
            alpha: DEFAULT_ALPHA_PRIOR,
            beta: DEFAULT_BETA_PRIOR,
            success_count: 0,
            failure_count: 0,
            failure_tallies: HashMap::new(),
            tags: Vec::new(),
            created_at: now,
            last_used_at: now,
        }
    }

    /// Expected execution success rate: E[\theta] = \alpha / (\alpha + \beta)
    pub fn expected_success_rate(&self) -> f64 {
        compute_bayesian_expectation(self.alpha, self.beta)
    }

    /// Variance of Beta distribution: Var(\theta) = (\alpha * \beta) / [(\alpha + \beta)^2 * (\alpha + \beta + 1)]
    pub fn success_variance(&self) -> f64 {
        let sum = self.alpha + self.beta;
        if sum <= 0.0 {
            return 0.0;
        }
        (self.alpha * self.beta) / (sum * sum * (sum + 1.0))
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RankedSkill {
    pub skill: ProceduralSkill,
    pub similarity: f64,
    pub expected_success_rate: f64,
    pub quality_multiplier: f64,
    pub final_rank_score: f64,
}

/// Calculate expected value E[\theta] of Beta(\alpha, \beta)
pub fn compute_bayesian_expectation(alpha: f64, beta: f64) -> f64 {
    let sum = alpha + beta;
    if sum > 0.0 {
        alpha / sum
    } else {
        0.0
    }
}

/// Calculate ranking score penalizing lower historical success rates:
/// Score(s, q) = S_sim(s, q) * [1.0 - \lambda * (1.0 - E[\theta]) * \sigma(distinct_fails / \mu)]
pub fn compute_ranking_score(similarity: f64, alpha: f64, beta: f64, total_fails: u32) -> (f64, f64) {
    let expected = compute_bayesian_expectation(alpha, beta);
    let x = (total_fails as f64) / FAILURE_SATURATION_MU;
    let sigma = x / (1.0 + x);
    let quality_multiplier = (1.0 - LAMBDA_PENALTY * (1.0 - expected) * sigma).clamp(0.0, 1.0);
    let final_score = similarity * quality_multiplier;
    (quality_multiplier, final_score)
}

/// L4 Procedural Memory Registry
/// Thread-safe skill repository with Bayesian success rate updates and quality ranking.
pub struct L4ProceduralRegistry {
    skills: Arc<RwLock<HashMap<String, ProceduralSkill>>>,
}

impl Default for L4ProceduralRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl L4ProceduralRegistry {
    pub fn new() -> Self {
        Self {
            skills: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new skill into the procedural memory.
    pub async fn register_skill(&self, skill: ProceduralSkill) -> String {
        let mut map = self.skills.write().await;
        let id = skill.skill_id.clone();
        map.insert(id.clone(), skill);
        id
    }

    /// Retrieve a skill by its ID.
    pub async fn get_skill(&self, skill_id: &str) -> Option<ProceduralSkill> {
        let map = self.skills.read().await;
        map.get(skill_id).cloned()
    }

    /// List all registered skills.
    pub async fn list_skills(&self) -> Vec<ProceduralSkill> {
        let map = self.skills.read().await;
        map.values().cloned().collect()
    }

    /// Delete a skill by its ID.
    pub async fn delete_skill(&self, skill_id: &str) -> bool {
        let mut map = self.skills.write().await;
        map.remove(skill_id).is_some()
    }

    /// Record an execution outcome and update Bayesian posterior parameters (\alpha, \beta).
    pub async fn record_outcome(
        &self,
        skill_id: &str,
        outcome: &ExecutionOutcome,
    ) -> Result<(f64, f64), String> {
        let mut map = self.skills.write().await;
        let skill = map.get_mut(skill_id)
            .ok_or_else(|| format!("Skill '{skill_id}' not found in procedural registry"))?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        skill.last_used_at = now;

        match outcome {
            ExecutionOutcome::Success => {
                skill.alpha += 1.0;
                skill.success_count += 1;
            }
            ExecutionOutcome::Failure { failure_type, severity, .. } => {
                let weight = failure_type.weight();
                let effective_severity = severity.clamp(0.1, 2.0);
                skill.beta += weight * effective_severity;
                skill.failure_count += 1;
                let tally = skill.failure_tallies.entry(failure_type.as_str().to_string()).or_insert(0);
                *tally += 1;
            }
        }

        Ok((skill.alpha, skill.beta))
    }

    /// Rank skills according to semantic/BM25 similarity and Bayesian quality priors.
    pub async fn rank_skills(&self, query_similarities: &[(String, f64)]) -> Vec<RankedSkill> {
        let map = self.skills.read().await;
        let mut ranked = Vec::new();

        for (skill_id, sim) in query_similarities {
            if let Some(skill) = map.get(skill_id) {
                let (multiplier, final_score) = compute_ranking_score(
                    *sim,
                    skill.alpha,
                    skill.beta,
                    skill.failure_count,
                );

                ranked.push(RankedSkill {
                    skill: skill.clone(),
                    similarity: *sim,
                    expected_success_rate: skill.expected_success_rate(),
                    quality_multiplier: multiplier,
                    final_rank_score: final_score,
                });
            }
        }

        // Deterministic sorting: final score descending, ID ascending on tie
        ranked.sort_by(|a, b| {
            b.final_rank_score
                .partial_cmp(&a.final_rank_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.skill.skill_id.cmp(&b.skill.skill_id))
        });

        ranked
    }
}
