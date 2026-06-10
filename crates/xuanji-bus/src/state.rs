use crate::bus::KnowledgeBus;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use tokio::sync::Mutex;

/// A declaration of modification intent.
#[derive(Debug, Clone)]
pub struct IntentScope {
    /// File paths this agent intends to modify.
    pub files: Vec<String>,
    /// Logical resources this agent intends to modify.
    pub resources: Vec<String>,
}

/// Ticket returned after declaring intent.
#[derive(Debug, Clone)]
pub struct IntentTicket {
    /// Agent that holds this intent.
    pub agent: String,
    /// The declared scope.
    pub scope: IntentScope,
}

/// A state entry with version info.
#[derive(Debug, Clone)]
pub struct StateEntry {
    /// The stored value.
    pub value: Value,
    /// Version number for CAS.
    pub version: u64,
    /// Agent that last modified this entry.
    pub last_modified_by: String,
}

/// Error type for shared state operations.
#[derive(Debug)]
pub enum StateError {
    /// Another agent already holds a conflicting intent.
    Conflict {
        agent: String,
        conflicting_agent: String,
        conflicting_files: Vec<String>,
    },
    /// CAS write failed due to version mismatch.
    VersionMismatch {
        key: String,
        expected: u64,
        actual: u64,
    },
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateError::Conflict { agent, conflicting_agent, conflicting_files } => {
                write!(f, "Agent '{}': conflicts with '{}' on files {:?}", agent, conflicting_agent, conflicting_files)
            }
            StateError::VersionMismatch { key, expected, actual } => {
                write!(f, "Version mismatch for '{}': expected {}, actual {}", key, expected, actual)
            }
        }
    }
}

impl std::error::Error for StateError {}

/// Shared state layer for conflict prevention between agents.
pub struct SharedState {
    /// Currently declared intents: agent_name → scope.
    intents: Arc<Mutex<HashMap<String, IntentScope>>>,
    /// Key-value state store with versioning.
    state: Arc<Mutex<HashMap<String, StateEntry>>>,
    /// Bus for broadcasting state changes.
    bus: KnowledgeBus,
}

impl SharedState {
    /// Create a new shared state layer backed by the given knowledge bus.
    pub fn new(bus: KnowledgeBus) -> Self {
        Self {
            intents: Arc::new(Mutex::new(HashMap::new())),
            state: Arc::new(Mutex::new(HashMap::new())),
            bus,
        }
    }

    /// Declare modification intent for a set of files/resources.
    ///
    /// Returns `Ok(IntentTicket)` if no conflicts exist.
    /// Returns `Err(StateError::Conflict)` if another agent already holds
    /// an overlapping intent.
    pub async fn declare_intent(
        &self,
        agent: &str,
        scope: IntentScope,
    ) -> Result<IntentTicket, StateError> {
        let mut intents = self.intents.lock().await;

        // Check for conflicts with existing intents
        let mut conflicting_files = Vec::new();
        let mut conflicting_agent = String::new();

        for (existing_agent, existing_scope) in intents.iter() {
            if existing_agent == agent {
                continue; // Same agent can re-declare
            }

            // Check file overlap
            for file in &scope.files {
                if existing_scope.files.iter().any(|f| files_overlap(f, file)) {
                    if conflicting_agent.is_empty() {
                        conflicting_agent = existing_agent.clone();
                    }
                    conflicting_files.push(file.clone());
                }
            }

            // Check resource overlap
            for resource in &scope.resources {
                if existing_scope.resources.contains(resource) {
                    if conflicting_agent.is_empty() {
                        conflicting_agent = existing_agent.clone();
                    }
                    conflicting_files.push(resource.clone());
                }
            }
        }

        if !conflicting_files.is_empty() {
            return Err(StateError::Conflict {
                agent: agent.to_string(),
                conflicting_agent,
                conflicting_files,
            });
        }

        // No conflicts — register the intent
        intents.insert(agent.to_string(), scope.clone());

        // Broadcast the intent declaration
        self.bus.publish_state(agent, serde_json::json!({
            "action": "declare_intent",
            "files": scope.files,
            "resources": scope.resources,
        }));

        Ok(IntentTicket {
            agent: agent.to_string(),
            scope,
        })
    }

    /// Read a state value.
    pub async fn read(&self, key: &str) -> Option<StateEntry> {
        let state = self.state.lock().await;
        state.get(key).cloned()
    }

    /// Write a state value with CAS (compare-and-swap).
    ///
    /// The `expected_version` from the ticket must match the current version.
    /// On success, the version is incremented.
    pub async fn write(
        &self,
        ticket: &IntentTicket,
        key: &str,
        value: Value,
    ) -> Result<(), StateError> {
        let mut state = self.state.lock().await;

        let expected_version = 0; // New entries start at version 0
        let current_version = state.get(key).map(|e| e.version).unwrap_or(0);

        if current_version != expected_version && state.contains_key(key) {
            return Err(StateError::VersionMismatch {
                key: key.to_string(),
                expected: expected_version,
                actual: current_version,
            });
        }

        let new_version = current_version + 1;
        state.insert(
            key.to_string(),
            StateEntry {
                value,
                version: new_version,
                last_modified_by: ticket.agent.clone(),
            },
        );

        // Broadcast the state change
        self.bus.publish_state(&ticket.agent, serde_json::json!({
            "action": "write",
            "key": key,
            "version": new_version,
        }));

        Ok(())
    }

