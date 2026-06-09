# Xuanji（璇玑）设计文档

> **状态**：已审核（v2）
> **日期**：2026-06-09
> **作者**：gaochao + Claude

---

## 一、项目概览

**xuanji（璇玑）** 是一个用 Rust 编写的 AI 驱动的通用自动化平台 CLI 工具。

**命名由来**：璇玑是中国古代天文观测仪器（浑天仪的核心部件），用来模拟和追踪天体运转。象征着精密编排、协调万象——xuanji 帮助用户管理和编排复杂的自动化系统。

### 核心理念

LLM 是核心引擎——既能作为 AI Agent 自主理解意图、拆解任务、编排工作流、调用工具，也能在用户定义的 YAML 工作流中作为一个智能节点被调用。所有插件同时暴露给 LLM 作为 Tool Use 工具。

### 双模式运行

| 模式 | 交互方式 | 说明 |
|------|----------|------|
| **Agent 模式** | `xuanji "帮我部署项目到测试环境"` | 用户用自然语言描述目标，LLM 自主规划、拆解、执行 |
| **Workflow 模式** | `xuanji run deploy.yaml` | 用户定义 YAML 工作流，引擎按 DAG 执行，LLM 节点可在其中被调用 |

两种模式可互相转换：Agent 可生成 YAML 工作流供复用，YAML 工作流中可调用 LLM 智能节点。

### 目标用户

MVP 阶段面向个人开发者，架构上为后续多 Agent 协同和团队/企业扩展预留空间。

### 设计原则

- **Workspace 拆分**：各 crate 各司其职，边界清晰
- **插件语言无关**：通过 JSON-RPC over stdio 通信，任何语言都能写插件
- **异步优先**：基于 tokio，DAG 中无依赖的节点并行执行
- **LLM 驱动**：LLM 是系统大脑，不是外挂
- **架构预留**：多 Agent 协同、预算控制、共享状态在架构层面预留接口，MVP 不实现但不留技术债

---

## 二、项目结构

```
xuanji/
├── crates/
│   ├── xuanji-core/       # 核心引擎：DAG 解析、调度、状态管理、系统工具
│   ├── xuanji-llm/        # LLM 抽象层：多 Provider 适配、Tool Use 协议
│   ├── xuanji-agent/      # Agent 循环：推理 → 工具调用 → 观察 → 再推理
│   ├── xuanji-plugin/     # 插件协议定义 + IPC 通信层 + 进程管理
│   ├── xuanji-trigger/    # 触发器：CLI / 文件监听 / 定时 / HTTP
│   ├── xuanji-memory/     # 上下文与记忆：会话管理、长期记忆存储
│   ├── xuanji-bus/        # [预留] 知识总线：多 Agent 间知识共享与状态同步
│   ├── xuanji-budget/     # [预留] 预算控制：Token 计量、配额管理、熔断
│   └── xuanji-cli/        # CLI 入口（二进制）
├── plugins/               # 官方内置插件（如 shell、http、fs）
├── examples/              # 示例工作流 + 示例 Agent 对话
├── docs/                  # 文档
│   └── superpowers/specs/ # 设计文档
└── Cargo.toml             # workspace
```

**说明**：
- `xuanji-plugin` 包含进程管理（启动、超时、重试、回收），不单独拆 `xuanji-runner`
- `xuanji-bus` 和 `xuanji-budget` 为预留 crate，MVP 阶段仅定义 trait 接口，不实现功能

---

## 三、配置系统

### 配置文件位置与优先级

| 位置 | 优先级 | 说明 |
|------|--------|------|
| `./xuanji.toml` | 最高 | 项目级配置，覆盖全局 |
| `~/.xuanji/config.toml` | 中 | 用户全局配置 |
| CLI flags (`--provider`, `--model`) | 最高（运行时） | 单次运行覆盖 |

配置加载顺序：全局 → 项目级合并 → CLI flags 覆盖。

### 环境变量插值

所有字符串值支持 `${VAR_NAME}` 语法引用环境变量，启动时解析。未定义的环境变量视为空字符串。

