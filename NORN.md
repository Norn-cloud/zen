# Norn fork of GoRules ZEN

This is Norn's downstream fork of [gorules/zen](https://github.com/gorules/zen).
Norn uses the ZEN graph evaluator and ZEN Expression language inside its pure
decision kernel. That kernel has to run natively (celld) and as
`wasm32-unknown-unknown` inside Cloudflare Workers / Durable Objects, with no
JavaScript runtime, no tokio and no network client.

The fork carries a short, reviewable patch series on top of an immutable upstream
release tag. **Upstream default behaviour is preserved**: a default-features build
of this fork behaves like upstream. The Norn profile is opt-in
(`default-features = false`).

We do not open PRs against gorules/zen. Upstream only accepts outside contributions
for docs and tests. If upstream later adopts equivalent optional-runtime support, we
drop the matching patch.

## Upstream base

| | |
| --- | --- |
| Upstream tag | `zen-engine-v2.0.1` (same commit as `zen-expression-v2.0.1`) |
| Upstream commit | `6c0fbabba1266afffc2ac3cb24f20f2be7790e38` (2026-08-22) |
| Fork branch | `norn/v2.0.1` (default branch of Norn-cloud/zen) |
| Crate versions | zen-engine / zen-expression / zen-tmpl / zen-types 2.0.1 |

Consumers pin a `norn-v2.0.1-N` **tag** (or its commit), never the moving branch.

## Patch list

### Compatibility patches (no semantic change to default builds)

| # | Patch | Rationale |
| --- | --- | --- |
| C1 | `zen-engine`: optional `js` feature (default on) gates `rquickjs`, `tokio` and `swc_ts_fast_strip`, plus function-node v1/v2 execution. Without it a function node fails with the typed `zen_engine::UnsupportedNodeError { node_kind: "functionNode", feature: "js" }`, delivered as the `source` of `EvaluationError::NodeError`. | The pure kernel cannot embed QuickJS. On wasm32-unknown-unknown QuickJS also needs `bindgen`, which needs libclang. |
| C2 | `zen-engine`: optional `http` feature (default on, implies `js`) gates the native reqwest/reqsign/sha2 HTTP backend used by the function-node `http` module. The `HttpHandler` trait and `with_http_handler` stay available in all builds. | No ambient network client in the pure profile. |
| C3 | `zen-engine`: graph, walker and policy timing use `crate::time::Instant`, which is `web_time::Instant` on `wasm32-unknown-unknown` and `std::time::Instant` everywhere else. | `std::time::Instant::now()` panics on wasm32-unknown-unknown. |
| C4 | Default-feature propagation: `zen-engine` and `zen-tmpl` depend on `zen-expression` with `default-features = false` and forward `regex-deprecated` (default) / `regex-lite` explicitly. `zen-expression` now builds with neither regex feature by falling back to `regex-lite`, which is now a non-optional dependency. Precedence is unchanged when features are set: `regex-lite` wins, then `regex-deprecated` selects `regex`. | Before this, zen-tmpl silently re-enabled `regex-deprecated`, and `zen-expression` with defaults off did not compile. |
| C5 | `zen-engine`: the tokio/criterion dev-dependencies are now native-only. | Keeps `cargo tree --target wasm32-unknown-unknown` free of tokio. The multi-thread test runtime cannot run on wasm anyway. |
| C6 | `zen-engine`: jsonschema's default HTTP/file `$ref` resolvers (reqwest, tokio, rustls) are gated behind the new `schema-resolvers` feature (default on). The feature enables the native-only helper crate `core/schema-resolvers` (`zen-schema-resolvers`), whose only job is to depend on jsonschema with default features; cargo feature unification does the rest, so wasm targets keep upstream's `default-features = false`. Without it an external `$ref` fails schema compilation with the typed `zen_engine::SchemaCompileError { unresolved_reference: true, .. }`, delivered as the `source` of `EvaluationError::NodeError`. Default builds keep the jsonschema message text. | Before this the native pure tree still contained jsonschema → reqwest → tokio, and a schema `$ref` could fetch remotely. (norn-v2.0.1-2) |

### Semantic patches

