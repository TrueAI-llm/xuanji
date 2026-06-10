use xuanji_bus::KnowledgeMessage;
use xuanji_llm::ToolSchema;
use xuanji_memory::working::WorkingMemory;

/// Build the system prompt with tools, working memory, optional long-term memory context,
/// and optional bus messages from other agents.
pub fn build_system_prompt(
    tools: &[ToolSchema],
    working_memory: Option<&WorkingMemory>,
    memory_context: Option<&str>,
    text_tool_mode: bool,
    bus_messages: Option<&[KnowledgeMessage]>,
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
        if text_tool_mode {
            prompt.push_str("\n## 可用工具\n");
            for tool in tools {
                prompt.push_str(&format!(
                    "- **{}**: {}\n  参数schema: {}\n",
                    tool.name,
                    tool.description,
                    tool.input_schema
                ));
            }
            prompt.push_str(
                r#"
## 工具调用格式（必须严格遵守）
当你需要执行任何操作时，必须使用以下格式调用工具。每次只调用一个工具。
不要用markdown代码块包裹，直接输出：

ACTION: shell.run
PARAMS: {"command": "你要执行的命令"}

等待工具返回结果后，再决定下一步操作。
当所有子任务完成后，直接输出总结文本，不要再用 ACTION 格式。

## 示例
用户: 列出当前目录的文件
ACTION: shell.run
PARAMS: {"command": "ls -la"}

## 重要
- 不要询问用户，直接开始执行
- 不要解释你要做什么，直接调用工具
- 每一步都必须使用 ACTION/PARAMS 格式
"#,
            );
        } else {
            prompt.push_str("\n## 可用工具\n");
            for tool in tools {
                prompt.push_str(&format!("- **{}**: {}\n", tool.name, tool.description));
            }
        }
    }

    if let Some(wm) = working_memory {
        prompt.push_str(&format!("\n{}\n", wm.to_prompt_context()));
    }

    if let Some(ctx) = memory_context {
        prompt.push_str(&format!("\n## 项目知识\n{}\n", ctx));
    }

    if let Some(messages) = bus_messages {
        if !messages.is_empty() {
            prompt.push_str(&xuanji_bus::KnowledgeBus::format_messages(messages));
        }
    }

    prompt
}