### 完整配置 Schema

```toml
# xuanji.toml - 完整配置示例

# ---- LLM 配置 ----
[llm]
default = "deepseek"

[llm.providers.deepseek]
protocol = "openai"
base_url = "https://api.deepseek.com/v1"
api_key = "${DEEPSEEK_API_KEY}"
model = "deepseek-chat"
timeout = "120s"
max_tokens = 4096
temperature = 0.7

[llm.providers.claude]
protocol = "anthropic"
base_url = "https://api.anthropic.com"
api_key = "${ANTHROPIC_API_KEY}"
model = "claude-sonnet-4-6"

[llm.providers.local]
protocol = "openai"
base_url = "http://localhost:11434/v1"
model = "qwen3:32b"
# api_key 可选，本地模型不需要

# ---- Agent 配置 ----
[agent]
max_loops = 20
step_timeout = "60s"
confirm_risky = true

# 高风险操作匹配规则
# 匹配目标：工具名 + 参数的拼接字符串
# 匹配方式：大小写敏感的子串匹配
# 使用 regex 语法
[[agent.risky_patterns]]
tool = "shell.run"
pattern = "rm\\s+-rf"
[[agent.risky_patterns]]
tool = "shell.run"
pattern = "(?:drop|DELETE)\\s+"

# ---- 插件注册 ----
[[plugin]]
name = "shell"
exec = "xuanji-plugin-shell"

[[plugin]]
name = "http"
exec = "xuanji-plugin-http"

[[plugin]]
name = "fs"
exec = "xuanji-plugin-filesystem"

# ---- 触发器配置 ----
[trigger]
webhook_port = 8080
webhook_secret = "${WEBHOOK_SECRET}"

# ---- 记忆配置 ----
[memory]
max_history = 100           # 最大历史记录条数
max_context_turns = 20      # 短期记忆保留轮数
```

配置在启动时校验，格式错误直接报错退出。

---

## 四、LLM 抽象层（xuanji-llm）

### 多 Provider 架构

xuanji 不关心 Provider "是谁"，只关心 "怎么调用"（base_url + API 格式）。Provider 类型本质上是 API 协议格式的选择，而不是厂商锁定。

| 协议类型 | 适用范围 |
|---------|---------|
| `openai` | OpenAI、DeepSeek、通义千问、Ollama、任何 OpenAI 兼容服务 |
| `anthropic` | Claude 官方或私有部署的 Anthropic 兼容服务 |
| `gemini` | Google Gemini 或兼容服务 |

### 核心数据结构

```rust
pub enum Protocol {
    OpenAI,
    Anthropic,
    Gemini,
}

pub struct ProviderConfig {
    pub name: String,
    pub protocol: Protocol,       // 必填，决定 API 调用格式
    pub base_url: String,         // 必填，所有 Provider 统一支持
    pub api_key: Option<String>,  // 可选，本地模型可能不需要
    pub model: String,
    pub timeout: Option<Duration>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}
```

### 关键决策

- 优先实现 OpenAI Compatible + Anthropic 两个适配器（覆盖最广）
- Ollama 复用 OpenAI Compatible（Ollama 已兼容 OpenAI API 格式）
- Gemini 后续迭代加入
- 工作流中可以指定不同节点用不同 Provider/模型
- 所有 Provider 统一支持 SSE 流式响应

---

## 五、工具系统

xuanji 的工具分为两类：**插件工具**和**系统工具**。所有工具在 Agent 和 Workflow 模式中统一使用，但对 LLM 而言无区别——都是 Tool Schema。

### 5.1 插件工具（xuanji-plugin）

插件是独立进程，通过 JSON-RPC 2.0 over stdio 与 xuanji 通信，语言无关，安全隔离。

#### JSON-RPC 协议

协议版本：JSON-RPC 2.0。每行一条消息，以 `\n` 分隔。

**initialize 请求**：
```json
{"jsonrpc": "2.0", "method": "initialize", "params": {}, "id": 1}
```