    /// Write with explicit expected version for CAS.
    pub async fn write_cas(
        &self,
        ticket: &IntentTicket,
        key: &str,
        value: Value,
        expected_version: u64,
    ) -> Result<(), StateError> {
        let mut state = self.state.lock().await;

        let current_version = state.get(key).map(|e| e.version).unwrap_or(0);

        if current_version != expected_version {
            return Err(StateError::VersionMismatch {
                key: key.to_string(),
                expected: expected_version,
                actual: current_version,
            });
        }

        let new_version = current_version + 1;
        state.insert(
            key.to_string(),
            StateEntry {
                value,
                version: new_version,
                last_modified_by: ticket.agent.clone(),
            },
        );

        self.bus.publish_state(&ticket.agent, serde_json::json!({
            "action": "write",
            "key": key,
            "version": new_version,
        }));

        Ok(())
    }

    /// Release an intent when the agent is done.
    pub async fn release_intent(&self, agent: &str) {
        let mut intents = self.intents.lock().await;
        intents.remove(agent);

        self.bus.publish_state(agent, serde_json::json!({
            "action": "release_intent",
        }));
    }
}

/// Check if two file patterns overlap.
/// Simple implementation: exact match or one is a prefix of the other.
fn files_overlap(a: &str, b: &str) -> bool {
    a == b || a.starts_with(b) || b.starts_with(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_shared_state_declare_intent() {
        let bus = KnowledgeBus::new(16);
        let state = SharedState::new(bus);

        let scope = IntentScope {
            files: vec!["src/main.rs".to_string()],
            resources: vec![],
        };
        let result = state.declare_intent("agent-1", scope).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_shared_state_conflict_detection() {
        let bus = KnowledgeBus::new(16);
        let state = SharedState::new(bus);

        let scope1 = IntentScope {
            files: vec!["src/main.rs".to_string()],
            resources: vec![],
        };
        state.declare_intent("agent-1", scope1).await.unwrap();

        let scope2 = IntentScope {
            files: vec!["src/main.rs".to_string()],
            resources: vec![],
        };
        let result = state.declare_intent("agent-2", scope2).await;
        assert!(result.is_err());
        if let Err(StateError::Conflict { conflicting_agent, .. }) = result {
            assert_eq!(conflicting_agent, "agent-1");
        }
    }

    #[tokio::test]
    async fn test_shared_state_cas_write() {
        let bus = KnowledgeBus::new(16);
        let state = SharedState::new(bus);

        let ticket = IntentTicket {
            agent: "agent-1".to_string(),
            scope: IntentScope {
                files: vec![],
                resources: vec![],
            },
        };

        // First write (version 0 → 1)
        state.write(&ticket, "key1", serde_json::json!("value1")).await.unwrap();
        let entry = state.read("key1").await.unwrap();
        assert_eq!(entry.version, 1);
        assert_eq!(entry.value, serde_json::json!("value1"));

        // Second write (version 1 → 2)
        state.write_cas(&ticket, "key1", serde_json::json!("value2"), 1).await.unwrap();
        let entry = state.read("key1").await.unwrap();
        assert_eq!(entry.version, 2);
    }

    #[tokio::test]
    async fn test_shared_state_cas_version_mismatch() {
        let bus = KnowledgeBus::new(16);
        let state = SharedState::new(bus);

        let ticket = IntentTicket {
            agent: "agent-1".to_string(),
            scope: IntentScope {
                files: vec![],
                resources: vec![],
            },
        };

        state.write(&ticket, "key1", serde_json::json!("v1")).await.unwrap();
        // Try CAS with wrong version
        let result = state.write_cas(&ticket, "key1", serde_json::json!("v2"), 0).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_shared_state_release_intent() {
        let bus = KnowledgeBus::new(16);
        let state = SharedState::new(bus);

        let scope = IntentScope {
            files: vec!["src/main.rs".to_string()],
            resources: vec![],
        };
        state.declare_intent("agent-1", scope).await.unwrap();
        state.release_intent("agent-1").await;

        // After release, another agent can declare intent on same files
        let scope2 = IntentScope {
            files: vec!["src/main.rs".to_string()],
            resources: vec![],
        };
        let result = state.declare_intent("agent-2", scope2).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_files_overlap_prefix() {
        assert!(files_overlap("src/", "src/main.rs"));
        assert!(files_overlap("src/main.rs", "src/"));
        assert!(!files_overlap("src/", "lib/"));
    }
}
