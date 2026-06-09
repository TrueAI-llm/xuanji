# Xuanji（璇玑）设计文档

> **状态**：已审核
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

MVP 阶段面向个人开发者，架构上为后续团队/企业扩展留空间。

### 设计原则

- **Workspace 拆分**：core / llm / agent / plugin / runner / trigger / memory / cli 各司其职，边界清晰
- **插件语言无关**：通过 JSON-RPC over stdio 通信，任何语言都能写插件
- **异步优先**：基于 tokio，DAG 中无依赖的节点并行执行
- **LLM 驱动**：LLM 是系统大脑，不是外挂

---

## 二、项目结构

```
xuanji/
├── crates/
│   ├── xuanji-core/       # 核心引擎：DAG 解析、调度、状态管理
│   ├── xuanji-llm/        # LLM 抽象层：多 Provider 适配、Tool Use 协议
│   ├── xuanji-agent/      # Agent 循环：推理 → 工具调用 → 观察 → 再推理
│   ├── xuanji-plugin/     # 插件协议定义 + IPC 通信层
│   ├── xuanji-runner/     # 执行器：进程管理、超时、重试
│   ├── xuanji-trigger/    # 触发器：CLI / 文件监听 / 定时 / HTTP
│   ├── xuanji-memory/     # 上下文与记忆：会话管理、长期记忆存储
│   └── xuanji-cli/        # CLI 入口（二进制）
├── plugins/               # 官方内置插件（如 shell、http、fs）
├── examples/              # 示例工作流 + 示例 Agent 对话
├── docs/                  # 文档
│   └── superpowers/specs/ # 设计文档
└── Cargo.toml             # workspace
```

---

## 三、LLM 抽象层（xuanji-llm）

### 多 Provider 架构

xuanji 不关心 Provider "是谁"，只关心 "怎么调用"（base_url + API 格式）。Provider 类型本质上是 API 协议格式的选择，而不是厂商锁定。

| 协议类型 | 适用范围 |
|---------|---------|
| `openai` | OpenAI、DeepSeek、通义千问、Ollama、任何 OpenAI 兼容服务 |
| `anthropic` | Claude 官方或私有部署的 Anthropic 兼容服务 |
| `gemini` | Google Gemini 或兼容服务 |

### Provider 配置

配置文件：`xuanji.toml`

```toml
[llm]
default = "deepseek"

[llm.providers.deepseek]
protocol = "openai"
base_url = "https://api.deepseek.com/v1"
api_key = "${DEEPSEEK_API_KEY}"
model = "deepseek-chat"

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
```

关键设计：
- 每个 Provider 显式声明 `protocol`，决定 API 调用格式
- 所有 Provider 统一支持 `base_url`，灵活对接代理、私有部署、本地服务
- `api_key` 为可选字段，本地模型可能不需要
- 支持环境变量引用（`${VAR_NAME}`）

### 核心数据结构

```rust
pub enum Protocol {
    OpenAI,
    Anthropic,
    Gemini,
}

pub struct ProviderConfig {
    pub name: String,
    pub protocol: Protocol,
    pub base_url: String,
    pub api_key: Option<String>,
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

## 四、Agent 循环（xuanji-agent）

### 执行流程

用户自然语言输入 → Agent 意图理解与任务拆解 → ReAct 循环（Reason → Act → Observe）→ 输出总结。

### Agent 驱动引擎

每一轮循环，引擎做四件事：构造请求 → 解析响应 → 执行分支 → 追加历史。

#### 构造请求

```rust
struct AgentRequest {
    system_prompt: String,     // 系统提示词：角色定义 + 规则
    tools: Vec<ToolSchema>,    // 可用工具清单（从插件自动生成）
    messages: Vec<Message>,    // 对话历史
}
```

System Prompt 定义 Agent 的行为规则，包含：
- 角色定义与工作方式
- 可用工具描述
- 执行规则（只做必要操作、失败重试、高风险确认、信息不足时提问）
- 输出格式指引（工具调用 vs 纯文本回复）

#### 解析响应

LLM 的响应只有两种可能：

```rust
enum AgentResponse {
    ToolCall {
        tool_name: String,
        arguments: serde_json::Value,
        reasoning: Option<String>,
    },
    Text {
        content: String,
        awaiting_input: bool,
    },
}
```

#### 执行分支

- **ToolCall 路径**：风险检查 → 低风险直接执行 / 高风险用户确认后执行 → 执行插件（IPC）→ 获取结果 → 追加到 messages → 进入下一轮
- **Text(awaiting_input=false)**：循环结束，输出总结
- **Text(awaiting_input=true)**：展示给用户，等待输入，追加后继续循环

#### History 管理

```rust
enum Message {
    User { content: String },
    AssistantToolCall { tool_name: String, arguments: Value },
    ToolResult { tool_name: String, result: String, success: bool },
    AssistantText { content: String },
}
```

上下文压缩策略：
- **MVP**：滑动窗口，保留最近 N 轮
- **后续**：摘要压缩，用 LLM 将历史摘要为精简描述

### 安全策略

```toml
[agent]
max_loops = 20
step_timeout = "60s"
confirm_risky = true
risky_patterns = [
    "rm -rf",
    "drop ",
    "deploy",
    "shutdown",
]
```

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

## 五、插件系统（xuanji-plugin）

### 通信协议

插件是独立进程，通过 JSON-RPC over stdio 与 xuanji 通信，语言无关，安全隔离。

### 插件必须实现的 JSON-RPC 方法

| 方法 | 说明 |
|------|------|
| `initialize` | 插件启动时调用，返回 `{ name, version, tools: [ToolSchema] }` |
| `execute` | Agent 循环中调用工具，传入 `{ tool, arguments }`，返回 `{ success, output, error }` |
| `shutdown` | 插件关闭前调用，做清理 |

### 一次工具调用流程

1. Agent 决定调用某工具
2. xuanji-plugin 查找对应插件进程（未启动则懒启动）
3. 通过 stdin 发送 JSON-RPC 请求
4. 插件进程执行，通过 stdout 返回结果
5. 结果返回给 Agent 循环

### 插件注册

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

### 官方内置插件（MVP）

| 插件名 | 提供的工具 |
|--------|-----------|
| **shell** | `shell.run` |
| **http** | `http.get`, `http.post`, `http.put`, `http.delete` |
| **fs** | `fs.read`, `fs.write`, `fs.list`, `fs.copy`, `fs.move`, `fs.delete` |

### 插件进程管理

| 策略 | 说明 |
|------|------|
| 懒启动 | 注册时不启动，首次 tool_call 时才启动 |
| 进程复用 | 启动后保持运行，后续调用复用同一进程 |
| 空闲回收 | 超过 N 分钟未使用自动关闭 |
| 优雅退出 | xuanji 退出时调用 shutdown，超时后强杀 |

### 插件开发

插件只需是遵守 JSON-RPC 协议的可执行文件，任何语言都可以编写。

---

## 六、工作流引擎（xuanji-core）

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
  skip_tests:
    type: boolean
    default: false

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

- 解析 YAML → 构建 DAG → 拓扑排序校验（无环）
- 取出所有就绪节点（依赖已全部完成）并行执行
- 收集结果，更新 DAG 状态
- 重复直到所有节点完成

### 模板变量

| 变量 | 来源 |
|------|------|
| `${{ inputs.xxx }}` | 用户传入的参数 |
| `${{ tasks.xxx.output }}` | 上游任务的输出结果 |
| `${{ tasks.xxx.exit_code }}` | 上游任务的退出码 |
| `${{ env.xxx }}` | 环境变量 |
| `${{ trigger.xxx }}` | 触发器提供的上下文 |

### LLM 智能节点

工作流中的 `llm.ask` 工具，复用 xuanji-llm 抽象层：

```yaml
tasks:
  analyze-log:
    tool: llm.ask
    arguments:
      prompt: "分析以下日志中的异常: ${{ tasks.fetch-log.output }}"
      provider: "deepseek"
      model: "deepseek-chat"
      temperature: 0.3
      max_tokens: 2000
