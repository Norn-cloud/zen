use crate::nodes::variable_json::VariableJson;
use ahash::HashMap;
use anyhow::Context;
use jsonschema::Validator;
use serde_json::Value;
use std::sync::{Arc, RwLock};

#[derive(Clone, Default, Debug)]
pub struct ValidatorCache {
    inner: Arc<RwLock<HashMap<u64, Arc<Validator<VariableJson>>>>>,
}

impl PartialEq for ValidatorCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

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

#[cfg(all(test, not(feature = "schema-resolvers")))]
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
