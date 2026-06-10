use anyhow::{Context, Result};
use std::path::PathBuf;
use xuanji_trigger::DaemonRunner;

/// Paths for daemon runtime files.
fn xuanji_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".xuanji")
}

fn pid_file_path() -> PathBuf {
    xuanji_home().join("daemon.pid")
}

fn log_file_path() -> PathBuf {
    xuanji_home().join("daemon.log")
}

/// Start the daemon by spawning a background process.
pub fn start_daemon() -> Result<()> {
    let pid_path = pid_file_path();
    let log_path = log_file_path();

    // Check if already running
    if pid_path.exists() {
        let pid_str = std::fs::read_to_string(&pid_path)?;
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            // Check if process is alive (signal 0 = existence check)
            unsafe {
                if libc::kill(pid as i32, 0) == 0 {
                    anyhow::bail!("Daemon already running (PID: {})", pid);
                }
            }
        }
        // Stale PID file, clean up
        let _ = std::fs::remove_file(&pid_path);
    }

    // Ensure ~/.xuanji directory exists
    std::fs::create_dir_all(xuanji_home())?;

    // Ensure workflows directory exists
    let workflows_dir = xuanji_home().join("workflows");
    std::fs::create_dir_all(&workflows_dir)?;

    // Spawn the daemon process
    let exe = std::env::current_exe().context("Cannot determine current executable")?;
    let log_file = std::fs::File::create(&log_path).context("Cannot create log file")?;

    let child = std::process::Command::new(exe)
        .arg("_daemon_run")
        .arg("--pid-file")
        .arg(&pid_path)
        .arg("--log-file")
        .arg(&log_path)
        .stdout(log_file.try_clone()?)
        .stderr(log_file)
        .spawn()
        .context("Failed to spawn daemon process")?;

    let child_pid = child.id();
    println!("Starting daemon (PID: {})...", child_pid);

    // Wait for PID file to appear (max 3s)
    let start = std::time::Instant::now();
    loop {
        if pid_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&pid_path) {
                if let Ok(pid) = content.trim().parse::<u32>() {
                    println!("✅ Daemon started (PID: {})", pid);
                    println!("   Log: {}", log_path.display());
                    println!("   Workflows: {}", workflows_dir.display());
                    return Ok(());
                }
            }
        }
        if start.elapsed() > std::time::Duration::from_secs(3) {
            println!("⚠ Daemon process spawned but PID file not found yet");
            println!("   Check log: {}", log_path.display());
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Stop the daemon by sending SIGTERM.
pub fn stop_daemon() -> Result<()> {
    let pid_path = pid_file_path();

    if !pid_path.exists() {
        println!("Daemon is not running (no PID file)");
        return Ok(());
    }

    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: i32 = pid_str
        .trim()
        .parse()
        .context("Invalid PID file content")?;

    // Send SIGTERM
    unsafe {
        let result = libc::kill(pid, libc::SIGTERM);
        if result != 0 {
            anyhow::bail!("Failed to send SIGTERM to PID {}", pid);
        }
    }

    println!("Sent SIGTERM to daemon (PID: {})...", pid);

    // Wait for process to exit (max 5s)
    let start = std::time::Instant::now();
    loop {
        unsafe {
            if libc::kill(pid, 0) != 0 {
                // Process no longer exists
                let _ = std::fs::remove_file(&pid_path);
                println!("✅ Daemon stopped");
                return Ok(());
            }
        }
        if start.elapsed() > std::time::Duration::from_secs(5) {
            println!("⚠ Daemon did not stop within 5s, you may need to kill -9 {}", pid);
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// Check daemon status.
pub fn status_daemon() -> Result<()> {
    let pid_path = pid_file_path();

    if !pid_path.exists() {
        println!("Daemon is not running");
        return Ok(());
    }

    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: i32 = pid_str
        .trim()
        .parse()
        .context("Invalid PID file content")?;

    unsafe {
        if libc::kill(pid, 0) == 0 {
            println!("✅ Daemon is running (PID: {})", pid);
            println!("   Log: {}", log_file_path().display());
        } else {
            println!("❌ Daemon is not running (stale PID file for {})", pid);
            println!("   Run 'xuanji daemon start' to restart");
        }
    }

    Ok(())
}

/// Run the daemon process (called from the spawned child).
pub async fn run_daemon(pid_file: &str, log_file: &str) -> Result<()> {
    // Write PID file
    let pid = std::process::id();
    std::fs::write(pid_file, pid.to_string())?;

    tracing::info!("Daemon starting (PID: {})", pid);

    // Set up Ctrl+C handler for cleanup
    let pid_file_owned = pid_file.to_string();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("Received Ctrl+C, cleaning up...");
        let _ = std::fs::remove_file(&pid_file_owned);
        std::process::exit(0);
    });

    // Load config
    let config = crate::config::XuanjiConfig::load()?;

    let (_, provider_config) = crate::main_fns::get_default_provider(&config)?;

    // Create and run the daemon
    let runner = DaemonRunner::new(
        config.trigger,
        provider_config,
        config.mcp_servers,
    );

    let result = runner.run().await;

    // Clean up PID file
    let _ = std::fs::remove_file(pid_file);

    result.map_err(|e| anyhow::anyhow!("Daemon error: {}", e))
}
