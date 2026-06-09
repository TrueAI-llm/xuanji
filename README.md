# xuanji (璇玑)

AI 驱动的通用自动化平台 CLI 工具。

## 安装

```bash
cargo build --release
# 二进制文件在 target/release/xuanji
```

## 快速开始

```bash
# 初始化配置
xuanji config-init

# 设置 API Key
export DEEPSEEK_API_KEY=your-key-here

# 运行 Agent
xuanji "列出当前目录下的文件"

# 交互式对话
xuanji chat

# 查看 MCP 工具
xuanji mcp list
```

## 配置

编辑 `xuanji.toml`：

```toml
[llm]
default_provider = "deepseek"

[llm.providers.deepseek]
protocol = "openai"
model = "deepseek-chat"
api_key = "${DEEPSEEK_API_KEY}"
base_url = "https://api.deepseek.com/v1"

[agent]
max_loops = 20
confirm_risky = true

[[mcp_server]]
name = "shell"
command = "xuanji-mcp-shell"
```

## 架构

```
xuanji-cli → xuanji-agent → xuanji-llm (LLM 调用)
                       → xuanji-plugin (MCP 工具)
                       → xuanji-memory (上下文管理)
```

## License

MIT
