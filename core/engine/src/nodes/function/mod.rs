pub mod http_handler;
#[cfg(feature = "js")]
pub(crate) mod v1;
#[cfg(feature = "js")]
pub(crate) mod v2;

use crate::nodes::definition::NodeHandler;
#[cfg(feature = "js")]
use crate::nodes::function::v1::{FunctionV1NodeHandler, FunctionV1Trace};
#[cfg(feature = "js")]
use crate::nodes::function::v2::{FunctionV2NodeHandler, FunctionV2Trace};
use crate::nodes::result::NodeResult;
use crate::nodes::NodeContext;
#[cfg(feature = "js")]
use std::sync::Arc;
#[cfg(feature = "js")]
use zen_types::decision::FunctionContent;
use zen_types::decision::FunctionNodeContent;
use zen_types::variable::Variable;

#[derive(Debug, Clone)]
pub struct FunctionNodeHandler;

pub type FunctionNodeData = FunctionNodeContent;

pub type FunctionNodeTrace = Variable;

/// Norn: without the `js` feature there is no JavaScript runtime; function nodes fail
/// with a typed [`crate::UnsupportedNodeError`] instead of executing.
#[cfg(not(feature = "js"))]
impl NodeHandler for FunctionNodeHandler {
    type NodeData = FunctionNodeData;
    type TraceData = FunctionNodeTrace;

    async fn handle(&self, ctx: NodeContext<Self::NodeData, Self::TraceData>) -> NodeResult {
        ctx.error(crate::UnsupportedNodeError {
            node_kind: "functionNode",
            feature: "js",
        })
    }
}

#[cfg(feature = "js")]
impl NodeHandler for FunctionNodeHandler {
    type NodeData = FunctionNodeData;
    type TraceData = FunctionNodeTrace;

    async fn handle(&self, ctx: NodeContext<Self::NodeData, Self::TraceData>) -> NodeResult {
        match &ctx.node {
            FunctionNodeContent::Version1(source) => {
                let v1_context = NodeContext::<Arc<str>, FunctionV1Trace> {
                    id: ctx.id.clone(),
                    name: ctx.name.clone(),
                    input: ctx.input.clone(),
                    nodes: ctx.nodes.clone(),
                    extensions: ctx.extensions.clone(),
                    trace: ctx.config.trace.then(|| Default::default()),
                    iteration: ctx.iteration,
                    config: ctx.config,
                    node: source.clone(),
                };

                FunctionV1NodeHandler.handle(v1_context).await
            }
            FunctionNodeContent::Version2(content) => {
                let v2_context = NodeContext::<FunctionContent, FunctionV2Trace> {
                    id: ctx.id.clone(),
                    name: ctx.name.clone(),
                    input: ctx.input.clone(),
                    nodes: ctx.nodes.clone(),
                    extensions: ctx.extensions.clone(),
                    trace: ctx.config.trace.then(|| Default::default()),
                    iteration: ctx.iteration,
                    config: ctx.config,
                    node: content.clone(),
                };

                FunctionV2NodeHandler.handle(v2_context).await
            }
        }
    }
}

#[cfg(all(test, not(feature = "js")))]
mod tests {
    use crate::model::GraphContent;
    use crate::{Decision, EvaluationError, UnsupportedNodeError};
    use serde_json::json;

    #[tokio::test]
    async fn function_node_is_a_typed_unsupported_error_without_js() {
        let graph = json!({
            "nodes": [
                { "id": "in", "name": "in", "type": "inputNode" },
                { "id": "fn", "name": "fn", "type": "functionNode",
                  "content": "const handler = (input) => ({ output: input.input * 2 });" },
                { "id": "out", "name": "out", "type": "outputNode" }
            ],
            "edges": [
                { "id": "e1", "sourceId": "in", "targetId": "fn", "type": "edge" },
                { "id": "e2", "sourceId": "fn", "targetId": "out", "type": "edge" }
            ]
        });
        let content: GraphContent = serde_json::from_value(graph).unwrap();
        let decision = Decision::from(content);

        let err = decision
            .evaluate(json!({ "input": 2 }).into())
            .await
            .unwrap_err();
        let EvaluationError::NodeError {
            node_id, source, ..
        } = *err
        else {
            panic!("expected NodeError, got {err:?}");
        };
        assert_eq!(&*node_id, "fn");
        let unsupported = source
            .downcast_ref::<UnsupportedNodeError>()
            .expect("source should be UnsupportedNodeError");
        assert_eq!(
            unsupported,
            &UnsupportedNodeError {
                node_kind: "functionNode",
                feature: "js",
            }
        );
    }
}
