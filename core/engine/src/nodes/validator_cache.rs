#[cfg(feature = "json-schema")]
use crate::nodes::variable_json::VariableJson;
#[cfg(feature = "json-schema")]
use ahash::HashMap;
#[cfg(feature = "json-schema")]
use anyhow::Context;
#[cfg(feature = "json-schema")]
use jsonschema::Validator;
#[cfg(feature = "json-schema")]
use serde_json::Value;
#[cfg(feature = "json-schema")]
use std::sync::{Arc, RwLock};

#[cfg(feature = "json-schema")]
#[derive(Clone, Default, Debug)]
pub struct ValidatorCache {
    inner: Arc<RwLock<HashMap<u64, Arc<Validator<VariableJson>>>>>,
}

/// Norn (C8): without `json-schema` there are no validators to cache; node schemas
/// fail closed in `NodeContext::validate`.
#[cfg(not(feature = "json-schema"))]
#[derive(Clone, Default, Debug)]
pub struct ValidatorCache;

impl PartialEq for ValidatorCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

#[cfg(feature = "json-schema")]
impl ValidatorCache {
    pub fn get(&self, key: u64) -> Option<Arc<Validator<VariableJson>>> {
        let read = self.inner.read().ok()?;
        read.get(&key).cloned()
    }

    pub fn get_or_insert(
        &self,
        key: u64,
        schema: &Value,
    ) -> anyhow::Result<Arc<Validator<VariableJson>>> {
        if let Some(v) = self.get(key) {
            return Ok(v);
        }

        let mut w_shared = self
            .inner
            .write()
            .ok()
            .context("Failed to acquire lock on validator cache")?;
        let validator = Arc::new(
            jsonschema::options_for::<VariableJson>()
                .with_draft(jsonschema::Draft::Draft7)
                .build(schema)
                .map_err(|err| crate::SchemaCompileError {
                    unresolved_reference: matches!(
                        err.kind(),
                        jsonschema::error::ValidationErrorKind::Referencing(_)
                    ),
                    message: err.to_string(),
                })?,
        );
        w_shared.insert(key, validator.clone());

        Ok(validator)
    }
}

#[cfg(all(test, feature = "json-schema", not(feature = "schema-resolvers")))]
mod tests {
    use super::ValidatorCache;
    use crate::model::GraphContent;
    use crate::{Decision, EvaluationError, SchemaCompileError};
    use serde_json::json;

    fn assert_unresolved(reference: &str) {
        let schema = json!({ "$ref": reference });
        let err = ValidatorCache::default()
            .get_or_insert(1, &schema)
            .expect_err("external $ref must not compile without schema-resolvers");
        let typed = err
            .downcast_ref::<SchemaCompileError>()
            .expect("error should be SchemaCompileError");
        assert!(typed.unresolved_reference, "{typed:?}");
    }

    #[test]
    fn remote_ref_is_a_typed_error_without_resolvers() {
        assert_unresolved("https://example.com/schema.json");
    }

    #[test]
    fn file_ref_is_a_typed_error_without_resolvers() {
        assert_unresolved("file:///etc/norn-does-not-exist.json");
    }

    #[tokio::test]
    async fn input_node_with_remote_ref_fails_evaluation() {
        let schema = json!({ "$ref": "https://example.com/schema.json" }).to_string();
        let graph = json!({
            "nodes": [
                { "id": "in", "name": "in", "type": "inputNode", "content": { "schema": schema } },
                { "id": "out", "name": "out", "type": "outputNode" }
            ],
            "edges": [{ "id": "e1", "sourceId": "in", "targetId": "out", "type": "edge" }]
        });
        let content: GraphContent = serde_json::from_value(graph).unwrap();
        let err = Decision::from(content)
            .evaluate(json!({ "a": 1 }).into())
            .await
            .unwrap_err();
        let EvaluationError::NodeError { source, .. } = *err else {
            panic!("expected NodeError, got {err:?}");
        };
        let typed = source
            .downcast_ref::<SchemaCompileError>()
            .expect("source should be SchemaCompileError");
        assert!(typed.unresolved_reference);
    }
}

/// Norn (C8): a node schema without the `json-schema` feature is a typed error, never
/// skipped validation.
#[cfg(all(test, not(feature = "json-schema")))]
mod tests_without_json_schema {
    use crate::model::GraphContent;
    use crate::{Decision, EvaluationError, SchemaCompileError};
    use serde_json::json;

