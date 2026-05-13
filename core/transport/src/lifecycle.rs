use serde::{Deserialize, Serialize};
use std::{
    fmt,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeLifecycleState {
    Genesis,
    Bootstrap,
    Discovering,
    Authenticating,
    Syncing,
    Active,
    Partitioned,
    Quarantined,
    Offline,
}

impl fmt::Display for NodeLifecycleState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            NodeLifecycleState::Genesis => "GENESIS",
            NodeLifecycleState::Bootstrap => "BOOTSTRAP",
            NodeLifecycleState::Discovering => "DISCOVERING",
            NodeLifecycleState::Authenticating => "AUTHENTICATING",
            NodeLifecycleState::Syncing => "SYNCING",
            NodeLifecycleState::Active => "ACTIVE",
            NodeLifecycleState::Partitioned => "PARTITIONED",
            NodeLifecycleState::Quarantined => "QUARANTINED",
            NodeLifecycleState::Offline => "OFFLINE",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleTransition {
    pub from: NodeLifecycleState,
    pub to: NodeLifecycleState,
    pub reason: String,
}

pub struct LifecycleEngine {
    state: NodeLifecycleState,
    last_transition: Instant,
    history: Vec<LifecycleTransition>,
}

impl Default for LifecycleEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LifecycleEngine {
    pub fn new() -> Self {
        Self {
            state: NodeLifecycleState::Genesis,
            last_transition: Instant::now(),
            history: Vec::new(),
        }
    }

    pub fn state(&self) -> NodeLifecycleState {
        self.state
    }

    pub fn history(&self) -> &[LifecycleTransition] {
        &self.history
    }

    pub fn transition(
        &mut self,
        to: NodeLifecycleState,
        reason: impl Into<String>,
    ) -> Option<LifecycleTransition> {
        if self.state == to {
            return None;
        }

        let transition = LifecycleTransition {
            from: self.state,
            to,
            reason: reason.into(),
        };
        self.state = to;
        self.last_transition = Instant::now();
        self.history.push(transition.clone());
        Some(transition)
    }

    pub fn transition_if_stalled(
        &mut self,
        to: NodeLifecycleState,
        after: Duration,
        reason: impl Into<String>,
    ) -> Option<LifecycleTransition> {
        if self.last_transition.elapsed() >= after {
            self.transition(to, reason)
        } else {
            None
        }
    }

    pub fn mark_failure(&mut self, reason: impl Into<String>) -> Option<LifecycleTransition> {
        self.transition(NodeLifecycleState::Quarantined, reason)
    }

    pub fn mark_offline(&mut self, reason: impl Into<String>) -> Option<LifecycleTransition> {
        self.transition(NodeLifecycleState::Offline, reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_transitions_are_explicit() {
        let mut lifecycle = LifecycleEngine::new();
        let transition = lifecycle
            .transition(NodeLifecycleState::Bootstrap, "identity loaded")
            .unwrap();

        assert_eq!(transition.from, NodeLifecycleState::Genesis);
        assert_eq!(transition.to, NodeLifecycleState::Bootstrap);
        assert_eq!(lifecycle.state(), NodeLifecycleState::Bootstrap);
    }
}