**initialize 响应**：
```json
{
  "jsonrpc": "2.0",
  "result": {
    "name": "shell",
    "version": "0.1.0",
    "protocol_version": 1,
    "tools": [
      {
        "name": "shell.run",
        "description": "在 shell 中执行命令",
        "parameters": {
          "type": "object",
          "properties": {
            "command": {"type": "string", "description": "要执行的命令"},
            "timeout": {"type": "integer", "description": "超时秒数"}
          },
          "required": ["command"]
        }
      }
    ]
  },
  "id": 1
}
```

**execute 请求**：
```json
{
  "jsonrpc": "2.0",
  "method": "execute",
  "params": {"tool": "run", "arguments": {"command": "ls -la"}},
  "id": 2
}
```

**execute 响应**：
```json
{
  "jsonrpc": "2.0",
  "result": {
    "success": true,
    "output": "total 32\ndrwxr-xr-x ...",
    "error": null
  },
  "id": 2
}
```

**shutdown 请求**：
```json
{"jsonrpc": "2.0", "method": "shutdown", "params": {}, "id": 3}
```

#### 并发与进程模型

- 每个插件进程同时只处理一个请求（请求-响应串行）
- 当 DAG 并行执行多个同插件任务时，自动启动多个插件进程实例
- 插件通过 stderr 输出日志，xuanji 捕获并转发到 tracing 日志系统

#### 输出大小限制

- 单次 execute 响应的 `output` 字段最大 1MB（可配置）
- 超限时插件应截断并在 `error` 中说明
- 插件可在 `initialize` 时声明 `max_output_size` 覆盖默认值

#### 插件进程管理

| 策略 | 说明 |
|------|------|
| 懒启动 | 注册时不启动，首次 tool_call 时才启动 |
| 进程复用 | 单请求串行复用同一进程 |
| 并发扩展 | 并行请求自动启动多进程实例 |
| 空闲回收 | 超过 N 分钟未使用自动关闭（默认 10 分钟） |
| 优雅退出 | xuanji 退出时调用 shutdown，5s 超时后强杀 |

#### 插件注册

```toml
[[plugin]]
name = "shell"
exec = "xuanji-plugin-shell"

[[plugin]]
name = "http"
exec = "xuanji-plugin-http"

[[plugin]]
name = "fs"
exec = "xuanji-plugin-filesystem"
```

#### 官方内置插件（MVP）

| 插件名 | 提供的工具 |
|--------|-----------|
| **shell** | `shell.run` |
| **http** | `http.get`, `http.post`, `http.put`, `http.delete` |
| **fs** | `fs.read`, `fs.write`, `fs.list`, `fs.copy`, `fs.move`, `fs.delete` |

#### 插件开发

插件只需是遵守 JSON-RPC 协议的可执行文件，任何语言都可以编写。从 stdin 按行读取请求，向 stdout 按行写入响应，日志输出到 stderr。

### 5.2 系统工具

系统工具由 xuanji-core 直接实现，不经过插件 IPC 层，但同样注册为 Tool Schema 暴露给 LLM。

| 系统工具 | 实现位置 | 说明 |
|---------|---------|------|
| `llm.ask` | xuanji-llm | 在 Workflow/Agent 中调用 LLM 进行推理 |
| `workflow.generate` | xuanji-core | 根据自然语言描述生成 YAML 工作流 |
| `workflow.run` | xuanji-core | 执行已保存的工作流 |
| `memory.recall` | xuanji-memory | 查询长期记忆 |
| `memory.store` | xuanji-memory | 写入长期记忆 |

系统工具的调用路径：Agent/Workflow → 直接调用对应 crate API → 返回结果。不经过 JSON-RPC IPC。

### 5.3 工具调用结果映射

插件 `execute` 返回 `{ success, output, error }` 需要映射为 Agent 的 `ToolResult`：

