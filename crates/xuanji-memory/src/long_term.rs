use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// A history entry for a completed task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: String,
    pub goal: String,
    pub summary: String,
    pub success: bool,
}

/// Project-specific context stored in long-term memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectContext {
    pub name: String,
    #[serde(default)]
    pub tech_stack: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
    pub updated_at: String,
}

/// The full memory content loaded for a project.
#[derive(Debug, Clone, Default)]
pub struct MemoryContent {
    pub preferences: HashMap<String, String>,
    pub rules: Vec<String>,
    pub project_context: Option<ProjectContext>,
    pub recent_history: Vec<HistoryEntry>,
}

/// Long-term memory store backed by filesystem (JSON + JSONL).
pub struct LongTermMemory {
    base_dir: PathBuf,
}

impl LongTermMemory {
    /// Create a new LongTermMemory with the given base directory.
    /// Creates the directory if it doesn't exist.
    pub fn new(base_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&base_dir)?;
        Ok(Self { base_dir })
    }

    /// Create with default path: ~/.xuanji/memory/
    pub fn default_path() -> Result<Self> {
        let base = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".xuanji")
            .join("memory");
        Self::new(base)
    }

    /// Load all memory content for a specific project directory.
    pub fn load_for_project(&self, project_dir: &Path) -> Result<MemoryContent> {
        let mut content = MemoryContent::default();

        // 1. Load global preferences
        content.preferences = self.load_preferences()?;

        // 2. Load rules
        content.rules = self.load_rules()?;

        // 3. Load project context (if exists)
        let project_hash = path_to_hash(project_dir);
        let project_dir_path = self.base_dir.join("projects").join(&project_hash);
        let context_path = project_dir_path.join("context.json");
        if context_path.exists() {
            let data = std::fs::read_to_string(&context_path)?;
            content.project_context = Some(serde_json::from_str(&data)?);
        }

        // 4. Load recent history (last 5 entries)
        let history_path = project_dir_path.join("history.jsonl");
        if history_path.exists() {
            let data = std::fs::read_to_string(&history_path)?;
            let entries: Vec<HistoryEntry> = data
                .lines()
                .filter(|l| !l.is_empty())
                .filter_map(|l| serde_json::from_str(l).ok())
                .collect();
            let start = entries.len().saturating_sub(5);
            content.recent_history = entries[start..].to_vec();
        }

        Ok(content)
    }

    /// Generate a markdown summary for injection into the system prompt.
    pub fn to_prompt_context(content: &MemoryContent) -> String {
        let mut sections = Vec::new();

        if !content.preferences.is_empty() {
            let mut lines = vec!["## 用户偏好".to_string()];
            for (k, v) in &content.preferences {
                lines.push(format!("- {}: {}", k, v));
            }
            sections.push(lines.join("\n"));
        }

        if !content.rules.is_empty() {
            let mut lines = vec!["## 自定义规则".to_string()];
            for rule in &content.rules {
                lines.push(format!("- {}", rule));
            }
            sections.push(lines.join("\n"));
        }

        if let Some(ctx) = &content.project_context {
            let mut lines = vec!["## 项目知识".to_string()];
            if !ctx.tech_stack.is_empty() {
                lines.push(format!("- 技术栈: {}", ctx.tech_stack.join(", ")));
            }
            for note in &ctx.notes {
                lines.push(format!("- {}", note));
            }
            sections.push(lines.join("\n"));
        }

        if !content.recent_history.is_empty() {
            let mut lines = vec!["## 最近执行记录".to_string()];
            for entry in &content.recent_history {
                let status = if entry.success { "成功" } else { "失败" };
                lines.push(format!(
                    "- [{}] {} → {}",
                    entry.timestamp, entry.goal, status
                ));
            }
            sections.push(lines.join("\n"));
        }

        sections.join("\n\n")
    }

    // --- Write operations ---

    /// Save a preference key-value pair.
    pub fn save_preference(&self, key: &str, value: &str) -> Result<()> {
        let mut prefs = self.load_preferences()?;
        prefs.insert(key.to_string(), value.to_string());
        self.write_json("preferences.json", &prefs)
    }

    /// Add a custom rule.
    pub fn add_rule(&self, rule: &str) -> Result<()> {
        let mut rules = self.load_rules()?;
        rules.push(rule.to_string());
        self.write_json("rules.json", &rules)
    }

    /// Remove a rule by index. Returns true if removed.
    pub fn remove_rule(&self, index: usize) -> Result<bool> {
        let mut rules = self.load_rules()?;
        if index < rules.len() {
            rules.remove(index);
            self.write_json("rules.json", &rules)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Save project context.
    pub fn save_project_context(&self, project_dir: &Path, ctx: &ProjectContext) -> Result<()> {
        let project_hash = path_to_hash(project_dir);
        let project_dir_path = self.base_dir.join("projects").join(&project_hash);
        std::fs::create_dir_all(&project_dir_path)?;
        let path = project_dir_path.join("context.json");
        write_json_atomic(&path, ctx)
    }

    /// Add a note to the project context (creates context if needed).
    pub fn add_project_note(&self, project_dir: &Path, note: &str) -> Result<()> {
        let project_hash = path_to_hash(project_dir);
        let project_dir_path = self.base_dir.join("projects").join(&project_hash);
        std::fs::create_dir_all(&project_dir_path)?;

        let context_path = project_dir_path.join("context.json");
        let mut ctx: ProjectContext = if context_path.exists() {
            let data = std::fs::read_to_string(&context_path)?;
            serde_json::from_str(&data)?
        } else {
            ProjectContext {
                name: project_dir
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                tech_stack: Vec::new(),
                notes: Vec::new(),
                updated_at: chrono_now(),
            }
        };

        ctx.notes.push(note.to_string());
        ctx.updated_at = chrono_now();
        write_json_atomic(&context_path, &ctx)
    }

    /// Append a history entry for a project.
    pub fn append_history(&self, project_dir: &Path, entry: HistoryEntry) -> Result<()> {
        let project_hash = path_to_hash(project_dir);
        let project_dir_path = self.base_dir.join("projects").join(&project_hash);
        std::fs::create_dir_all(&project_dir_path)?;

        let history_path = project_dir_path.join("history.jsonl");
        let line = serde_json::to_string(&entry)? + "\n";

        // Use OpenOptions::append for atomic append
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&history_path)?;
        file.write_all(line.as_bytes())?;

        // Keep history file bounded (max 100 entries)
        self.trim_history(&history_path, 100)?;

        Ok(())
    }

    /// Clear all memory for a project.
    pub fn clear_project(&self, project_dir: &Path) -> Result<()> {
        let project_hash = path_to_hash(project_dir);
        let project_dir_path = self.base_dir.join("projects").join(&project_hash);
        if project_dir_path.exists() {
            std::fs::remove_dir_all(&project_dir_path)?;
        }
        Ok(())
    }

    /// Clear all global preferences and rules.
    pub fn clear_global(&self) -> Result<()> {
        let prefs_path = self.base_dir.join("preferences.json");
        let rules_path = self.base_dir.join("rules.json");
        if prefs_path.exists() {
            std::fs::remove_file(prefs_path)?;
        }
        if rules_path.exists() {
            std::fs::remove_file(rules_path)?;
        }
        Ok(())
    }

    // --- Internal helpers ---

    fn load_preferences(&self) -> Result<HashMap<String, String>> {
        let path = self.base_dir.join("preferences.json");
        if !path.exists() {
            return Ok(HashMap::new());
        }
        let data = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&data).unwrap_or_default())
    }

    fn load_rules(&self) -> Result<Vec<String>> {
        let path = self.base_dir.join("rules.json");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let data = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&data).unwrap_or_default())
    }

    fn write_json<T: Serialize>(&self, filename: &str, data: &T) -> Result<()> {
        let path = self.base_dir.join(filename);
        write_json_atomic(&path, data)
    }

    fn trim_history(&self, path: &Path, max_entries: usize) -> Result<()> {
        let data = std::fs::read_to_string(path).unwrap_or_default();
        let entries: Vec<String> = data
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();

        if entries.len() <= max_entries {
            return Ok(());
        }

        let start = entries.len() - max_entries;
        let trimmed = entries[start..].join("\n") + "\n";
        std::fs::write(path, trimmed)?;
        Ok(())
    }
}