| # | Patch | Rationale |
| --- | --- | --- |
| S1 | `median([])` returns an `"Empty array"` error, the same as `avg([])` and `mode([])`. Upstream computes `center - 1` on an empty array, which panics in debug builds and wraps in release. | Panic fix. It is observable only for input that used to panic or fail. |
| S2 | Feature `strict-errors` (default **off**): a decision-table input cell that fails to evaluate or is not a boolean, a failing output cell of a matching row, and a switch condition that fails or is not a boolean abort evaluation with the typed `zen_engine::StrictEvaluationError { site, id, expression, message }` (`site`: `DecisionTableInput` / `DecisionTableOutput` / `SwitchCondition`), delivered as the `source` of `EvaluationError::NodeError`. Upstream (feature off) turns these into a non-match, a dropped row result, or a false condition. Applies to both hit policies, traced and untraced evaluation. Strict evaluation does not use the decision-table index, because the index would prune a failing cell without evaluating it (-4). A sub-decision's error stays in the source chain (the decision node's `source` is the child `EvaluationError`), so callers can downcast through it to `StrictEvaluationError` (-4; applies with the feature off too, and the message text is unchanged). Four upstream fixture tests that depend on the fallback are ignored when the feature is on. | Review F4: a failing deny row must not yield a fallback result, and a failing switch branch must not fall through. (norn-v2.0.1-3) |
| S3 | Feature `deterministic-maps` (default **off**, on `zen-types`; forwarded by `zen-expression` and `zen-engine`): above the 32-key small-map threshold `VariableMap` uses an insertion-ordered `indexmap::IndexMap` (removal is order-preserving `shift_remove`) instead of an ahash `HashMap`. Iteration order is insertion order at every size, so `keys`, `values`, serialization and every other walk are independent of hash seeds. The engine's `$nodes` object is built in graph node order. | Review F1: eight equivalent 40-key inputs produced eight `keys` orders. (norn-v2.0.1-3) |
| S4 | Feature `metering` (default **off**; on `zen-expression`, forwarded by `zen-engine`): a shared `Meter` (limit + counter) charged by every executed VM opcode (so every closure iteration of `map`/`filter`/`flatMap`/...), by data-proportional builtins and opcodes, and by zen-engine graph node visits, decision-table rows and transform-attributes loop elements. Running out fails with the typed `BudgetExhausted { limit, used }`: `VMError::BudgetExhausted` in an `Isolate`, `EvaluationError::BudgetExhausted` from `Decision::evaluate_metered`. The meter is sticky, and the graph evaluator checks it after every node, so an exhaustion swallowed inside a node (for example a non-strict table cell) still aborts the evaluation. See "Metering cost model". | Review F3: poll/depth/reset counts do not bound work. (norn-v2.0.1-3) |

The semantic patches S2 to S4 are **off by default**. The Norn profile enables them
explicitly, for example
`zen-engine = { ..., default-features = false, features = ["strict-errors", "deterministic-maps", "metering"] }`.
Banning or pinning ambient time and randomness builtins (`rand`, `now`, default
timezone) is **not** a fork patch. Per D32 it belongs to Norn's own checker/admission
(`norn-expr` / `norn-decisions`).

Known gaps, outside this series:
- Diagnostic type-union strings in `functions/defs.rs` are built from std `HashSet`,
  so their order varies. They are diagnostics, not decision output.
- Trace maps (`HashMap<node id, trace>`) serialize in hash order. They are
  informational only.
- `GraphWalker::ITER_MAX` (1000 switch resets) still ends a walk silently. Metering
  bounds the work that leads up to it.

## Releases (tags)

Every tag is immutable and sits on `norn/v2.0.1`. `git log zen-engine-v2.0.1..<tag>`
lists the full downstream delta.

