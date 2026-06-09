use xuanji_llm::ToolSchema;
use xuanji_memory::working::WorkingMemory;

/// Build the system prompt with tools, working memory, and optional long-term memory context.
pub fn build_system_prompt(
    tools: &[ToolSchema],
    working_memory: Option<&WorkingMemory>,
    memory_context: Option<&str>,
) -> String {
    let mut prompt = String::from(
        r#"你是 xuanji，一个自动化任务执行助手。

## 你的工作方式
1. 理解用户目标，将其拆解为可执行的子任务
2. 按顺序调用工具来完成每个子任务
3. 观察工具返回的结果，决定下一步
4. 所有子任务完成后，总结结果

## 规则
- 每次只调用必要的工具，不要做多余操作
- 如果工具调用失败，分析原因并尝试替代方案
- 如果信息不足以完成任务，向用户提问
- 完成后给出简洁的执行总结
"#,
    );

    if !tools.is_empty() {
        prompt.push_str("\n## 可用工具\n");
        for tool in tools {
            prompt.push_str(&format!("- **{}**: {}\n", tool.name, tool.description));
        }
    }

    if let Some(wm) = working_memory {
        prompt.push_str(&format!("\n{}\n", wm.to_prompt_context()));
    }

    if let Some(ctx) = memory_context {
        prompt.push_str(&format!("\n## 项目知识\n{}\n", ctx));
    }

    prompt
}
