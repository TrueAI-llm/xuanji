use crate::types::*;

/// Discover candidate goals based on the role's purpose and knowledge.
pub struct DiscoverEngine;

impl DiscoverEngine {
    /// Generate candidate goals.
    /// In full implementation, this calls LLM.
    /// For MVP, we provide deterministic candidates for testing.
    pub fn discover(
        profile: &RoleProfile,
        _rules: &[Rule],
        cases: &[CaseEntry],
    ) -> Vec<GoalCandidate> {
        let mut candidates = Vec::new();

        // Candidate 1: Always re-evaluate seed purpose
        candidates.push(GoalCandidate {
            description: format!("重新评估进度: {}", profile.seed_purpose),
            rationale: "定期自我评估".into(),
            relevance_score: 1.0,
            exploration_score: 0.3,
        });

        // Candidate 2: From successful case domains
        let success_tags: Vec<String> = cases
            .iter()
            .filter(|c| matches!(c.outcome, CaseOutcome::Success))
            .flat_map(|c| c.context_tags.clone())
            .collect();
        if let Some(tag) = success_tags.first() {
            candidates.push(GoalCandidate {
                description: format!("深化领域专业知识: {}", tag),
                rationale: "建立在已验证的成功模式之上".into(),
                relevance_score: 0.8,
                exploration_score: 0.5,
            });
        }

        // Candidate 3: From failure cases (improvement)
        let failure_tags: Vec<String> = cases
            .iter()
            .filter(|c| matches!(c.outcome, CaseOutcome::Failure { .. }))
            .flat_map(|c| c.context_tags.clone())
            .collect();
        if let Some(tag) = failure_tags.first() {
            candidates.push(GoalCandidate {
                description: format!("重试和改进: {}", tag),
                rationale: "从失败中学习".into(),
                relevance_score: 0.9,
                exploration_score: 0.7,
            });
        }

        candidates
    }

    /// Score and prioritize candidates.
    pub fn prioritize(
        candidates: Vec<GoalCandidate>,
        _budget_remaining: u32,
    ) -> Vec<ScoredCandidate> {
        let mut scored: Vec<ScoredCandidate> = candidates
            .into_iter()
            .map(|c| {
                let score = 0.5 * c.relevance_score
                    + 0.3 * c.exploration_score
                    + 0.2 * (1.0 - c.exploration_score * 0.5);
                ScoredCandidate {
                    description: c.description,
                    rationale: c.rationale,
                    score,
                }
            })
            .collect();
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored
    }

    /// Decompose a goal description into sub-tasks.
    /// MVP: simple heuristic decomposition.
    pub fn decompose(goal_description: &str) -> Vec<SubTask> {
        vec![SubTask {
            description: goal_description.to_string(),
            depends_on: Vec::new(),
            result: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_generates_candidates() {
        let profile = RoleProfile::new("test", "审计项目安全");
        let candidates = DiscoverEngine::discover(&profile, &[], &[]);
        assert!(!candidates.is_empty());
        assert!(candidates[0].description.contains("审计项目安全"));
    }

    #[test]
    fn test_discover_from_cases() {
        let profile = RoleProfile::new("test", "优化性能");
        let cases = vec![
            CaseEntry {
                id: "1".into(),
                task_description: "Profile build".into(),
                context_tags: vec!["rust".into(), "build".into()],
                strategy_used: "cargo build --timings".into(),
                outcome: CaseOutcome::Success,
                lessons: "Use --timings flag".into(),
                created_at: "2026-01-01".into(),
            },
            CaseEntry {
                id: "2".into(),
                task_description: "Reduce binary size".into(),
                context_tags: vec!["rust".into(), "optimize".into()],
                strategy_used: "strip".into(),
                outcome: CaseOutcome::Failure {
                    reason: "Still too large".into(),
                },
                lessons: "Need LTO".into(),
                created_at: "2026-01-02".into(),
            },
        ];
        let candidates = DiscoverEngine::discover(&profile, &[], &cases);
        assert!(
            candidates.len() >= 2,
            "Expected >= 2 candidates, got {}",
            candidates.len()
        );
    }

    #[test]
    fn test_prioritize_orders_by_score() {
        let candidates = vec![
            GoalCandidate {
                description: "A".into(),
                rationale: "r".into(),
                relevance_score: 0.3,
                exploration_score: 0.1,
            },
            GoalCandidate {
                description: "B".into(),
                rationale: "r".into(),
                relevance_score: 0.9,
                exploration_score: 0.5,
            },
        ];
        let scored = DiscoverEngine::prioritize(candidates, 10000);
        assert_eq!(scored[0].description, "B");
        assert_eq!(scored[1].description, "A");
    }

    #[test]
    fn test_decompose_produces_subtasks() {
        let tasks = DiscoverEngine::decompose("分析代码质量");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].description, "分析代码质量");
        assert!(tasks[0].depends_on.is_empty());
    }
}
