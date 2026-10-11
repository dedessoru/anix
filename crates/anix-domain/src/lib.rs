mod entities;
mod policy;
mod runtime;
pub use entities::*;
pub use policy::*;
pub use runtime::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthorizationTrace {
    pub actor: Actor,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub project_id: String,
    pub branch_id: Option<String>,
    pub parent_branch_id: Option<String>,
    pub required_capability: String,
    pub granted_capabilities: Vec<String>,
    pub result: String,
    pub reason: String,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("validation: {0}")]
    Validation(String),
    #[error("conflicting command: {0}")]
    Conflict(String),
    #[error("invalid transition: {0}")]
    InvalidTransition(String),
    #[error("stale version; expected {expected}, actual {actual}")]
    StaleVersion { expected: u64, actual: u64 },
    #[error("stale proposal for {0:?}")]
    StaleProposal(EntityRef),
    #[error("permission denied")]
    PermissionDenied,
    #[error("permission denied")]
    AuthorizationDenied(Box<AuthorizationTrace>),
    #[error("human gate required")]
    GateRequired,
    #[error("gate already resolved")]
    GateAlreadyResolved,
    #[error("invalid relation")]
    InvalidRelation,
    #[error("entity belongs to a different project")]
    ProjectMismatch,
    #[error("storage: {0}")]
    Storage(String),
    #[error("model: {0}")]
    Model(String),
    #[error("tool: {0}")]
    Tool(String),
}

macro_rules! ids {
    ($($name:ident),+ $(,)?) => {$ (
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(uuid::Uuid);
        impl $name { pub fn new() -> Self { Self(uuid::Uuid::new_v4()) } }
        impl Default for $name { fn default() -> Self { Self::new() } }
        impl std::fmt::Display for $name { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { self.0.fmt(f) } }
        impl std::str::FromStr for $name {
            type Err = Error;
            fn from_str(s: &str) -> Result<Self> {
                let id = uuid::Uuid::parse_str(s).map_err(|_| Error::Validation(format!("invalid {}", stringify!($name))))?;
                if id.is_nil() { return Err(Error::Validation("nil identifier".into())); }
                Ok(Self(id))
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D:serde::Deserializer<'de>>(deserializer:D)->std::result::Result<Self,D::Error>{let value=String::deserialize(deserializer)?;value.parse().map_err(serde::de::Error::custom)}
        }
    )+};
}
ids!(
    ProjectId,
    QuestionId,
    SourceId,
    PaperId,
    ClaimId,
    HypothesisId,
    ExperimentId,
    ExperimentRunId,
    FindingId,
    ArtifactId,
    RelationId,
    GateId,
    TaskId,
    RunId,
    EventId,
    ProvenanceId,
    ActionId,
    WorkNodeId,
    WorkEdgeId,
    PlanRevisionId,
    BranchId,
    CheckpointId,
    ContextSnapshotId,
    ProcessMemoryId,
    ModelInvocationId,
    ToolInvocationId,
    CandidateClaimId
);

pub fn text(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > 1_000_000 {
        return Err(Error::Validation(format!(
            "{field} must contain 1..=1000000 bytes"
        )));
    }
    Ok(())
}

/// Reject credential-like metadata before it can enter persistent state or audit.
pub fn safe_metadata(value: &serde_json::Value) -> Result<()> {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                let key = key.to_lowercase().replace(['-', '_'], "");
                if [
                    "apikey",
                    "secret",
                    "password",
                    "authorization",
                    "accesstoken",
                    "refreshtoken",
                ]
                .iter()
                .any(|s| key.contains(s))
                {
                    return Err(Error::Validation("secret-bearing metadata".into()));
                }
                safe_metadata(value)?;
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                safe_metadata(value)?;
            }
        }
        serde_json::Value::String(s) if s.starts_with("sk-") || s.starts_with("Bearer ") => {
            return Err(Error::Validation("credential-like value".into()))
        }
        _ => {}
    }
    Ok(())
}
