//! LLM-driven orchestration for the God Role: decompose a goal, match subtasks to
//! roles (or signal a hire), and aggregate results into a final answer.
//!
//! Every LLM step falls back to a heuristic when the call fails or yields nothing
//! usable, so orchestration degrades gracefully without an LLM.

use crate::discover::DiscoverEngine;
use crate::types::*;
use serde::Deserialize;
use std::sync::Arc;
use xuanji_llm::{LlmProvider, Message};

pub struct RoleOrchestrator {
    provider: Arc<dyn LlmProvider>,
}

impl RoleOrchestrator {
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self { provider }
    }

    /// Decompose a goal into subtasks (LLM, fallback heuristic).
    pub async fn decompose(&self, goal: &str) -> Vec<SubTask> {
        match self.llm_decompose(goal).await {
            Some(tasks) if !tasks.is_empty() => tasks,
            _ => DiscoverEngine::decompose(goal),
        }
    }

    /// Assign each subtask to a role, or produce a hire signal (LLM, fallback heuristic).
    pub async fn assign(&self, subtasks: &[SubTask], roles: &[RoleProfile]) -> Vec<Assignment> {
        match self.llm_assign(subtasks, roles).await {
            Some(a) if a.len() == subtasks.len() => a,
            _ => heuristic_assignments(subtasks, roles),
        }
    }

    /// Aggregate subtask results into a single user-facing answer.
    pub async fn aggregate(&self, goal: &str, results: &[(String, String)]) -> String {
        if results.is_empty() {
            return String::new();
        }
        if results.len() == 1 {
            return results[0].1.clone();
        }
        self.llm_aggregate(goal, results).await.unwrap_or_else(|| {
            results
                .iter()
                .map(|(d, r)| format!("### {}\n{}", d, r))
                .collect::<Vec<_>>()
                .join("\n\n")
        })
    }

    // ─── LLM calls ───

    async fn ask(&self, system: &str, user: &str) -> Option<String> {
        let messages = vec![
            Message::System {
                content: system.to_string(),
            },
            Message::User {
                content: user.to_string(),
            },
        ];
        let resp = self.provider.complete(&messages, &[]).await.ok()?;
        resp.text_content()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    async fn llm_decompose(&self, goal: &str) -> Option<Vec<SubTask>> {
        let system = "你是任务分解助手。把用户目标拆成可独立执行的子任务。只输出 JSON 数组，不要任何解释。";
        let user = format!(
            "目标：{goal}\n\
             输出格式：[{{\"description\":\"子任务描述\",\"needed_expertise\":\"安全|性能|文档|测试|部署|构建|其他或留空\"}}]\n\
             若无需拆分，返回只含一个元素（即原目标）的数组。"
        );
        let text = self.ask(system, &user).await?;
        let raw = extract_json(&text)?;
        let items: Vec<DecomposeItem> = serde_json::from_str(&raw).ok()?;
        Some(
            items
                .into_iter()
                .filter(|i| !i.description.trim().is_empty())
                .map(|it| SubTask {
                    description: it.description,
                    depends_on: Vec::new(),
                    required_skill: (!it.needed_expertise.trim().is_empty()).then_some(it.needed_expertise),
                    assignee: None,
                    result: None,
                })
                .collect(),
        )
    }

    async fn llm_assign(
        &self,
        subtasks: &[SubTask],
        roles: &[RoleProfile],
    ) -> Option<Vec<Assignment>> {
        let system = "你是任务分配助手。把每个子任务分配给最合适的现有角色；若没有合适角色，则建议 hire。只输出 JSON 数组。";
        let roster = if roles.is_empty() {
            "（暂无其他角色）".to_string()
        } else {
            roles
                .iter()
                .map(|r| format!("- {}：{}", r.name, r.seed_purpose))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let tasks_json = subtasks
            .iter()
            .enumerate()
            .map(|(i, t)| format!("{}. {}", i, t.description))
            .collect::<Vec<_>>()
            .join("\n");
        let user = format!(
            "现有角色：\n{roster}\n\n子任务：\n{tasks_json}\n\n\
             输出格式：[{{\"task_idx\":0,\"description\":\"对应子任务描述\",\"assignee\":\"角色名或null\",\"hire\":\"需新建角色的职责描述或null\"}}]\n\
             assignee 与 hire 二选一：能匹配现有角色就用 assignee，否则用 hire。task_idx 必须与上面子任务序号一一对应。"
        );
        let text = self.ask(system, &user).await?;
        let raw = extract_json(&text)?;
        let items: Vec<MatchItem> = serde_json::from_str(&raw).ok()?;
        // Reorder by task_idx to align with subtasks, and carry the authoritative description.
        let mut out = Vec::with_capacity(subtasks.len());
        for (i, task) in subtasks.iter().enumerate() {
            let m = items.iter().find(|m| m.task_idx == i);
            let (assignee, hire) = match m {
                Some(m) => (m.assignee.clone(), m.hire.clone()),
                None => (None, None),
            };
            out.push(Assignment {
                description: task.description.clone(),
                assignee: assignee.filter(|s| !s.is_empty()),
                hire: hire.filter(|s| !s.is_empty()),
            });
        }
        Some(out)
    }

    async fn llm_aggregate(&self, goal: &str, results: &[(String, String)]) -> Option<String> {
        let system = "你是结果汇总助手。把多个子任务的执行结果整合成一份面向用户的最终回答（markdown）。";
        let parts = results
            .iter()
            .map(|(d, r)| format!("## 子任务：{d}\n{r}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        let user = format!("原始目标：{goal}\n\n各子任务结果：\n{parts}\n\n请整合为最终回答。");
        self.ask(system, &user).await
    }
}

#[derive(Deserialize)]
struct DecomposeItem {
    #[serde(default)]
    description: String,
    #[serde(default)]
    needed_expertise: String,
}

#[derive(Deserialize)]
struct MatchItem {
    #[serde(default)]
    task_idx: usize,
    #[serde(default)]
    assignee: Option<String>,
    #[serde(default)]
    hire: Option<String>,
}

/// Heuristic fallback: use the existing keyword skill inference + role-purpose matching.
pub fn heuristic_assignments(subtasks: &[SubTask], roles: &[RoleProfile]) -> Vec<Assignment> {
    subtasks
        .iter()
        .map(|task| {
            let assignee = task.required_skill.as_deref().and_then(|skill| {
                let lower_purpose_collect: Vec<(String, String)> = roles
                    .iter()
                    .map(|r| (r.name.clone(), r.seed_purpose.to_lowercase()))
                    .collect();
                lower_purpose_collect
                    .into_iter()
                    .find(|(_, p)| skill_matches(skill, p))
                    .map(|(name, _)| name)
            });
            let hire = if assignee.is_none() {
                task.required_skill.clone()
            } else {
                None
            };
            Assignment {
                description: task.description.clone(),
                assignee,
                hire,
            }
        })
        .collect()
}

fn skill_matches(skill: &str, lower_purpose: &str) -> bool {
    match skill {
        "security" | "安全" => {
            lower_purpose.contains("安全") || lower_purpose.contains("security")
        }
        "performance" | "性能" => {
            lower_purpose.contains("性能") || lower_purpose.contains("performance")
        }
        "documentation" | "文档" => {
            lower_purpose.contains("文档") || lower_purpose.contains("doc")
        }
        "testing" | "测试" => {
            lower_purpose.contains("测试") || lower_purpose.contains("test")
        }
        "devops" | "部署" => {
            lower_purpose.contains("部署") || lower_purpose.contains("devops")
        }
        "build" | "构建" => {
            lower_purpose.contains("构建") || lower_purpose.contains("build")
        }
        other => lower_purpose.contains(other),
    }
}

/// Extract the first JSON array/object fragment from text that may include markdown
/// fences or surrounding prose. Returns the slice inclusive of the brackets.
fn extract_json(text: &str) -> Option<String> {
    let t = text.trim();
    let t = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```"))
        .unwrap_or(t);
    let t = t.trim_end_matches("```");
    let start = t.find('[').or_else(|| t.find('{'))?;
    let end = t.rfind(']').or_else(|| t.rfind('}'))?;
    if end >= start {
        Some(t[start..=end].to_string())
    } else {
        None
    }
}
