use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrecencyCollectionPolicy {
    Session,
    Persistent,
    Disabled,
}

impl fmt::Display for FrecencyCollectionPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Session => "session",
            Self::Persistent => "persistent",
            Self::Disabled => "disabled",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CouplingPreparePolicy {
    LazyOnRequest,
    WarmOnStart,
    Disabled,
}

impl fmt::Display for CouplingPreparePolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::LazyOnRequest => "lazy on request",
            Self::WarmOnStart => "warm on start",
            Self::Disabled => "disabled",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorktreeRoutingPolicy {
    ExplicitCallTime,
    Disabled,
}

impl fmt::Display for WorktreeRoutingPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::ExplicitCallTime => "explicit call-time",
            Self::Disabled => "disabled",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RankingDiagnosticsPolicy {
    CallTimeExplain,
    DefaultOn,
    Disabled,
}

impl fmt::Display for RankingDiagnosticsPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::CallTimeExplain => "call-time explain",
            Self::DefaultOn => "default on",
            Self::Disabled => "disabled",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CapabilityPolicy {
    pub frecency_collection: FrecencyCollectionPolicy,
    pub coupling_prepare: CouplingPreparePolicy,
    pub worktree_routing: WorktreeRoutingPolicy,
    pub ranking_diagnostics: RankingDiagnosticsPolicy,
}

impl CapabilityPolicy {
    pub fn disabled() -> Self {
        Self {
            frecency_collection: FrecencyCollectionPolicy::Disabled,
            coupling_prepare: CouplingPreparePolicy::Disabled,
            worktree_routing: WorktreeRoutingPolicy::Disabled,
            ranking_diagnostics: RankingDiagnosticsPolicy::Disabled,
        }
    }
}

impl Default for CapabilityPolicy {
    fn default() -> Self {
        Self {
            frecency_collection: FrecencyCollectionPolicy::Session,
            coupling_prepare: CouplingPreparePolicy::LazyOnRequest,
            worktree_routing: WorktreeRoutingPolicy::ExplicitCallTime,
            ranking_diagnostics: RankingDiagnosticsPolicy::CallTimeExplain,
        }
    }
}