```rust
impl From<PluginResult> for ToolResult {
    fn from(result: PluginResult) -> Self {
        if result.success {
            ToolResult {
                tool_name: /* caller sets */,
                result: result.output.unwrap_or_default(),
                success: true,
            }
        } else {
            // 失败时合并 error 和 output（如果有部分输出）
            let msg = match (result.error, result.output) {
                (Some(e), Some(o)) => format!("Error: {e}\nPartial output: {o}"),
                (Some(e), None) => e,
                (None, Some(o)) => format!("Failed with output: {o}"),
                (None, None) => "Unknown error".into(),
            };
            ToolResult {
                tool_name: /* caller sets */,
                result: msg,
                success: false,
            }
        }
    }
}
```

---

## 六、Agent 循环（xuanji-agent）

### 执行流程

用户自然语言输入 → Agent 意图理解与任务拆解 → ReAct 循环（Reason → Act → Observe）→ 输出总结。

### Agent 驱动引擎

每一轮循环，引擎做四件事：构造请求 → 解析响应 → 执行分支 → 追加历史。

#### 构造请求

```rust
struct AgentRequest {
    system_prompt: String,     // 系统提示词：角色定义 + 规则
    tools: Vec<ToolSchema>,    // 可用工具清单（插件工具 + 系统工具统一注册）
    messages: Vec<Message>,    // 对话历史
}
```

System Prompt 定义 Agent 的行为规则，包含：
- 角色定义与工作方式
- 可用工具描述
- 执行规则（只做必要操作、失败重试、高风险确认、信息不足时提问）
- 输出格式指引（工具调用 vs 纯文本回复）

#### 解析响应

LLM 在单次响应中可能返回**多个并行工具调用**（OpenAI/Anthropic/Gemini 均支持）：

```rust
enum AgentResponse {
    /// LLM 想调用一个或多个工具（无依赖时可并行执行）
    ToolCalls {
        calls: Vec<ToolCall>,
    },
    /// LLM 认为任务完成或需要用户输入
    Text {
        content: String,
        awaiting_input: bool,
    },
}

struct ToolCall {
    tool_name: String,
    arguments: serde_json::Value,
    reasoning: Option<String>,
}
```

多个 ToolCalls 之间没有依赖关系时并行执行，所有结果收集完毕后一起追加到 messages，进入下一轮。

#### 执行分支

- **ToolCalls 路径**：
  1. 对每个 ToolCall 进行风险检查
  2. 低风险直接执行 / 高风险用户确认后执行
  3. 所有 ToolCall 并行执行（系统工具直接调用，插件工具走 IPC）
  4. 收集所有结果，追加到 messages
  5. 进入下一轮
- **Text(awaiting_input=false)**：循环结束，输出总结
- **Text(awaiting_input=true)**：展示给用户，等待输入，追加后继续循环

#### History 管理

```rust
enum Message {
    User { content: String },
    AssistantToolCalls { calls: Vec<ToolCall> },
    ToolResults { results: Vec<ToolResult> },
    AssistantText { content: String },
}

struct ToolResult {
    tool_name: String,
    result: String,
    success: bool,
}
```

上下文压缩策略：
- **MVP**：滑动窗口，保留最近 N 轮，始终保留系统提示词 + 用户原始目标 + 关键结果
- **后续**：摘要压缩，用 LLM 将历史摘要为精简描述

### 安全策略

```toml
[agent]
max_loops = 20
step_timeout = "60s"
confirm_risky = true

# 风险模式按 (工具名, 正则) 匹配
# 匹配目标：工具名匹配 tool 字段，参数 JSON 序列化后匹配 pattern
# 大小写敏感
[[agent.risky_patterns]]
tool = "shell.run"
pattern = "rm\\s+-rf"
```

### 交互模式

- `xuanji "任务描述"`：单次 Agent 执行，任务完成后退出
- `xuanji chat`：交互式多轮 Agent 对话，支持连续追问和上下文保持，用户输入 `exit` 或 Ctrl-C 退出

### 工作记忆

任务执行期间维护状态，每轮注入 system prompt：

```rust
struct WorkingMemory {
    goal: String,
    subtasks: Vec<SubTask>,
    key_results: Vec<String>,
    errors: Vec<ErrorRecord>,
}
```

