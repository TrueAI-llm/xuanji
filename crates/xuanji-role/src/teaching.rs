use crate::error::RoleError;
use crate::types::*;
use std::path::PathBuf;

/// Global shared teaching library across all roles.
pub struct TeachingLibrary {
    entries: Vec<Teaching>,
    path: PathBuf,
}

impl TeachingLibrary {
    const FILE_NAME: &'static str = "entries.json";

    /// Open or create the global teaching library.
    pub fn load() -> Result<Self, RoleError> {
        let dir = teaching_lib_dir();
        std::fs::create_dir_all(&dir).map_err(RoleError::Io)?;
        let path = dir.join(Self::FILE_NAME);
        let entries = if path.exists() {
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            if content.trim().is_empty() {
                Vec::new()
            } else {
                serde_json::from_str(&content).unwrap_or_default()
            }
        } else {
            Vec::new()
        };
        Ok(Self { entries, path })
    }

    /// Create a new empty library (ephemeral, no persistence).
    pub fn new_empty() -> Self {
        let dir = std::env::temp_dir().join("xuanji-teaching-test");
        let _ = std::fs::create_dir_all(&dir);
        Self {
            entries: Vec::new(),
            path: dir.join(Self::FILE_NAME),
        }
    }

    /// Publish a teaching from a role.
    pub fn publish(&mut self, teaching: Teaching) -> Result<(), RoleError> {
        if let Some(existing) = self.entries.iter_mut().find(|t| t.id == teaching.id) {
            *existing = teaching;
        } else {
            self.entries.push(teaching);
        }
        self.save()
    }

    /// Validate a teaching (increment validation count and confidence).
    pub fn validate(&mut self, teaching_id: &str, success: bool) -> Result<(), RoleError> {
        if let Some(t) = self.entries.iter_mut().find(|t| t.id == teaching_id) {
            t.validation_count += 1;
            if success {
                t.confidence = (t.confidence * 0.9 + 0.1).min(1.0);
            } else {
                t.confidence = (t.confidence * 0.95).max(0.1);
            }
        }
        self.save()
    }

    /// Query teachings by domain tags.
    pub fn query_by_tags(&self, tags: &[String]) -> Vec<&Teaching> {
        if tags.is_empty() {
            return self.entries.iter().collect();
        }
        self.entries
            .iter()
            .filter(|t| t.domain_tags.iter().any(|dt| tags.contains(dt)))
            .collect()
    }

    /// List all teachings.
    pub fn list(&self) -> &[Teaching] {
        &self.entries
    }

    /// Get a teaching by id.
    pub fn get(&self, id: &str) -> Option<&Teaching> {
        self.entries.iter().find(|t| t.id == id)
    }

    fn save(&self) -> Result<(), RoleError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(RoleError::Io)?;
        }
        let content = serde_json::to_string_pretty(&self.entries)?;
        std::fs::write(&self.path, content)?;
        Ok(())
    }
}

fn teaching_lib_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".xuanji")
        .join("teaching-library")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_publish_and_query() {
        let mut lib = TeachingLibrary::new_empty();

        let teaching = Teaching::new(
            "role-a",
            TeachingKind::Rule,
            "Always run cargo test before commit",
            vec!["rust".into(), "testing".into()],
        );
        let id = teaching.id.clone();
        lib.publish(teaching).unwrap();

        let results = lib.query_by_tags(&["rust".to_string()]);
        assert!(!results.is_empty());
        assert_eq!(results[0].author_role, "role-a");

        lib.validate(&id, true).unwrap();
        let t = lib.get(&id).unwrap();
        assert_eq!(t.validation_count, 1);
        assert!(t.confidence > 0.5);
    }

    #[test]
    fn test_teaching_serialization() {
        let t = Teaching::new(
            "role-a",
            TeachingKind::Heuristic,
            "Use shell.run for file operations",
            vec!["operations".into()],
        );
        let json = serde_json::to_string(&t).unwrap();
        let back: Teaching = serde_json::from_str(&json).unwrap();
        assert_eq!(back.author_role, "role-a");
        assert_eq!(back.kind, TeachingKind::Heuristic);
    }

    #[test]
    fn test_list_empty() {
        let lib = TeachingLibrary::new_empty();
        assert!(lib.list().is_empty());
    }

    #[test]
    fn test_get_not_found() {
        let lib = TeachingLibrary::new_empty();
        assert!(lib.get("nonexistent").is_none());
    }

    #[test]
    fn test_validate_failure_decreases_confidence() {
        let mut lib = TeachingLibrary::new_empty();
        let t = Teaching::new("test", TeachingKind::Rule, "test", vec![]);
        let id = t.id.clone();
        let initial = t.confidence;
        lib.publish(t).unwrap();
        lib.validate(&id, false).unwrap();
        let updated = lib.get(&id).unwrap();
        assert!(updated.confidence < initial);
    }
}
