mod analysis;
mod cursor;
mod dependencies;
mod editor;
mod enhance;
pub(crate) mod function;
mod nl;
mod queries;
mod schema;
#[cfg(feature = "ts-types")]
mod ts_type;
/// Norn (C9): without `ts-types` no TypeScript declaration is parsed; every declared
/// function type stays unresolved.
#[cfg(not(feature = "ts-types"))]
mod ts_type {
    use zen_expression::variable::VariableType;

    pub(crate) struct TsTypeParser;

    impl TsTypeParser {
        pub(crate) fn variable_type(_source: &str) -> Option<VariableType> {
            None
        }
    }
}

pub use analysis::{GraphAnalysis, GraphNodeAnalysis, GraphSignature};
pub use enhance::GraphTraceMap;
pub use function::{FunctionResolutionRequest, FunctionTypeResolver};
pub(crate) use schema::SchemaType;

use std::rc::Rc;
use std::sync::Arc;

use zen_expression::variable::VariableType;
use zen_types::decision::FunctionNodeContent;

pub(crate) fn function_source(content: &FunctionNodeContent) -> Arc<str> {
    match content {
        FunctionNodeContent::Version2(function) => function.source.clone(),
        FunctionNodeContent::Version1(source) => source.clone(),
    }
}

pub(crate) fn wrap_optional(resolved: VariableType) -> VariableType {
    if matches!(
        resolved,
        VariableType::Any | VariableType::Null | VariableType::Nullable(_)
    ) {
        resolved
    } else {
        VariableType::Nullable(Rc::new(resolved))
    }
}