```

### LLM 生成工作流

内置工具 `workflow.generate`，Agent 可根据用户自然语言描述生成 YAML 工作流：

- System Prompt 中注入完整的工作流 YAML Schema
- LLM 生成后进行 Schema 校验
- 校验失败则将错误信息注入 Agent 上下文，LLM 自我修正（最多重试 3 次）
- 校验通过后展示给用户，支持：保存 / 编辑 / 运行 / 丢弃

Agent 模式中也可通过 `workflow.run` 执行已保存的工作流，实现两种模式的双向打通。

---

## 七、触发器系统（xuanji-trigger）

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
| 文件监听 | `notify` crate | 监听文件系统事件，支持防抖 |
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

## 八、记忆系统（xuanji-memory）

### 三层记忆架构

| 层级 | 生命周期 | 内容 |
|------|----------|------|
| 短期记忆 | 会话内 | 对话历史、工具调用记录、LLM 推理过程 |
| 工作记忆 | 任务执行期 | 子任务进度、关键结果、错误记录 |
| 长期记忆 | 跨会话 | 用户偏好、项目知识、历史摘要 |

### 短期记忆

即 Agent 循环中的 `messages: Vec<Message>`，会话结束即消失。核心问题是上下文窗口管理——超限时进行压缩（MVP：滑动窗口；后续：摘要压缩）。

始终保留：系统提示词 + 用户原始目标 + 关键结果。

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

## 九、CLI 命令一览

```bash
# Agent 模式
xuanji "自然语言任务描述"
xuanji chat

# Workflow 模式
xuanji run <workflow.yaml>
xuanji run --input key=value
xuanji workflow create "描述"
xuanji workflow list
xuanji workflow edit <name>

# Daemon 模式
xuanji daemon start
xuanji daemon status
xuanji daemon stop

# 插件管理
xuanji plugin list
xuanji plugin info <name>

# 配置
xuanji config init
xuanji config show
xuanji config set <key> <value>

# 记忆
xuanji memory show
xuanji memory clear
xuanji memory rule add "规则描述"
```

---

## 十、MVP 分期计划

| 阶段 | 内容 | 交付物 |
|------|------|--------|
| **P0** | xuanji-llm + xuanji-agent + xuanji-plugin + xuanji-cli | Agent 模式跑通：自然语言 → 工具调用 → 完成 |
| **P1** | xuanji-core (DAG) + workflow.generate | Workflow 模式跑通：YAML 定义 + LLM 生成工作流 |
| **P2** | xuanji-trigger + daemon | 触发器跑通：文件监听 / 定时 / Webhook 自动执行 |
| **P3** | xuanji-memory (长期) | 跨会话记忆：偏好 / 项目知识 / 历史摘要 |

每个阶段独立可用，P0 完成后即是一个能用的 AI Agent CLI。

---

## 十一、技术栈

| 组件 | 选型 |
|------|------|
| 语言 | Rust |
| 异步运行时 | tokio |
| HTTP 客户端（LLM 调用） | reqwest |
| CLI 框架 | clap |
| 配置文件 | TOML (toml crate) |
| YAML 解析 | serde_yaml |
| DAG 调度 | petgraph |
| 文件监听 | notify |
| Cron 调度 | tokio-cron-scheduler |
| HTTP 服务器（Webhook） | axum |
| JSON Schema | schemars |
| 序列化 | serde + serde_json |
| 日志 | tracing + tracing-subscriber |