/// Hash a path to a hex string for use as a directory name.
fn path_to_hash(path: &Path) -> String {
    let canonical = std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf());
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    canonical.to_string_lossy().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Write JSON atomically: write to temp file, then rename.
fn write_json_atomic<T: Serialize>(path: &Path, data: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(data)?;
    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, &json)?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

/// Simple timestamp string.
fn chrono_now() -> String {
    // Use a simple format without depending on chrono
    let output = std::process::Command::new("date")
        .arg("+%Y-%m-%d")
        .output();
    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(_) => "unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_dir() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("memory");
        (dir, base)
    }

    #[test]
    fn test_new_creates_directory() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("memory");
        assert!(!base.exists());
        let _mem = LongTermMemory::new(base.clone()).unwrap();
        assert!(base.exists());
    }

    #[test]
    fn test_save_and_load_preferences() {
        let (_dir, base) = temp_dir();
        let mem = LongTermMemory::new(base).unwrap();

        mem.save_preference("language", "中文").unwrap();
        mem.save_preference("editor", "vim").unwrap();

        let prefs = mem.load_preferences().unwrap();
        assert_eq!(prefs.get("language").unwrap(), "中文");
        assert_eq!(prefs.get("editor").unwrap(), "vim");
    }

    #[test]
    fn test_add_and_load_rules() {
        let (_dir, base) = temp_dir();
        let mem = LongTermMemory::new(base).unwrap();

        mem.add_rule("使用 pnpm").unwrap();
        mem.add_rule("提交信息用英文").unwrap();

        let rules = mem.load_rules().unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0], "使用 pnpm");
    }

    #[test]
    fn test_remove_rule() {
        let (_dir, base) = temp_dir();
        let mem = LongTermMemory::new(base).unwrap();

        mem.add_rule("rule1").unwrap();
        mem.add_rule("rule2").unwrap();
        mem.remove_rule(0).unwrap();

        let rules = mem.load_rules().unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0], "rule2");
    }

    #[test]
    fn test_append_and_load_history() {
        let (_dir, base) = temp_dir();
        let mem = LongTermMemory::new(base).unwrap();
        let proj = PathBuf::from("/tmp/test-project");

        mem.append_history(
            &proj,
            HistoryEntry {
                timestamp: "2026-06-10".to_string(),
                goal: "部署到测试环境".to_string(),
                summary: "执行成功".to_string(),
                success: true,
            },
        )
        .unwrap();

        mem.append_history(
            &proj,
            HistoryEntry {
                timestamp: "2026-06-09".to_string(),
                goal: "添加触发器".to_string(),
                summary: "P2 完成".to_string(),
                success: true,
            },
        )
        .unwrap();

        let content = mem.load_for_project(&proj).unwrap();
        assert_eq!(content.recent_history.len(), 2);
        assert_eq!(content.recent_history[0].goal, "部署到测试环境");
        assert_eq!(content.recent_history[1].goal, "添加触发器");
    }

    #[test]
    fn test_project_context() {
        let (_dir, base) = temp_dir();
        let mem = LongTermMemory::new(base).unwrap();
        let proj = PathBuf::from("/tmp/test-project");

        mem.add_project_note(&proj, "使用 anthropic 协议").unwrap();

        let content = mem.load_for_project(&proj).unwrap();
        assert!(content.project_context.is_some());
        let ctx = content.project_context.unwrap();
        assert!(ctx.notes.contains(&"使用 anthropic 协议".to_string()));
    }

    #[test]
    fn test_to_prompt_context() {
        let mut content = MemoryContent::default();
        content.preferences.insert("language".to_string(), "中文".to_string());
        content.rules.push("使用 pnpm".to_string());

        let prompt = LongTermMemory::to_prompt_context(&content);
        assert!(prompt.contains("用户偏好"));
        assert!(prompt.contains("自定义规则"));
        assert!(prompt.contains("使用 pnpm"));
    }

    #[test]
    fn test_clear_project() {
        let (_dir, base) = temp_dir();
        let mem = LongTermMemory::new(base).unwrap();
        let proj = PathBuf::from("/tmp/test-project");

        mem.add_project_note(&proj, "test").unwrap();
        mem.clear_project(&proj).unwrap();

        let content = mem.load_for_project(&proj).unwrap();
        assert!(content.project_context.is_none());
    }

    #[test]
    fn test_path_to_hash_deterministic() {
        let path = PathBuf::from("/tmp/test-project");
        let h1 = path_to_hash(&path);
        let h2 = path_to_hash(&path);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 16);
    }
}