    #[tokio::test]
    async fn node_schema_fails_closed_without_json_schema() {
        for node_type in ["inputNode", "outputNode"] {
            let schema = json!({ "type": "object" }).to_string();
            let (input, output) = if node_type == "inputNode" {
                (json!({ "schema": schema }), json!({}))
            } else {
                (json!({}), json!({ "schema": schema }))
            };
            let graph = json!({
                "nodes": [
                    { "id": "in", "name": "in", "type": "inputNode", "content": input },
                    { "id": "out", "name": "out", "type": "outputNode", "content": output }
                ],
                "edges": [{ "id": "e1", "sourceId": "in", "targetId": "out", "type": "edge" }]
            });
            let content: GraphContent = serde_json::from_value(graph).unwrap();
            let err = Decision::from(content)
                .evaluate(json!({ "a": 1 }).into())
                .await
                .unwrap_err();
            let EvaluationError::NodeError { source, .. } = *err else {
                panic!("{node_type}: expected NodeError, got {err:?}");
            };
            let typed = source
                .downcast_ref::<SchemaCompileError>()
                .expect("source should be SchemaCompileError");
            assert!(!typed.unresolved_reference, "{node_type}: {typed:?}");
        }
    }

    fn assert_schema_error(err: &EvaluationError) {
        let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(err);
        while let Some(current) = cause {
            if let Some(typed) = current.downcast_ref::<SchemaCompileError>() {
                assert!(!typed.unresolved_reference, "{typed:?}");
                return;
            }
            cause = current.source();
        }
        panic!("no SchemaCompileError in the source chain of {err:?}");
    }

    #[tokio::test]
    async fn dictionary_schema_fails_closed_without_loading() {
        let schema = json!({ "type": "object", "properties": { "a": { "$dictionary": "missing" } } })
            .to_string();
        for node in ["inputNode", "outputNode"] {
            let content = json!({ "schema": schema });
            let (input, output) = if node == "inputNode" {
                (content, json!({}))
            } else {
                (json!({}), content)
            };
            let graph = json!({
                "nodes": [
                    { "id": "in", "name": "in", "type": "inputNode", "content": input },
                    { "id": "out", "name": "out", "type": "outputNode", "content": output }
                ],
                "edges": [{ "id": "e1", "sourceId": "in", "targetId": "out", "type": "edge" }],
                "imports": ["missing-policy"]
            });
            let content: GraphContent = serde_json::from_value(graph).unwrap();
            let err = Decision::from(content)
                .evaluate(json!({ "a": 1 }).into())
                .await
                .unwrap_err();
            assert_schema_error(&err);
        }
    }

    #[tokio::test]
    async fn sub_decision_schema_fails_closed() {
        use crate::loader::MemoryLoader;
        use std::sync::Arc;

        let schema = json!({ "type": "object" }).to_string();
        let sub = json!({
            "nodes": [
                { "id": "in", "name": "in", "type": "inputNode" },
                { "id": "out", "name": "out", "type": "outputNode", "content": { "schema": schema } }
            ],
            "edges": [{ "id": "e1", "sourceId": "in", "targetId": "out", "type": "edge" }]
        });
        let root = json!({
            "nodes": [
                { "id": "in", "name": "in", "type": "inputNode" },
                { "id": "call", "name": "call", "type": "decisionNode", "content": { "key": "sub" } },
                { "id": "out", "name": "out", "type": "outputNode" }
            ],
            "edges": [
                { "id": "e1", "sourceId": "in", "targetId": "call", "type": "edge" },
                { "id": "e2", "sourceId": "call", "targetId": "out", "type": "edge" }
            ]
        });
        let loader = MemoryLoader::default();
        loader.add("sub", serde_json::from_value::<GraphContent>(sub).unwrap());
        let content: GraphContent = serde_json::from_value(root).unwrap();
        let err = Decision::from(content)
            .with_loader(Arc::new(loader))
            .evaluate(json!({ "a": 1 }).into())
            .await
            .unwrap_err();
        assert_schema_error(&err);
    }

    #[tokio::test]
    async fn graph_without_schema_still_evaluates() {
        let graph = json!({
            "nodes": [
                { "id": "in", "name": "in", "type": "inputNode" },
                { "id": "out", "name": "out", "type": "outputNode" }
            ],
            "edges": [{ "id": "e1", "sourceId": "in", "targetId": "out", "type": "edge" }]
        });
        let content: GraphContent = serde_json::from_value(graph).unwrap();
        let response = Decision::from(content)
            .evaluate(json!({ "a": 1 }).into())
            .await
            .unwrap();
        assert_eq!(response.result.to_value(), json!({ "a": 1 }));
    }
}
