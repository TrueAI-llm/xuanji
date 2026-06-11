# xuanji (璇玑)

[![Rust](https://img.shields.io/badge/Rust-1.85+-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

AI 驱动的通用自动化平台 CLI 工具。

璇玑是中国古代天文观测仪器（浑天仪的核心部件），用来模拟和追踪天体运转。xuanji 用 AI Agent 模拟和自动化你的工作流程。

## 特性

- **Agent 模式** — ReAct 循环，支持工具调用、多轮对话、风险评估
- **工作流引擎** — YAML 定义 DAG 工作流，支持并行执行、依赖管理、模板变量
- **MCP 生态** — 基于 Model Context Protocol 的工具集成，可扩展
- **多 Agent 协作** — Swarm 模式，KnowledgeBus 知识共享、BudgetController 预算控制
- **三层记忆** — 短期（对话历史）、工作记忆（子任务追踪）、长期（项目知识持久化）
- **触发系统** — Cron 定时任务、文件监控、Webhook，守护进程模式运行
- **多提供商** — 支持 OpenAI、Anthropic 协议及所有兼容 API
- **内置 Shell** — `shell.run` 系统工具，无需外部 MCP 即可执行命令

## 快速开始

### 安装

```bash
git clone https://github.com/TrueAI-llm/xuanji.git
cd xuanji
cargo build --release
# 二进制文件在 target/release/xuanji
```

### 初始化

```bash
# 交互式向导（推荐）
xuanji init

# 或使用非交互式模板
xuanji config-init
```

### 使用

```bash
# 单次任务
xuanji "列出当前目录下的文件并分类"

# 交互式对话
xuanji chat

# 查看可用工具
xuanji mcp list
```

## 命令参考

| 命令 | 说明 |
|------|------|
| `xuanji "<prompt>"` | 单次 Agent 任务 |
| `xuanji chat` | 交互式多轮对话 |
| `xuanji init` | 交互式配置向导 |
| `xuanji config-init` | 生成默认配置模板 |
| `xuanji run <workflow.yaml>` | 执行 YAML 工作流 |
| `xuanji swarm "<task>" --workers N` | 多 Agent 协作模式 |
| `xuanji mcp list` | 列出所有 MCP 工具 |
| `xuanji mcp install <package>` | 安装 MCP 服务器 |
| `xuanji mcp add <name> --command <cmd>` | 手动添加 MCP 服务器 |
| `xuanji mcp remove <name>` | 移除 MCP 服务器 |
| `xuanji daemon start` | 启动守护进程 |
| `xuanji daemon status` | 查看守护进程状态 |
| `xuanji daemon stop` | 停止守护进程 |
| `xuanji memory show` | 查看项目记忆 |
| `xuanji memory clear` | 清除项目记忆 |
| `xuanji memory rule <text>` | 添加自定义规则 |
| `xuanji budget` | 查看预算配置 |

## 配置

xuanji 使用双层配置：全局 (`~/.xuanji/config.toml`) + 项目本地 (`./xuanji.toml`)，本地覆盖全局。

API Key 支持 `${ENV_VAR}` 格式，运行时从环境变量读取。

```toml
[llm]
default_provider = "deepseek"

[llm.providers.deepseek]
protocol = "openai"
model = "deepseek-chat"
api_key = "${DEEPSEEK_API_KEY}"
base_url = "https://api.deepseek.com/v1"

# 可以配置多个提供商
[llm.providers.openai]
protocol = "openai"
model = "gpt-4o"
api_key = "${OPENAI_API_KEY}"

[llm.providers.anthropic]
protocol = "anthropic"
model = "claude-sonnet-4-20250514"
api_key = "${ANTHROPIC_API_KEY}"

[agent]
max_loops = 20
step_timeout = "60s"
confirm_risky = true

[[mcp_server]]
name = "shell"
command = "xuanji-mcp-shell"

[[mcp_server]]
name = "playwright"
command = "npx"
args = ["-y", "@playwright/mcp"]

[trigger]
webhook_port = 9090
workflows_dir = "~/.xuanji/workflows"

[memory]
max_history = 100
max_context_turns = 20

[budget]
total_budget = 1000000
per_agent_budget = 200000
max_depth = 3
```

### 提供商配置

| 字段 | 必填 | 说明 |
|------|------|------|
| `protocol` | ✅ | `openai` 或 `anthropic` |
| `model` | ✅ | 模型标识符 |
| `api_key` | ✅ | API 密钥，支持 `${ENV_VAR}` |
| `base_url` | ❌ | 自定义 API 地址 |
| `max_tokens` | ❌ | 最大输出 token 数 |
| `temperature` | ❌ | 采样温度 |

### MCP 服务器安装

```bash
# npm 包（自动识别 @scope/name 格式）
xuanji mcp install @playwright/mcp

# Python 包
xuanji mcp install akshare-one-mcp

# 手动添加
xuanji mcp add my-server --command /path/to/binary

# 保存到全局配置
xuanji mcp install @playwright/mcp --global
```

## 工作流

使用 YAML 定义自动化工作流，支持 DAG 依赖、模板变量和多种触发器。

### 工作流示例

```yaml
# examples/scheduled-report.yaml
name: daily-report
description: 每个工作日早上 9 点生成项目报告

triggers:
  - type: cron
    schedule: "0 9 * * 1-5"

tasks:
  report:
    tool: llm.ask
    arguments:
      prompt: "生成今日项目状态报告"
```

```yaml
# examples/auto-build.yaml
name: auto-build
description: 源码变更时自动构建

triggers:
  - type: file-watcher
    paths: ["src/"]
    events: ["modified"]

tasks:
  build:
    tool: shell.run
    arguments:
      command: "cargo build 2>&1"
  test:
    tool: shell.run
    arguments:
      command: "cargo test 2>&1"
    depends_on: [build]
```

### 触发器类型

| 类型 | 说明 | 用途 |
|------|------|------|
| `cron` | 定时调度 | 定期执行报告、检查 |
| `file-watcher` | 文件变更监控 | 自动构建、测试 |
| `webhook` | HTTP 触发器 | CI/CD 集成、外部触发 |

### 模板变量

工作流中可以使用模板变量：

- `${{ inputs.X }}` — 工作流输入参数
- `${{ tasks.X.output }}` — 上游任务输出
- `${{ env.X }}` — 环境变量
- `${{ trigger.X }}` — 触发器上下文

更多示例见 [`examples/`](examples/) 目录。

## 多 Agent 协作

Swarm 模式支持多个 Agent 协同工作：

```bash
xuanji swarm "分析这个项目的代码质量并生成改进建议" --workers 3
```

架构组件：
- **KnowledgeBus** — Agent 间知识共享（发现、警告、状态、洞察）
- **BudgetController** — Token 预算控制，每 Agent 独立限制
- **SharedState** — 共享状态管理，带意图声明和冲突检测

## 架构

```
xuanji-cli
├── xuanji-agent ─── xuanji-llm        (LLM 提供商抽象)
│                 ├── xuanji-plugin     (MCP 工具注册表)
│                 ├── xuanji-memory     (三层记忆系统)
│                 ├── xuanji-bus        (Agent 间通信)
│                 └── xuanji-budget     (Token 预算控制)
├── xuanji-core ─── (DAG 工作流引擎、系统工具)
└── xuanji-trigger ─ (Cron、文件监控、Webhook)

plugins/
└── xuanji-mcp-shell  (内置 Shell MCP 服务器)
```

## 开发

```bash
# 构建
cargo build

# 运行测试
cargo test

# 运行
cargo run --bin xuanji -- --help
```

### 项目结构

```
crates/
├── xuanji-cli/        CLI 入口，命令分发
├── xuanji-core/       工作流引擎，系统工具
├── xuanji-llm/        LLM 提供商抽象
├── xuanji-agent/      Agent 循环，工具调用
├── xuanji-plugin/     MCP 客户端，工具注册
├── xuanji-memory/     三层记忆系统
├── xuanji-trigger/    触发器系统
├── xuanji-bus/        Agent 间通信
└── xuanji-budget/     预算控制
plugins/
└── xuanji-mcp-shell/  内置 Shell 工具
examples/              工作流 YAML 示例
```

## License

[MIT](LICENSE)
