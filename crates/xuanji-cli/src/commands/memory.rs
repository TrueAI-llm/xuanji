use anyhow::Result;
use xuanji_memory::LongTermMemory;

/// Show current project memory.
pub fn show_memory() -> Result<()> {
    let memory = LongTermMemory::default_path()?;
    let cwd = std::env::current_dir()?;
    let content = memory.load_for_project(&cwd)?;

    let prompt = LongTermMemory::to_prompt_context(&content);
    if prompt.is_empty() {
        println!("No memory found for current project.");
        println!("Memory will be built automatically as you use xuanji.");
    } else {
        println!("{}", prompt);
    }

    Ok(())
}

/// Clear all memory for the current project.
pub fn clear_memory() -> Result<()> {
    let memory = LongTermMemory::default_path()?;
    let cwd = std::env::current_dir()?;
    memory.clear_project(&cwd)?;
    println!("✅ Project memory cleared");
    Ok(())
}

/// Add a custom rule to long-term memory.
pub fn add_rule(text: &str) -> Result<()> {
    let memory = LongTermMemory::default_path()?;
    memory.add_rule(text)?;
    println!("✅ Rule added: {}", text);
    Ok(())
}
