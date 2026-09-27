//! Norn `metering` feature: a deterministic operation budget.
//!
//! A [`Meter`] is a shared counter of abstract work units with a fixed limit. The
//! expression VM charges it for every executed opcode and for data-proportional work
//! (string, collection and regex builtins), and zen-engine charges graph node and
//! decision-table row visits. Units depend only on the program and its input values,
//! never on time, hash seeds, caching or the target, so the same evaluation exhausts
//! the same budget at the same count natively and on `wasm32-unknown-unknown`.
//!
//! Once a charge exceeds the limit the meter stays exhausted: every later charge fails
//! with the same [`BudgetExhausted`] value, even if an intermediate caller swallowed
//! the first error.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;

use crate::variable::Variable;

/// Returned when a charge would take a [`Meter`] past its limit.
///
/// `used` is the total that the failing charge would have reached (so `used > limit`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("operation budget exhausted: {used} units needed, limit {limit}")]
pub struct BudgetExhausted {
    pub limit: u64,
    pub used: u64,
}

/// Shared, cloneable operation budget. Clones charge the same counter.
#[derive(Debug, Clone)]
pub struct Meter(Arc<Inner>);

#[derive(Debug)]
struct Inner {
    limit: u64,
    used: AtomicU64,
    /// `used` value of the first failing charge; 0 while not exhausted.
    exhausted_at: AtomicU64,
}

impl Meter {
    pub fn new(limit: u64) -> Self {
        Self(Arc::new(Inner {
            limit,
            used: AtomicU64::new(0),
            exhausted_at: AtomicU64::new(0),
        }))
    }

    pub fn limit(&self) -> u64 {
        self.0.limit
    }

    /// Units still available before the limit.
    pub fn remaining(&self) -> u64 {
        self.0.limit.saturating_sub(self.used())
    }

    /// Units charged so far (not counting a failing charge).
    pub fn used(&self) -> u64 {
        self.0.used.load(Ordering::Relaxed)
    }

    /// The first exhaustion, if the budget has been exceeded.
    pub fn exhausted(&self) -> Option<BudgetExhausted> {
        match self.0.exhausted_at.load(Ordering::Relaxed) {
            0 => None,
            used => Some(BudgetExhausted {
                limit: self.0.limit,
                used,
            }),
        }
    }

    /// Adds `units`, or fails without adding them if that would exceed the limit.
    pub fn charge(&self, units: u64) -> Result<(), BudgetExhausted> {
        if let Some(exhausted) = self.exhausted() {
            return Err(exhausted);
        }

        let used = self.used().saturating_add(units);
        if used > self.0.limit {
            self.0.exhausted_at.store(used, Ordering::Relaxed);
            return Err(BudgetExhausted {
                limit: self.0.limit,
                used,
            });
        }

        self.0.used.store(used, Ordering::Relaxed);
        Ok(())
    }
}

/// Cost model. Every executed opcode costs [`cost::OPCODE`]; the constants below are
/// charged on top of it. Sizes are shallow: string length in bytes, array or object
/// length in elements, 1 for anything else.
pub mod cost {
    use super::Variable;

    /// Every executed VM opcode (closure bodies are opcodes, so every iteration of
    /// `map`, `filter`, `flatMap`, ... is charged).
    pub const OPCODE: u64 = 1;
    /// Compiling a regular expression (`matches`, `extract`), plus the pattern length.
    pub const REGEX_COMPILE: u64 = 64;
    /// One zen-engine graph node visit.
    pub const NODE_VISIT: u64 = 1;
    /// One zen-engine decision-table row considered.
    pub const TABLE_ROW: u64 = 1;
    /// One element of a zen-engine transform-attributes loop, charged up front for the
    /// whole input array.
    pub const TRANSFORM_ITEM: u64 = 1;

    /// Shallow size of a value, used by data-proportional charges.
    pub fn size(value: &Variable) -> u64 {
        match value {
            Variable::String(s) => s.len() as u64,
            Variable::Array(a) => a.borrow().len() as u64,
            Variable::Object(o) => o.borrow().len() as u64,
            _ => 1,
        }
    }

    /// A container's length plus the shallow size of each direct child: the data that
    /// one-level builtins (`flatten`, `merge`, the flatten opcode) traverse.
    pub fn nested_size(value: &Variable) -> u64 {
        let children = |acc: u64, v: &Variable| acc.saturating_add(size(v));
        match value {
            Variable::Array(a) => {
                let a = a.borrow();
                a.iter().fold(a.len() as u64, children)
            }
            Variable::Object(o) => {
                let o = o.borrow();
                o.iter()
                    .fold(o.len() as u64, |acc, (_, v)| children(acc, v))
            }
            other => size(other),
        }
    }

    /// Recursive size, for builtins whose work is proportional to all nested data
    /// (`mergeDeep`, `fuzzyMatch`, join). The walk stops once the total exceeds `cap`
    /// (the caller passes the meter's remaining units), so measuring is itself bounded
    /// by the budget; any result above `cap` exhausts the meter the same way.
    pub fn deep_size_capped(value: &Variable, cap: u64) -> u64 {
        fn walk(value: &Variable, acc: &mut u64, cap: u64) {
            if *acc > cap {
                return;
            }
            match value {
                Variable::Array(a) => {
                    *acc = acc.saturating_add(1);
                    for v in a.borrow().iter() {
                        walk(v, acc, cap);
                        if *acc > cap {
                            return;
                        }
                    }
                }
                Variable::Object(o) => {
                    *acc = acc.saturating_add(1);
                    for (_, v) in o.borrow().iter() {
                        walk(v, acc, cap);
                        if *acc > cap {
                            return;
                        }
                    }
                }
                other => *acc = acc.saturating_add(size(other)),
            }
        }
        let mut acc = 0;
        walk(value, &mut acc, cap);
        acc.min(cap.saturating_add(1))
    }
}