让 LLM 知道"当前做到哪了"——已完成子任务、当前步骤、待执行步骤。

---

## 七、工作流引擎（xuanji-core）

### YAML 工作流定义

```yaml
name: deploy-to-test
description: 构建并部署到测试环境

triggers:
  - type: file-watcher
    paths: ["src/**", "Cargo.toml"]
    events: [modified]
  - type: cron
    schedule: "0 9 * * 1-5"
  - type: webhook
    path: "/deploy"

inputs:
  environment:
    type: string
    default: "test"

tasks:
  run-tests:
    tool: shell.run
    arguments:
      command: "cargo test"
    timeout: 120s
    retry:
      max_attempts: 2
      delay: 5s

  lint:
    tool: shell.run
    arguments:
      command: "cargo clippy"
    timeout: 60s

  build:
    tool: shell.run
    arguments:
      command: "cargo build --release"
    depends_on: [run-tests, lint]
    timeout: 300s

  generate-changelog:
    tool: llm.ask
    arguments:
      prompt: "分析最近的 git log，生成中文 changelog"
      provider: "deepseek"
    depends_on: [build]

  deploy:
    tool: shell.run
    arguments:
      command: "scp target/release/app ${{ inputs.environment }}-server:/opt/app/"
    depends_on: [build, generate-changelog]
    confirm: true

  notify:
    tool: http.post
    arguments:
      url: "https://hooks.slack.com/services/xxx"
      body:
        text: "✅ ${{ inputs.environment }} 部署完成"
    depends_on: [deploy]
```

### DAG 调度

1. 解析 YAML → 构建 DAG → 拓扑排序校验（无环）
2. 取出所有就绪节点（依赖已全部完成）并行执行
3. 收集结果，更新 DAG 状态
4. 重复直到所有节点完成

### 条件执行

任务支持 `when` 字段，值为模板表达式，求值结果为布尔值：

```yaml
tasks:
  run-tests:
    tool: shell.run
    arguments:
      command: "cargo test"
    when: "${{ inputs.skip_tests }} != true"
```

`when` 求值为 `false` 时，该任务标记为 `Skipped`，下游依赖视为已满足。

### 失败处理策略

**任务级失败**：
- `retry` 定义重试策略（max_attempts + delay）
- 重试耗尽后任务标记为 `Failed`

**DAG 级失败传播**：
- 失败任务的所有下游任务标记为 `Blocked`（跳过）
- 无依赖关系的兄弟任务继续执行
- 当没有任何就绪任务时，工作流结束
- 最终状态包含每个任务的详细结果（Done / Failed / Blocked / Skipped）

**超时处理**：
- 任务执行超过 `timeout` 后，强制终止插件进程，任务标记为 `Failed`
- 超时视为重试的一次失败尝试

**confirm 在 daemon 模式下的行为**：
- 带 `confirm: true` 的工作流注册 daemon 触发器时，校验报错，不允许注册
- 或通过配置 `confirm_auto_approve = true` 在 daemon 模式下自动确认（需显式开启）

### 模板变量

| 变量 | 来源 |
|------|------|
| `${{ inputs.xxx }}` | 用户传入的参数 |
| `${{ tasks.xxx.output }}` | 上游任务的输出结果 |
| `${{ tasks.xxx.exit_code }}` | 上游任务的退出码 |
| `${{ tasks.xxx.status }}` | 上游任务的状态（done/failed/blocked/skipped） |
| `${{ env.xxx }}` | 环境变量 |
| `${{ trigger.xxx }}` | 触发器提供的上下文 |

### 触发器上下文 Schema

每种触发器提供的 `trigger` 变量结构：

**文件监听**：
```json
{ "path": "src/main.rs", "event": "modified" }
```

**Cron 定时**：
```json
{ "scheduled_time": "2026-06-09T09:00:00+08:00" }
```

**HTTP/Webhook**：
```json
{ "method": "POST", "path": "/deploy", "headers": {...}, "body": {...} }
```

### LLM 智能节点