| Tag | Commit | Adds |
| --- | --- | --- |
| `norn-v2.0.1-1` | `6ac6817e799a55e8c4e43ef26784088c45d9e3d8` | Compatibility series C1 to C5, S1 (`median([])`), Norn CI, this file |
| `norn-v2.0.1-2` | `1e6c6bd2ec36df15cc31680bda513dcb4cce6d5c` | C6 (`schema-resolvers`, typed `SchemaCompileError`), native pure-tree CI check, regex-backend difference tests plus the pure-regex CI job (review fixes on -1) |
| `norn-v2.0.1-3` | `526980cc0c8a6f8eb2f59f46f1eee763740fdf53` | Semantic series S2 `strict-errors` (#2), S3 `deterministic-maps` (#3), S4 `metering` (#4), all default off, with CI covering them on and off and on wasm32-unknown-unknown (T3-ZEN-1, Norn-cloud/norn-platform#2768) |
| `norn-v2.0.1-4` | tag of the merge commit of Norn-cloud/zen#5 | Fixes from the gpt-6-sol review of -1..-3: strict evaluation skips the table index; transform loops are metered; `flatten`/`merge`/deep builtins are charged by the data they traverse (bounded walk); sub-decision errors stay in the source chain; CI builds the zen-engine Norn profile for wasm32 |

## Feature matrix (`zen-engine`)

| Feature | Default | Gates |
| --- | --- | --- |
| `js` | on | QuickJS function nodes (`rquickjs`, `tokio` OnceCell, TypeScript stripping via `swc_ts_fast_strip`) |
| `http` | on | Native HTTP backend for function nodes (`reqwest`, `reqsign`, `sha2`, `http`, `async-trait`). Implies `js`. Non-wasm only. |
| `schema-resolvers` | on | jsonschema's HTTP + file `$ref` retrieval (`reqwest`, `tokio`, `rustls`). Non-wasm only. |
| `regex-deprecated` | on | `regex` backend for zen-expression / zen-tmpl (upstream default) |
| `regex-lite` | off | `regex-lite` backend. Takes precedence when enabled. |
| `bindgen` | off | `rquickjs/bindgen`. Implies `js` and needs libclang. |
| `arbitrary_precision` | off | unchanged from upstream |
| `strict-errors` | off | Norn semantic patch S2: typed abort on failing table cells / switch conditions |
| `deterministic-maps` | off | Norn semantic patch S3: insertion-ordered `VariableMap` at every size (`indexmap`). Also on `zen-expression` and `zen-types`. |
| `metering` | off | Norn semantic patch S4: deterministic operation budget (`zen_engine::meter::Meter`, `Decision::evaluate_metered`). Also on `zen-expression` (`Isolate::set_meter`). |

The Norn pure profile is `zen-engine = { ..., default-features = false }`. Its
regex backend is `regex-lite`, either by fallback or by setting `regex-lite`
explicitly. The regex backend choice is part of the Norn language profile.
Choose it deliberately and pin it at the consumer.

### Open profile decision: regex backend (Norn must pin; not decided here)

`regex` and `regex-lite` accept different languages and give different answers on
non-ASCII input. `core/expression/tests/regex_backends.rs` pins both columns:

| Construct | `regex` (upstream default) | `regex-lite` (pure fallback) |
| --- | --- | --- |
| `\d`, `\s`, `\w` | Unicode classes (`\w` matches `é`, `\d` matches `٣`, `\s` matches NBSP) | ASCII only |
| `\b`, `\B` | Unicode word boundary | ASCII word boundary |
| `\p{..}`, `\P{..}` | Unicode properties | Do not compile: `matches()` / `extract()` return an error |
| `(?i)` | Simple Unicode case folding (`é` ~ `É`) | ASCII case folding only |
| `.`, literals, `[[:alpha:]]`, anchors, alternation, lazy quantifiers | Same | Same |
| Worst case | Linear time | `O(m * n)`, no DFA/literal optimizations |

Rules written against one backend can silently change result under the other. The
Norn side must record which backend its decision profile uses (and therefore which
zen-engine features the kernel enables), and treat a switch as a semantic change.
CI runs the expression integration tests under both backends.

Cargo unifies features across the whole dependency graph. Any crate in the
consumer's graph that enables `zen-engine/default`, `js` or `http` brings these
runtimes back. Run the `cargo tree` check below on the **integrated consumer**,
not only on this crate.

### Timing is informational only

`DecisionGraphResponse::performance` and the per-node trace timings come from a
monotonic clock. On Cloudflare Workers, `performance.now()` (behind `web_time`)
only advances across I/O, not during synchronous code, so timings measured inside
the pure evaluator read as roughly zero. Never use these values for deadlines,
budgets or metering. Use host CPU telemetry and deterministic fuel instead.

## Metering cost model (`metering`)

Units depend only on the decision and its input values, never on time, hash seeds,
tracing, precompilation or the target. Constants live in `zen_expression::meter::cost`.

| Charge | Units |
| --- | --- |
| Every executed VM opcode | 1 |
| `CallFunction` / `CallMethod` | + shallow size of the arguments (string bytes, array/object length, else 1) |
| `flatten`, `merge`, `Flatten` opcode | + outer length + shallow size of every direct child (`cost::nested_size`) instead of the shallow size (-4) |
| `matches`, `extract` | + 64 (regex compilation) on top of the argument sizes |
| `fuzzyMatch` | + deep size(subject) x size(pattern) |
| `mergeDeep`, `Join` | + recursive size (`cost::deep_size_capped`); the walk stops once it exceeds the meter's remaining units, so measuring is itself bounded (-4) |
| `Slice`, `In`, `Equal`, string `Add` | + size of the data they touch |
| Loop `Begin` (closures) | + array length; an interval `[a..b]` is charged for its length **before** it is materialized |
| Graph node visit (engine) | 1 |
| Decision-table row considered (engine) | 1 |
| Transform-attributes loop (engine) | 1 per input array element, charged up front; the loop stops once the meter is exhausted (-4) |

Determinism rules:
- Charges are computed before an opcode runs, so exhaustion happens before the work.
- A metered decision-table evaluation does not use the table index. Index pruning
  skips cells, and whether it applies depends on tracing and `compile()`. Without the
  index, every row is evaluated in order.
- Trace-only work (the row reference map in traced tables) is not charged. Total cost
  is identical with and without `trace` and with and without `compile()` (tested).
- Parsing/compiling expression source is not charged. It depends on the opcode cache
  and is bounded at admission.
- Not metered: `Expression::evaluate*` (a standalone VM; use an `Isolate`), policy
  documents, function/custom nodes, schema validation, and memory. The budget bounds
  work, not allocation size.

Pinned counts (`core/expression/tests/metering.rs`, run natively and on
`wasm32-unknown-unknown` in CI): `flatMap([0..19], map([0..19], # * 2))` costs **3469**
units (3449 before -4 charged `Flatten` by its children); with a budget of 500 it fails at `BudgetExhausted { limit: 500, used: 502 }` on
both targets.

## CI (`.github/workflows/norn.yaml`, pinned toolchain)

1. Upstream test suite, native, default features (upstream's binding exclusions).
2. Pure profile: native `--lib` tests with `--no-default-features`, zen-expression
   integration tests with `--no-default-features` (regex-lite backend), then
   `cargo build -p zen-engine --no-default-features --target wasm32-unknown-unknown --release`
   (no libclang needed).
3. `cargo tree -e normal -p zen-engine --no-default-features` (native host) must not
   contain `reqwest`, `tokio` or `rquickjs`, and
   `cargo tree -e features --target wasm32-unknown-unknown -p zen-engine --no-default-features`
   must not mention `tokio` or `rquickjs`. Positive controls check that the default
   trees do, so the negative checks cannot pass vacuously.
4. Semantic features on (`strict-errors`, `deterministic-maps`, `metering`): the
   upstream suite plus Norn tests with default features, and the Norn tests in the pure
   profile. Upstream fixture tests that rely on the failing-cell fallback are ignored
   only under `strict-errors`.
5. Norn profile on `wasm32-unknown-unknown`: `zen-engine` builds (release) with
   `--no-default-features --features strict-errors,deterministic-maps,metering`, and that
   tree has no tokio, rquickjs or reqwest. `core/expression/tests/metering.rs` runs under
   `wasm-bindgen-test-runner` (CLI version read from `Cargo.lock`) and must hit the
   same pinned counts as the native run. zen-expression's `criterion` dev-dependency
   is native-only for this, and the wasm test build adds `wasm-bindgen-test` and
   `getrandom/wasm_js` (ahash's RNG; zen-engine already gets the same feature through
   jsonschema).

## Rebase / update policy

- Base only on **immutable upstream release tags**. Never track a moving upstream
  branch, and never force-push over upstream history.
- Each fork branch is named after its base, e.g. `norn/v2.0.1`. Updating to a new
  upstream release means a new branch (`norn/vX.Y.Z`) with the patch series
  replayed and CI green. Old branches and tags stay.
- Every change consumers can pick up gets a new immutable tag `norn-vX.Y.Z-N`, with
  N incremented per release on the same base (`norn-v2.0.1-1`, `norn-v2.0.1-2`, …).
  Nothing is updated silently.
- Review upstream releases and advisories monthly. Apply security fixes promptly,
  out of cycle.
- A new upstream major version, or any semantic patch, requires a fresh
  qualification decision on the Norn side before consumers move.
- Keep compatibility patches and semantic patches in separate commits so either
  set can be dropped or upstreamed on its own.

## License and provenance

Upstream is MIT, © GoRules.io. See `LICENSE`, which this fork keeps unchanged.
Norn modifications are also MIT. Every Norn change is a separate commit on top of
the upstream tag, so `git log zen-engine-v2.0.1..` lists the full downstream delta.
