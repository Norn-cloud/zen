//! Norn `metering`: the operation budget is deterministic.
//!
//! These tests pin exact unit counts. CI runs them natively and on
//! `wasm32-unknown-unknown` (wasm-bindgen-test); both must hit the same numbers.
#![cfg(feature = "metering")]

use serde_json::json;
use zen_expression::meter::{BudgetExhausted, Meter};
use zen_expression::vm::VMError;
use zen_expression::{Isolate, IsolateError};

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use wasm_bindgen_test::wasm_bindgen_test as test;

/// Nested closures: 20 outer iterations, each mapping 20 inner elements.
const NESTED: &str = "flatMap([0..19], map([0..19], # * 2))";
/// Units a full evaluation of `NESTED` costs.
const NESTED_FULL: u64 = 3449;
/// A budget smaller than `NESTED_FULL`, and the count at which it runs out.
const SMALL_BUDGET: u64 = 500;
const SMALL_EXHAUSTED_AT: u64 = 502;

fn isolate(meter: &Meter) -> Isolate {
    Isolate::with_environment(json!({ "s": "The quick brown fox" }).into())
        .with_meter(Some(meter.clone()))
}

fn exhausted(err: IsolateError) -> BudgetExhausted {
    match err {
        IsolateError::VMError {
            source: VMError::BudgetExhausted(exhausted),
        } => exhausted,
        other => panic!("expected BudgetExhausted, got {other:?}"),
    }
}

#[test]
fn nested_map_flat_map_full_cost_is_pinned() {
    let meter = Meter::new(u64::MAX);
    let result = isolate(&meter).run_standard(NESTED).unwrap();
    assert_eq!(result.as_array().unwrap().borrow().len(), 400);
    assert_eq!(meter.used(), NESTED_FULL);
    assert_eq!(meter.exhausted(), None);
}

#[test]
fn nested_map_flat_map_exhausts_small_budget_at_pinned_count() {
    let meter = Meter::new(SMALL_BUDGET);
    let err = isolate(&meter).run_standard(NESTED).unwrap_err();
    let expected = BudgetExhausted {
        limit: SMALL_BUDGET,
        used: SMALL_EXHAUSTED_AT,
    };
    assert_eq!(exhausted(err), expected);
    assert_eq!(meter.exhausted(), Some(expected));

    // Sticky: later work on the same meter fails with the first exhaustion.
    let err = isolate(&meter).run_standard("1 + 1").unwrap_err();
    assert_eq!(exhausted(err), expected);
}

#[test]
fn string_and_regex_builtins_are_charged_by_size() {
    let short = Meter::new(u64::MAX);
    isolate(&short)
        .run_standard(r#"matches("ab", "a+")"#)
        .unwrap();
    let long = Meter::new(u64::MAX);
    isolate(&long)
        .run_standard(r#"matches("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab", "a+")"#)
        .unwrap();
    // One opcode sequence, so the difference is exactly the extra input bytes.
    assert_eq!(long.used() - short.used(), 39);
    // Regex compilation carries a fixed surcharge over a plain string builtin.
    let plain = Meter::new(u64::MAX);
    isolate(&plain)
        .run_standard(r#"contains("ab", "a+")"#)
        .unwrap();
    assert_eq!(short.used() - plain.used(), 64);
}

#[test]
fn large_interval_is_charged_before_it_is_materialized() {
    let meter = Meter::new(1_000);
    let err = isolate(&meter)
        .run_standard("count([0..1000000000], # > 0)")
        .unwrap_err();
    assert!(exhausted(err).used > 1_000_000_000);
}