工作流中的 `llm.ask` 是系统工具，由 xuanji-llm 直接处理：

```yaml
tasks:
  analyze-log:
    tool: llm.ask
    arguments:
      prompt: "分析以下日志中的异常: ${{ tasks.fetch-log.output }}"
      provider: "deepseek"       # 可选，不填用默认 provider
      model: "deepseek-chat"     # 可选，覆盖 provider 默认模型
      temperature: 0.3           # 可选
      max_tokens: 2000           # 可选
```

### LLM 生成工作流

系统工具 `workflow.generate`，Agent 可根据用户自然语言描述生成 YAML 工作流：

- System Prompt 中注入完整的工作流 YAML Schema + 已注册工具列表
- LLM 生成后进行 Schema 校验
- 校验失败则将错误信息注入 Agent 上下文，LLM 自我修正（最多重试 3 次）
- 校验通过后展示给用户，支持：保存 / 编辑 / 运行 / 丢弃

Agent 模式中也可通过 `workflow.run` 执行已保存的工作流，实现两种模式的双向打通。

---

## 八、触发器系统（xuanji-trigger）

### 统一接口

```rust
#[async_trait]
trait Trigger: Send + Sync {
    fn trigger_type(&self) -> &str;
    async fn start(&self, sender: TriggerSender) -> Result<()>;
    async fn stop(&self) -> Result<()>;
}

struct TriggerEvent {
    trigger_type: String,
    workflow_name: String,
    payload: serde_json::Value,
}
```

### 四种触发器

| 触发器 | 实现 | 说明 |
|--------|------|------|
| CLI 手动 | `xuanji run <yaml>` | 最基础，不需要后台进程 |
| 文件监听 | `notify` crate | 监听文件系统事件，支持防抖（默认 500ms） |
| Cron 定时 | `tokio-cron-scheduler` | 标准 cron 表达式，支持时区 |
| HTTP/Webhook | `axum` 内嵌 HTTP 服务器 | 接收外部请求触发，支持签名验证 |

### 守护进程模式

文件监听、定时、Webhook 需要 xuanji 保持后台运行：

```bash
xuanji daemon start    # 启动守护进程
xuanji daemon status   # 查看状态
xuanji daemon stop     # 停止
```

进程管理：
- PID 文件 + 单实例保证（`~/.xuanji/daemon.pid`）
- CLI 与守护进程通过 Unix socket 通信（`~/.xuanji/daemon.sock`）
- 各触发器跑在独立的 tokio task 中

---

## 九、记忆系统（xuanji-memory）

### 三层记忆架构

| 层级 | 生命周期 | 内容 |
|------|----------|------|
| 短期记忆 | 会话内 | 对话历史、工具调用记录、LLM 推理过程 |
| 工作记忆 | 任务执行期 | 子任务进度、关键结果、错误记录 |
| 长期记忆 | 跨会话 | 用户偏好、项目知识、历史摘要 |

### 短期记忆

即 Agent 循环中的 `messages: Vec<Message>`，会话结束即消失。

压缩策略：
- **MVP**：滑动窗口，保留最近 N 轮（默认 20），始终保留系统提示词 + 用户原始目标 + 关键结果
- **后续**：摘要压缩，用 LLM 将早期轮次压缩为摘要

### 长期记忆存储

```
~/.xuanji/memory/
├── preferences.json         # 用户偏好
├── projects/
│   └── <project>/
│       ├── context.json     # 项目知识（技术栈、目录结构等）
│       └── history.jsonl    # 历史执行记录摘要
└── global/
    └── patterns.json        # 用户常用操作模式
```

### 记忆注入

每次新会话启动时，从长期记忆中检索相关内容，拼接到 system prompt：

1. 用户偏好（始终注入）
2. 当前项目知识（按工作目录匹配）
3. 最近历史（最近 5 条摘要）
4. 用户自定义规则

### 记忆更新时机

| 时机 | 更新内容 |
|------|---------|
| 任务完成后 | 追加历史记录摘要 |
| 项目首次使用时 | 自动分析目录结构，生成初始项目知识 |
| 用户主动告知 | 写入项目知识（如"记住用 pnpm"） |
| 偏好变更 | 用户修改配置或通过 CLI 设置 |

