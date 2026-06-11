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
    /// MVP: heuristic decomposition. Enhanced mode uses LLM.
    pub fn decompose(goal_description: &str) -> Vec<SubTask> {
        // Rule-based heuristic: detect keywords and split
        let lower = goal_description.to_lowercase();

        // "A 和 B" or "A与B" pattern → two subtasks
        if let Some(pos) = lower.find("和").or_else(|| lower.find("与")) {
            let part1 = goal_description[..pos].trim().to_string();
            let part2 = goal_description[pos + 3..].trim().to_string();

            if !part2.is_empty() {
                let mut tasks = Vec::new();

                // Infer skill from description content
                let skill1 = infer_skill(&part1);
                let skill2 = infer_skill(&part2);

                if !part1.is_empty() {
                    tasks.push(SubTask {
                        description: part1,
                        depends_on: Vec::new(),
                        required_skill: skill1,
                        assignee: None,
                        result: None,
                    });
                }
                tasks.push(SubTask {
                    description: part2,
                    depends_on: Vec::new(),
                    required_skill: skill2,
                    assignee: None,
                    result: None,
                });
                return tasks;
            }
        }

        // Default: single task, try to infer skill
        vec![SubTask {
            description: goal_description.to_string(),
            depends_on: Vec::new(),
            required_skill: infer_skill(goal_description),
            assignee: None,
            result: None,
        }]
    }
}

/// Simple keyword-based skill inference.
fn infer_skill(description: &str) -> Option<String> {
    let lower = description.to_lowercase();
    for (keyword, skill) in SKILL_KEYWORDS {
        if lower.contains(keyword) {
            return Some(skill.to_string());
        }
    }
    None
}

/// Keyword → skill mapping.
const SKILL_KEYWORDS: &[(&str, &str)] = &[
    ("安全", "security"),
    ("漏洞", "security"),
    ("审计", "security"),
    ("性能", "performance"),
    ("优化", "performance"),
    ("文档", "documentation"),
    ("doc", "documentation"),
    ("测试", "testing"),
    ("test", "testing"),
    ("部署", "devops"),
    ("deploy", "devops"),
    ("构建", "build"),
    ("build", "build"),
];

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

    #[test]
    fn test_decompose_with_he_connector() {
        let tasks = DiscoverEngine::decompose("审计安全漏洞和优化性能");
        assert!(tasks.len() >= 2, "Expected >= 2 tasks, got {}", tasks.len());
        assert!(tasks.iter().any(|t| t.description.contains("安全")));
        assert!(tasks.iter().any(|t| t.description.contains("性能")));
    }

    #[test]
    fn test_decompose_infers_skill() {
        let tasks = DiscoverEngine::decompose("审计安全漏洞和优化性能");
        assert!(tasks.iter().any(|t| t.required_skill == Some("security".into())));
        assert!(tasks.iter().any(|t| t.required_skill == Some("performance".into())));
    }

    #[test]
    fn test_decompose_single_skill() {
        let tasks = DiscoverEngine::decompose("生成项目文档");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].required_skill, Some("documentation".into()));
    }
}
