use anyhow::Result;
use std::collections::HashMap;
use xuanji_plugin::process::McpProcess;
use xuanji_plugin::types::McpServerConfig;
use xuanji_plugin::ToolRegistry;

pub async fn list_tools(mcp_servers: &[McpServerConfig]) -> Result<()> {
    let mut registry = ToolRegistry::new();
    let mut failures = Vec::new();

    for config in mcp_servers {
        let process = McpProcess::new(config.clone());
        let mut client = xuanji_plugin::McpClient::new(process);
        match client.initialize().await {
            Ok(()) => {
                registry.register_server(client).await?;
            }
            Err(e) => {
                failures.push((config.name.clone(), config.command.clone(), e.to_string()));
            }
        }
    }

    let tools = registry.list_tools();
    if tools.is_empty() && failures.is_empty() {
        println!("No MCP tools registered.");
        return Ok(());
    }

    if !tools.is_empty() {
        println!("Registered MCP tools:\n");
        for (name, desc) in tools {
            println!("  {} - {}", name, desc);
        }
    }

    registry.shutdown_all().await?;

    if !failures.is_empty() {
        println!("\n⚠ Failed to start {} server(s):", failures.len());
        for (name, command, error) in &failures {
            println!("  • {} ({}) — {}", name, command, error);
        }
        // Suggest installation for common tools
        for (_, command, _) in &failures {
            if command == "uvx" {
                println!("\n  💡 Install uvx: pip install uv  or  curl -LsSf https://astral.sh/uv/install.sh | sh");
            } else if command == "npx" {
                println!("\n  💡 Install npx: it comes with Node.js (https://nodejs.org)");
            }
        }
    }

    Ok(())
}

/// Add an MCP server to the config file.
pub fn add_server(
    name: &str,
    command: &str,
    args: &[String],
    env_pairs: &[String],
    global: bool,
) -> Result<()> {
    let mut config = if global {
        crate::config::XuanjiConfig::load_global_only()?
    } else {
        crate::config::XuanjiConfig::load_local_only()?
    };

    let server = McpServerConfig {
        name: name.to_string(),
        command: command.to_string(),
        args: args.to_vec(),
        env: parse_env_vars(env_pairs),
    };

    config.add_mcp_server(server);

    if global {
        config.save_global()?;
    } else {
        config.save_local()?;
    }

    println!("✓ Added MCP server '{}' (command: {})", name, command);
    Ok(())
}

/// Remove an MCP server from the config file.
pub fn remove_server(name: &str, global: bool) -> Result<()> {
    let mut config = if global {
        crate::config::XuanjiConfig::load_global_only()?
    } else {
        crate::config::XuanjiConfig::load_local_only()?
    };

    if config.remove_mcp_server(name) {
        if global {
            config.save_global()?;
        } else {
            config.save_local()?;
        }
        println!("✓ Removed MCP server '{}'", name);
    } else {
        println!("MCP server '{}' not found in config", name);
    }

    Ok(())
}

/// Install an MCP server from a package identifier (npm or Python).
pub fn install_server(
    package: &str,
    name_override: Option<&str>,
    type_override: Option<&str>,
    env_pairs: &[String],
    global: bool,
) -> Result<()> {
    let package_type = detect_package_type(package, type_override);
    let server_name = name_override.unwrap_or_else(|| {
        // Extract short name from scoped package: @scope/pkg → pkg
        package
            .split('/')
            .last()
            .unwrap_or(package)
    });

    let (command, args) = match package_type {
        PackageType::Npm => ("npx".to_string(), vec!["-y".to_string(), package.to_string()]),
        PackageType::Python => ("uvx".to_string(), vec![package.to_string()]),
    };

    // Check if the command exists on the system
    if which::which(&command).is_err() {
        let hint = match package_type {
            PackageType::Npm => "Install Node.js: https://nodejs.org",
            PackageType::Python => "Install uv: pip install uv  or  curl -LsSf https://astral.sh/uv/install.sh | sh",
        };
        anyhow::bail!(
            "Command '{}' not found in PATH. {}\n\
             The config entry was NOT written. Install the dependency first, or use:\n\
             xuanji mcp add {} --command <alternative-command>",
            command, hint, server_name
        );
    }

    let mut config = if global {
        crate::config::XuanjiConfig::load_global_only()?
    } else {
        crate::config::XuanjiConfig::load_local_only()?
    };

    let server = McpServerConfig {
        name: server_name.to_string(),
        command,
        args,
        env: parse_env_vars(env_pairs),
    };

    config.add_mcp_server(server);

    if global {
        config.save_global()?;
    } else {
        config.save_local()?;
    }

    let type_label = match package_type {
        PackageType::Npm => "npm",
        PackageType::Python => "Python",
    };
    println!(
        "✓ Installed MCP server '{}' from {} package '{}'",
        server_name, type_label, package
    );
    println!("  Run 'xuanji mcp list' to verify.");
    Ok(())
}

// ─── Helpers ────────────────────────────────────────────────────────────────

enum PackageType {
    Npm,
    Python,
}

fn detect_package_type(package: &str, type_override: Option<&str>) -> PackageType {
    if let Some(t) = type_override {
        return match t.to_lowercase().as_str() {
            "npm" | "node" => PackageType::Npm,
            "python" | "pip" | "pypi" => PackageType::Python,
            _ => PackageType::Npm,
        };
    }

    // Auto-detect: npm packages start with @ or contain /
    if package.starts_with('@') || package.contains('/') {
        PackageType::Npm
    } else {
        PackageType::Python
    }
}

/// Parse "KEY=VALUE" strings into a HashMap.
fn parse_env_vars(env_pairs: &[String]) -> HashMap<String, String> {
    env_pairs
        .iter()
        .filter_map(|s| {
            let mut parts = s.splitn(2, '=');
            let key = parts.next()?.to_string();
            let value = parts.next()?.to_string();
            Some((key, value))
        })
        .collect()
}
