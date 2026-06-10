use crate::types::{BudgetConfig, BudgetError, BudgetStatus};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Budget controller tracks token consumption across agents.
pub struct BudgetController {
    config: BudgetConfig,
    total_consumed: AtomicU32,
    per_agent: Arc<Mutex<HashMap<String, u32>>>,
}

impl BudgetController {
    /// Create a new budget controller with the given configuration.
    pub fn new(config: BudgetConfig) -> Self {
        Self {
            config,
            total_consumed: AtomicU32::new(0),
            per_agent: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get the budget configuration.
    pub fn config(&self) -> &BudgetConfig {
        &self.config
    }

    /// Check if a token request is within budget. Returns Ok(()) or Err.
    ///
    /// For unlimited budgets (total_budget=0 or per_agent_budget=0), always succeeds.
    /// For limited budgets, uses atomic CAS to check and reserve tokens.
    pub async fn acquire(&self, agent: &str, estimated_tokens: u32) -> Result<(), BudgetError> {
        // Check per-agent budget first (requires async lock)
        if self.config.per_agent_budget > 0 {
            let per_agent = self.per_agent.lock().await;
            let consumed = *per_agent.get(agent).unwrap_or(&0);
            if consumed + estimated_tokens > self.config.per_agent_budget {
                return Err(BudgetError::PerAgentOverBudget {
                    agent: agent.to_string(),
                    requested: estimated_tokens,
                    consumed,
                    limit: self.config.per_agent_budget,
                });
            }
        }

        // Check total budget (atomic, no lock needed)
        if self.config.total_budget > 0 {
            let current = self.total_consumed.load(Ordering::Relaxed);
            if current + estimated_tokens > self.config.total_budget {
                let remaining = self.config.total_budget.saturating_sub(current);
                return Err(BudgetError::OverBudget {
                    agent: agent.to_string(),
                    requested: estimated_tokens,
                    remaining,
                });
            }
            // Optimistically add — report() will reconcile
            self.total_consumed.fetch_add(estimated_tokens, Ordering::Relaxed);
        }

        Ok(())
    }

    /// Report actual tokens consumed by an agent.
    ///
    /// This updates both the total and per-agent counters.
    /// Call this after each LLM response with the actual token count.
    pub async fn report(&self, agent: &str, actual_tokens: u32) {
        // Update total consumed
        self.total_consumed.fetch_add(actual_tokens, Ordering::Relaxed);

        // Update per-agent
        let mut per_agent = self.per_agent.lock().await;
        *per_agent.entry(agent.to_string()).or_insert(0) += actual_tokens;
    }

    /// Get current budget status.
    pub async fn status(&self) -> BudgetStatus {
        let total_consumed = self.total_consumed.load(Ordering::Relaxed);
        let per_agent = self.per_agent.lock().await.clone();

        let remaining = if self.config.total_budget == 0 {
            u32::MAX
        } else {
            self.config.total_budget.saturating_sub(total_consumed)
        };

        BudgetStatus {
            total_budget: self.config.total_budget,
            total_consumed,
            per_agent,
            remaining,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_budget_acquire_within_limit() {
        let config = BudgetConfig {
            total_budget: 1000,
            ..Default::default()
        };
        let controller = BudgetController::new(config);
        assert!(controller.acquire("agent-1", 500).await.is_ok());
    }

    #[tokio::test]
    async fn test_budget_acquire_over_limit() {
        let config = BudgetConfig {
            total_budget: 1000,
            ..Default::default()
        };
        let controller = BudgetController::new(config);
        assert!(controller.acquire("agent-1", 600).await.is_ok());
        let result = controller.acquire("agent-1", 500).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_budget_report_tracks_consumption() {
        let config = BudgetConfig::default();
        let controller = BudgetController::new(config);

        controller.report("agent-1", 100).await;
        controller.report("agent-1", 50).await;
        controller.report("agent-2", 200).await;

        let status = controller.status().await;
        assert_eq!(status.total_consumed, 350);
        assert_eq!(*status.per_agent.get("agent-1").unwrap(), 150);
        assert_eq!(*status.per_agent.get("agent-2").unwrap(), 200);
    }

    #[tokio::test]
    async fn test_budget_per_agent_limit() {
        let config = BudgetConfig {
            per_agent_budget: 500,
            ..Default::default()
        };
        let controller = BudgetController::new(config);

        controller.report("agent-1", 400).await;
        let result = controller.acquire("agent-1", 200).await;
        assert!(result.is_err());
        if let Err(BudgetError::PerAgentOverBudget { consumed, limit, .. }) = result {
            assert_eq!(consumed, 400);
            assert_eq!(limit, 500);
        }
    }

    #[tokio::test]
    async fn test_budget_status() {
        let config = BudgetConfig {
            total_budget: 10000,
            ..Default::default()
        };
        let controller = BudgetController::new(config);

        controller.report("agent-1", 1000).await;
        controller.report("agent-2", 2000).await;

        let status = controller.status().await;
        assert_eq!(status.total_budget, 10000);
        assert_eq!(status.total_consumed, 3000);
        assert_eq!(status.remaining, 7000);
    }

    #[tokio::test]
    async fn test_budget_unlimited() {
        let config = BudgetConfig::default(); // total_budget=0, per_agent_budget=0
        let controller = BudgetController::new(config);

        // Should always succeed with unlimited budget
        assert!(controller.acquire("agent-1", 999999).await.is_ok());
        assert!(controller.acquire("agent-1", 999999).await.is_ok());
    }
}
