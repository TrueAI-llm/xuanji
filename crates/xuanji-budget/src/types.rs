use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Budget configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    /// Total token budget across all agents (0 = unlimited).
    #[serde(default)]
    pub total_budget: u32,
    /// Per-agent token budget (0 = unlimited).
    #[serde(default)]
    pub per_agent_budget: u32,
    /// Max recursion depth for agent delegation.
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,
}

fn default_max_depth() -> u32 {
    3
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            total_budget: 0,
            per_agent_budget: 0,
            max_depth: default_max_depth(),
        }
    }
}

/// Current budget status.
#[derive(Debug, Clone)]
pub struct BudgetStatus {
    /// Total budget (0 = unlimited).
    pub total_budget: u32,
    /// Total tokens consumed across all agents.
    pub total_consumed: u32,
    /// Per-agent token consumption.
    pub per_agent: HashMap<String, u32>,
    /// Remaining budget (u32::MAX if unlimited).
    pub remaining: u32,
}

/// Error type for budget operations.
#[derive(Debug)]
pub enum BudgetError {
    /// The total budget has been exceeded.
    OverBudget {
        agent: String,
        requested: u32,
        remaining: u32,
    },
    /// The per-agent budget has been exceeded.
    PerAgentOverBudget {
        agent: String,
        requested: u32,
        consumed: u32,
        limit: u32,
    },
    /// The delegation depth limit has been exceeded.
    DepthExceeded {
        agent: String,
        current_depth: u32,
        max_depth: u32,
    },
}

impl fmt::Display for BudgetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BudgetError::OverBudget { agent, requested, remaining } => {
                write!(f, "Agent '{}': over total budget (requested {}, remaining {})", agent, requested, remaining)
            }
            BudgetError::PerAgentOverBudget { agent, requested, consumed, limit } => {
                write!(f, "Agent '{}': over per-agent budget (requested {}, consumed {}/{})", agent, requested, consumed, limit)
            }
            BudgetError::DepthExceeded { agent, current_depth, max_depth } => {
                write!(f, "Agent '{}': delegation depth {} exceeds max {}", agent, current_depth, max_depth)
            }
        }
    }
}

impl std::error::Error for BudgetError {}
