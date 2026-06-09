use anyhow::Result;
use xuanji_plugin::process::McpProcess;
use xuanji_plugin::types::McpServerConfig;
use xuanji_plugin::ToolRegistry;

pub async fn list_tools(mcp_servers: &[McpServerConfig]) -> Result<()> {
    let mut registry = ToolRegistry::new();

    for config in mcp_servers {
        let process = McpProcess::new(config.clone());
        let mut client = xuanji_plugin::McpClient::new(process);
        client.initialize().await?;
        registry.register_server(client).await?;
    }

    let tools = registry.list_tools();
    if tools.is_empty() {
        println!("No MCP tools registered.");
        return Ok(());
    }

    println!("Registered MCP tools:\n");
    for (name, desc) in tools {
        println!("  {} - {}", name, desc);
    }

    registry.shutdown_all().await?;
    Ok(())
}
