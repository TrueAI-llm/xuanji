use serde::{Deserialize, Serialize};

/// JSON Schema definition for a tool's input parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// A single tool call returned by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// The response from an LLM completion request.
#[derive(Debug, Clone)]
pub enum LlmResponse {
    ToolCalls {
        calls: Vec<ToolCall>,
        text: Option<String>,
    },
    Text {
        content: String,
    },
}

impl LlmResponse {
    pub fn text_content(&self) -> Option<&str> {
        match self {
            LlmResponse::Text { content } => Some(content),
            LlmResponse::ToolCalls { text, .. } => text.as_deref(),
        }
    }

    pub fn tool_calls(&self) -> &[ToolCall] {
        match self {
            LlmResponse::ToolCalls { calls, .. } => calls,
            LlmResponse::Text { .. } => &[],
        }
    }
}

/// A message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role")]
#[serde(rename_all = "lowercase")]
pub enum Message {
    #[serde(rename = "system")]
    System {
        content: String,
    },
    #[serde(rename = "user")]
    User {
        content: String,
    },
    #[serde(rename = "assistant")]
    Assistant {
        content: String,
    },
    #[serde(rename = "assistant_tool_calls")]
    AssistantToolCalls {
        #[serde(rename = "toolCalls")]
        tool_calls: Vec<ToolCall>,
        content: Option<String>,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        result: String,
        success: bool,
    },
}
