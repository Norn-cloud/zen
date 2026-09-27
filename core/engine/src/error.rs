use crate::engine::EvaluationTraceKind;
use crate::loader::LoaderError;
use crate::DecisionGraphValidationError;
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use serde_json::Value;
use std::fmt;
use std::sync::Arc;
use thiserror::Error;
use zen_types::variable::Variable;

#[derive(Debug, Error)]
#[error("expected {expected} content, got {got}")]
pub struct ContentKindError {
    pub expected: &'static str,
    pub got: &'static str,
}

/// Norn: returned (as the `source` of [`EvaluationError::NodeError`]) when a graph
/// contains a node kind whose runtime was compiled out, e.g. a function node when the
/// `js` feature is disabled. Downcast `source` to detect it.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error(
    "{node_kind} is not supported in this build (enable the `{feature}` feature of zen-engine)"
)]
pub struct UnsupportedNodeError {
    pub node_kind: &'static str,
    pub feature: &'static str,
}

/// Norn: returned (as the `source` of [`EvaluationError::NodeError`]) when a node's JSON
/// schema cannot be compiled. `unresolved_reference` is set when compilation failed on
/// a `$ref` that could not be retrieved, e.g. a remote or file reference in a build
/// without the `schema-resolvers` feature. The message is jsonschema's own.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct SchemaCompileError {
    pub message: String,
    pub unresolved_reference: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileFailure {
    pub key: Arc<str>,
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<crate::policy::Diagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl fmt::Display for CompileFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} [{}]: ", self.key, self.kind)?;
        match &self.error {
            Some(error) => write!(f, "{error}"),
            None => {
                let messages: Vec<&str> = self
                    .diagnostics
                    .iter()
                    .map(|d| d.message.as_str())
                    .collect();
                write!(f, "{}", messages.join("; "))
            }
        }
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EvaluationError {
    #[error("Loader error")]
    LoaderError(LoaderError),

    #[error("{source}")]
    NodeError {
        node_id: Arc<str>,
        trace: Option<Variable>,
        source: Box<dyn std::error::Error>,
    },

    #[error("Depth limit exceeded")]
    DepthLimitExceeded,

    #[error("Invalid graph")]
    InvalidGraph(DecisionGraphValidationError),

    #[error("Validation failed")]
    Validation(Value),

    #[error("policy evaluation error: {0}")]
    Policy(crate::policy::EvaluationError),

    #[error("expected {expected} content for key '{key}', got {got}")]
    ContentKindMismatch {
        expected: &'static str,
        got: &'static str,
        key: Arc<str>,
    },
}

impl EvaluationError {
    pub fn serialize_with_mode<S>(
        &self,
        serializer: S,
        mode: EvaluationTraceKind,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(None)?;

        match self {
            EvaluationError::DepthLimitExceeded => {
                map.serialize_entry("type", "DepthLimitExceeded")?;
            }
            EvaluationError::NodeError {
                node_id,
                trace,
                source,
            } => {
                map.serialize_entry("type", "NodeError")?;
                map.serialize_entry("source", &source.to_string())?;
                map.serialize_entry("nodeId", &node_id)?;

                if let Some(trace) = &trace {
                    map.serialize_entry("trace", &mode.serialize_trace(trace))?;
                }
            }
            EvaluationError::LoaderError(err) => {
                map.serialize_entry("type", "LoaderError")?;
                match err {
                    LoaderError::Internal { key, source } => {
                        map.serialize_entry("key", key)?;
                        map.serialize_entry("source", &source.to_string())?;
                    }
                    LoaderError::NotFound(key) => {
                        map.serialize_entry("key", key)?;
                    }
                }
            }
            EvaluationError::InvalidGraph(err) => {
                map.serialize_entry("type", "InvalidGraph")?;
                map.serialize_entry("source", err)?;
            }
            EvaluationError::Validation(err) => {
                map.serialize_entry("type", "Validation")?;
                map.serialize_entry("source", err)?;
            }
            EvaluationError::Policy(err) => {
                map.serialize_entry("type", "PolicyError")?;
                err.serialize_into_map(&mut map)?;
            }
            EvaluationError::ContentKindMismatch { expected, got, key } => {
                map.serialize_entry("type", "ContentKindMismatch")?;
                map.serialize_entry("expected", expected)?;
                map.serialize_entry("got", got)?;
                map.serialize_entry("key", key)?;
            }
        }

        map.end()
    }
}

impl Serialize for EvaluationError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.serialize_with_mode(serializer, Default::default())
    }
}

impl From<LoaderError> for Box<EvaluationError> {
    fn from(error: LoaderError) -> Self {
        Box::new(EvaluationError::LoaderError(error.into()))
    }
}

impl From<DecisionGraphValidationError> for Box<EvaluationError> {
    fn from(error: DecisionGraphValidationError) -> Self {
        Box::new(EvaluationError::InvalidGraph(error.into()))
    }
}

impl From<crate::policy::EvaluationError> for Box<EvaluationError> {
    fn from(error: crate::policy::EvaluationError) -> Self {
        Box::new(EvaluationError::Policy(error))
    }
}