---

## 十、CLI 命令一览

```bash
# === Agent 模式 ===
xuanji "自然语言任务描述"              # 单次 Agent 执行
xuanji chat                           # 交互式多轮 Agent 对话（exit 退出）

# === Workflow 模式 ===
xuanji run <workflow.yaml>            # 手动运行工作流
xuanji run --input key=value         # 带参数运行
xuanji workflow create "描述"         # LLM 生成工作流 YAML
xuanji workflow list                  # 列出所有已保存的工作流
xuanji workflow edit <name>           # 编辑工作流

# === Daemon 模式 ===
xuanji daemon start                   # 启动守护进程（激活所有触发器）
xuanji daemon status                  # 查看守护进程状态
xuanji daemon stop                    # 停止守护进程

# === 插件管理 ===
xuanji plugin list                    # 列出已注册插件
xuanji plugin info <name>             # 查看插件详情

# === 配置 ===
xuanji config init                    # 初始化配置文件
xuanji config show                    # 查看当前配置
xuanji config set <key> <value>       # 修改配置

# === 记忆 ===
xuanji memory show                    # 查看当前项目记忆
xuanji memory clear                   # 清除项目记忆
xuanji memory rule add "规则描述"      # 添加自定义规则
```

---

## 十一、多 Agent 协同架构预留

> 本节定义面向未来的多 Agent 协同能力。MVP 不实现，但架构层面预留接口，确保后续扩展无技术债。

### 面向的五大核心瓶颈

1. **代理间缺乏知识共享与横向协同** → Knowledge Bus
2. **物理隔离下的状态盲区与覆盖冲突** → Shared State Layer
3. **底层 CLI 接口的脆弱性** → 已通过插件 JSON-RPC 协议 + 版本化缓解
4. **递归发散与账单失控** → Budget Controller
5. **人类审查认知过载** → 渐进式审查

### 11.1 Knowledge Bus（xuanji-bus）

多 Agent 间的实时通信与知识共享中间件。

```
┌────────┐  ┌────────┐  ┌────────┐
│ Agent A │  │ Agent B │  │ Agent C │
└───┬────┘  └───┬────┘  └───┬────┘
    │           │           │
    ▼           ▼           ▼
┌──────────────────────────────────┐
│         Knowledge Bus            │
│                                  │
│  Channels:                       │
│  - discovery.* (发现与经验)       │
│  - warning.*   (警告与注意)       │
│  - state.*     (变更通知)         │
│  - insight.*   (理解与洞察)       │
└──────────────────────────────────┘
```

**预留接口**：

```rust
#[async_trait]
trait KnowledgeBus: Send + Sync {
    /// 发布知识到指定频道
    async fn publish(&self, channel: &str, message: KnowledgeMessage);

    /// 订阅频道，接收消息
    async fn subscribe(&self, channel: &str) -> mpsc::Receiver<KnowledgeMessage>;
}

struct KnowledgeMessage {
    source_agent: String,
    channel: String,
    payload: serde_json::Value,
    timestamp: DateTime<Utc>,
}
```

**解决瓶颈 1**：Agent A 发现测试环境配置规则后，发布到 `discovery.env-config`，Agent B/C/D 自动订阅并获取，避免重复试错和 Token 浪费。

### 11.2 Shared State Layer

多 Agent 并行修改时的状态感知与冲突预防机制。

**预留接口**：

```rust
#[async_trait]
trait SharedState: Send + Sync {
    /// 声明修改意图（乐观锁）
    async fn declare_intent(&self, agent: &str, scope: IntentScope) -> Result<IntentTicket>;

    /// 读取当前状态（含版本号）
    async fn read(&self, key: &str) -> StateEntry;

    /// 写入状态（compare-and-swap）
    async fn write(&self, ticket: &IntentTicket, key: &str, value: Value) -> Result<()>;
}

struct IntentScope {
    files: Vec<PathPattern>,     // 声明要修改的文件范围
    resources: Vec<String>,      // 声明要修改的逻辑资源
}

struct StateEntry {
    value: serde_json::Value,
    version: u64,                // 乐观锁版本号
    last_modified_by: String,    // 最后修改的 Agent
}
```

