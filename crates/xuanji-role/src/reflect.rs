use crate::teaching::TeachingLibrary;
use crate::types::*;
use std::collections::HashMap;

/// Reflection and learning engine.
pub struct LearningEngine;

impl LearningEngine {
    /// Reflect on a completed goal and produce learning artifacts.
    pub fn reflect_on_goal(
        outcome: &GoalOutcome,
        rules: &mut Vec<Rule>,
        cases: &mut Vec<CaseEntry>,
        preferences: &mut HashMap<String, ToolPreference>,
    ) {
        // 1. Record as case
        let outcome_enum = if outcome.success {
            CaseOutcome::Success
        } else {
            CaseOutcome::Failure {
                reason: outcome.lessons.clone(),
            }
        };

        let case = CaseEntry {
            id: format!("case-{}", chrono_now_compact()),
            task_description: outcome.summary.clone(),
            context_tags: extract_tags(&outcome.summary),
            strategy_used: format!("{} tool calls", outcome.tool_calls_count),
            outcome: outcome_enum,
            lessons: outcome.lessons.clone(),
            created_at: chrono_now(),
        };
        cases.push(case);

        // 2. Derive rule from lessons (only for successful outcomes with lessons)
        if !outcome.lessons.is_empty() && outcome.success {
            let rule = Rule {
                id: format!("rule-{}", chrono_now_compact()),
                condition: format!("Working on: {}", outcome.summary),
                action: format!("Strategy: {}", outcome.lessons),
                confidence: 0.6,
                source_case_id: Some(
                    cases.last().map(|c| c.id.clone()).unwrap_or_default(),
                ),
                validated_count: 0,
                created_at: chrono_now(),
            };
            rules.push(rule);
        }

        // 3. Update tool preferences
        let tool_name = "shell.run".to_string();
        let pref = preferences
            .entry(tool_name.clone())
            .or_insert_with(|| ToolPreference::new(&tool_name));
        pref.record_call(outcome.success, outcome.tokens_used);
    }

    /// Generate a teaching from the most confident rules.
    pub fn generate_teaching(
        author_role: &str,
        rules: &[Rule],
        teaching_lib: &mut TeachingLibrary,
    ) -> Result<Vec<String>, crate::error::RoleError> {
        let mut published_ids = Vec::new();
        for rule in rules {
            if rule.confidence >= 0.7 && rule.validated_count >= 2 {
                let teaching = Teaching::new(
                    author_role,
                    TeachingKind::Rule,
                    &format!("当: {}\n则: {}", rule.condition, rule.action),
                    extract_tags(&rule.condition),
                );
                let id = teaching.id.clone();
                teaching_lib.publish(teaching)?;
                published_ids.push(id);
            }
        }
        Ok(published_ids)
    }
}

/// Extract keyword tags from text.
fn extract_tags(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter(|w| w.len() >= 3)
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| !w.is_empty())
        .take(5)
        .collect()
}

fn chrono_now() -> String {
    chrono::Local::now()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

fn chrono_now_compact() -> String {
    chrono::Local::now()
        .format("%Y%m%d%H%M%S")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reflect_creates_case_and_rule() {
        let outcome = GoalOutcome {
            goal_id: "g1".into(),
            success: true,
            summary: "Profiled project build".into(),
            tool_calls_count: 5,
            tokens_used: 2000,
            lessons: "Use cargo build --release for accurate profiling".into(),
        };

        let mut rules = Vec::new();
        let mut cases = Vec::new();
        let mut prefs = HashMap::new();

        LearningEngine::reflect_on_goal(&outcome, &mut rules, &mut cases, &mut prefs);

        assert_eq!(cases.len(), 1);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].confidence, 0.6);
    }

    #[test]
    fn test_reflect_failure_no_rule() {
        let outcome = GoalOutcome {
            goal_id: "g2".into(),
            success: false,
            summary: "Reduce binary size".into(),
            tool_calls_count: 3,
            tokens_used: 1000,
            lessons: "".into(),
        };

        let mut rules = Vec::new();
        let mut cases = Vec::new();
        let mut prefs = HashMap::new();

        LearningEngine::reflect_on_goal(&outcome, &mut rules, &mut cases, &mut prefs);

        assert_eq!(cases.len(), 1);
        assert_eq!(rules.len(), 0);
        // Tool preference should still be recorded
        assert_eq!(prefs["shell.run"].total_calls, 1);
    }

    #[test]
    fn test_teaching_generation_threshold() {
        let mut lib = crate::TeachingLibrary::new_empty();

        let rules = vec![
            Rule {
                id: "r1".into(),
                condition: "Working on builds".into(),
                action: "Use --release flag".into(),
                confidence: 0.8,
                source_case_id: None,
                validated_count: 3,
                created_at: "now".into(),
            },
            Rule {
                id: "r2".into(),
                condition: "Working on tests".into(),
                action: "Run with --nocapture".into(),
                confidence: 0.5,
                source_case_id: None,
                validated_count: 1,
                created_at: "now".into(),
            },
        ];

        let ids = LearningEngine::generate_teaching("test-role", &rules, &mut lib).unwrap();
        assert_eq!(ids.len(), 1);
    }
}