**解决瓶颈 2**：
- Agent 开始修改前声明意图，其他 Agent 可感知变更范围
- 写入时使用 compare-and-swap，版本冲突时拒绝并通知 Agent 协商
- 变更通过 Knowledge Bus 实时广播，避免盲区

### 11.3 Budget Controller（xuanji-budget）

Token 消耗与 API 配额的实时监控和熔断机制。

**预留接口**：

```rust
#[async_trait]
trait BudgetController: Send + Sync {
    /// 执行前预估 token 消耗
    async fn estimate(&self, request: &AgentRequest) -> TokenEstimate;

    /// 请求配额（返回是否允许执行）
    async fn acquire(&self, agent: &str, estimated_tokens: u32) -> Result<QuotaTicket>;

    /// 报告实际消耗
    async fn report(&self, ticket: &QuotaTicket, actual_tokens: u32);

    /// 查询当前配额状态
    async fn status(&self) -> BudgetStatus;
}

struct BudgetStatus {
    total_budget: u32,           // 总预算（token 数）
    total_consumed: u32,         // 已消耗
    per_agent: HashMap<String, u32>,  // 各 Agent 消耗
    remaining: u32,
}
```

**解决瓶颈 4**：
- 执行前预估 token 消耗，超预算自动熔断
- 递归派生深度硬限制（如 max_depth=3）
- 实时 token 计数，接近限额时降级或停止
- 支持 API 供应商的速率限制追踪

**MVP 中的简化实现**：P0 阶段实现基础的单 Agent 循环计数（loop count + timeout），不实现完整的多 Agent 预算控制。

### 11.4 渐进式审查

多 Agent 并行产出时，减轻人类审查负担的机制。

**预留设计**：
- **AI 自审**：Agent 完成任务后先生成变更摘要和风险评估
- **分级审查**：高风险变更（涉及公共模块、安全相关）必须人工审查，低风险可自动合并
- **上下文保持**：为人类评审者提供变更的完整推理链，而非只看 diff
- **增量审查**：变更按逻辑单元分组，而非按文件罗列

**MVP 中的简化实现**：P0 阶段 `confirm: true` + `risky_patterns` 提供基础的高风险操作确认机制。

---

## 十二、MVP 分期计划

| 阶段 | 内容 | 交付物 |
|------|------|--------|
| **P0** | xuanji-llm + xuanji-agent + xuanji-plugin + xuanji-cli | Agent 模式跑通：自然语言 → 工具调用 → 完成 |
| **P1** | xuanji-core (DAG) + workflow.generate | Workflow 模式跑通：YAML 定义 + LLM 生成工作流 |
| **P2** | xuanji-trigger + daemon | 触发器跑通：文件监听 / 定时 / Webhook 自动执行 |
| **P3** | xuanji-memory (长期) | 跨会话记忆：偏好 / 项目知识 / 历史摘要 |
| **P4** | xuanji-bus + xuanji-budget | 多 Agent 协同：知识共享、预算控制、冲突预防 |

每个阶段独立可用，P0 完成后即是一个能用的 AI Agent CLI。

---

## 十三、技术栈

| 组件 | 选型 |
|------|------|
| 语言 | Rust |
| 异步运行时 | tokio |
| HTTP 客户端（LLM 调用） | reqwest |
| CLI 框架 | clap |
| 配置文件 | TOML (toml crate) |
| YAML 解析 | serde_yml |
| DAG 调度 | petgraph |
| 文件监听 | notify |
| Cron 调度 | tokio-cron-scheduler |
| HTTP 服务器（Webhook） | axum |
| JSON Schema | schemars |
| 序列化 | serde + serde_json |
| 日志 | tracing + tracing-subscriber |
| 正则表达式 | regex |
