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
| C7 | `ahash` is a workspace dependency with `default-features = false` (`std`, `no-rng`). A new `runtime-rng` feature (default on) on `zen-types`, `zen-expression`, `zen-tmpl` and `zen-engine` re-enables `ahash/runtime-rng`, so default builds seed ahash maps from the OS RNG exactly as upstream. Without it ahash uses its fixed seeds. | Before this, `zen-expression` alone did not build for `wasm32-unknown-unknown`: ahash's default `runtime-rng` pulled getrandom 0.3, which needs a JS entropy import there (zen-engine only built because jsonschema enables getrandom `wasm_js`). With `deterministic-maps` no observable order depends on the seed. Maps keyed by untrusted input lose seed randomization in the pure profile, so Norn bounds input size and work at its own boundary. |
| C8 | `zen-engine`: jsonschema (input/output node `schema` validation, Draft 7) is optional behind the new `json-schema` feature (default on; `schema-resolvers` implies it). Without it a node whose `schema` decodes (root or sub-decision graph, input or output node) is never silently unvalidated: evaluation fails at that node with the typed `zen_engine::SchemaCompileError { unresolved_reference: false, .. }`, delivered as the `source` of `EvaluationError::NodeError`, and no schema dictionary or import is resolved or loaded. Caveat (upstream decoder, unchanged): `schema` text that is not valid JSON decodes to no schema in every build, so a consumer that must refuse node schemas checks the raw text at its own admission (Norn's `norn-decisions` refuses any non-blank node `schema`). The public `nodes::variable_json` module (the jsonschema adapter) exists only with the feature. | jsonschema enables getrandom `wasm_js` on wasm32, so the zen-engine Norn profile could not build without a JS entropy import (Norn's `zen-dependency-tree` gate forbids getrandom on wasm32). Norn validates decision input and output against its own closed RuleIR schemas with exact decimals and rejects node-level schemas at admission (T3-DEC-2). (norn-v2.0.1-7) |
| C9 | `zen-engine`: the swc TypeScript parser (`swc_common`, `swc_ecma_parser`, `swc_ecma_ast`) is optional behind the new `ts-types` feature (default on; `js` implies it). It parses the TypeScript type declarations of function nodes in workspace analysis. Without it a declared function type is not parsed and stays unresolved, exactly like a function without a declaration. C8 also moves `typed-arena` and `self_cell` (used only by the jsonschema adapter) behind `json-schema`. | The parser is large and unused by a profile that rejects function nodes, and `swc_ecma_parser` pulls `smartstring` (MPL-2.0), which Norn's license policy does not admit. (norn-v2.0.1-7) |
| C10 | UniFFI Android packaging CI pins `android-actions/setup-android` v4 at `be39fa834029ff78f1a44aa3bb0819b8fc2bd8fd`, with command-line tools 16.0 and `platform-tools`. | Upstream v2 always requests Google's removed SDK `tools` package and fails before AAR assembly. CI maintenance only; no crate or evaluation change. Proposed with `norn-v2.0.1-8`. |
| C11 | `zen-expression` parser: every top-level (precedence 0) expression parse is memoized per (token position, closure depth, closure context), together with the position it ended at, and a repeated request replays the recorded node and position. | Several productions parse speculatively and rewind: an interval is tried at `[` (twice: unary path and literal path) and at `(` before the array or group parse, and an assignment statement parses its key before it knows whether `=` follows. Each retry re-parsed the bracketed content, so parse time grew exponentially with nesting: a 60-byte input took more than 10 s, and `a = 1; b[a = 1; b[...]]` doubled per level with no range operator. Every bracket body goes through the memoized entry point, so parse time is now polynomial. Parse results and diagnostics are unchanged. Found by Norn's coverage-guided `zen_expression` fuzz target and the PR review (Norn-cloud/norn-platform#3243). |

### Semantic patches

| # | Patch | Rationale |
| --- | --- | --- |
| S1 | `median([])` returns an `"Empty array"` error, the same as `avg([])` and `mode([])`. Upstream computes `center - 1` on an empty array, which panics in debug builds and wraps in release. | Panic fix. It is observable only for input that used to panic or fail. |
| S2 | Feature `strict-errors` (default **off**): a decision-table input cell that fails to evaluate or is not a boolean, a failing output cell of a matching row, and a switch condition that fails or is not a boolean abort evaluation with the typed `zen_engine::StrictEvaluationError { site, id, expression, message }` (`site`: `DecisionTableInput` / `DecisionTableOutput` / `SwitchCondition`), delivered as the `source` of `EvaluationError::NodeError`. Upstream (feature off) turns these into a non-match, a dropped row result, or a false condition. Applies to both hit policies, traced and untraced evaluation. Strict evaluation does not use the decision-table index, because the index would prune a failing cell without evaluating it (-4). A sub-decision's error stays in the source chain (the decision node's `source` is the child `EvaluationError`), so callers can downcast through it to `StrictEvaluationError` (-4; applies with the feature off too, and the message text is unchanged). Four upstream fixture tests that depend on the fallback are ignored when the feature is on. | Review F4: a failing deny row must not yield a fallback result, and a failing switch branch must not fall through. (norn-v2.0.1-3) |
| S3 | Feature `deterministic-maps` (default **off**, on `zen-types`; forwarded by `zen-expression` and `zen-engine`): above the 32-key small-map threshold `VariableMap` uses an insertion-ordered `indexmap::IndexMap` (removal is order-preserving `shift_remove`) instead of an ahash `HashMap`. Iteration order is insertion order at every size, so `keys`, `values`, serialization and every other walk are independent of hash seeds. The engine's `$nodes` object is built in graph node order. | Review F1: eight equivalent 40-key inputs produced eight `keys` orders. (norn-v2.0.1-3) |
| S4 | Feature `metering` (default **off**; on `zen-expression`, forwarded by `zen-engine`): a shared `Meter` (limit + counter) charged by every executed VM opcode (so every closure iteration of `map`/`filter`/`flatMap`/...), by data-proportional builtins and opcodes, and by zen-engine graph node visits, decision-table rows and transform-attributes loop elements. Running out fails with the typed `BudgetExhausted { limit, used }`: `VMError::BudgetExhausted` in an `Isolate`, `EvaluationError::BudgetExhausted` from `Decision::evaluate_metered`. The meter is sticky, and the graph evaluator checks it after every node, so an exhaustion swallowed inside a node (for example a non-strict table cell) still aborts the evaluation. See "Metering cost model". | Review F3: poll/depth/reset counts do not bound work. (norn-v2.0.1-3) |
| S5 | Feature `deterministic-temporal` (default **off**; on `zen-expression`, forwarded by `zen-engine` and `zen-tmpl`, activates `zen-types::VariableType::Timestamp`): strict pure `date` / `timestamp` constructors, distinct calendar-date / UTC-instant values, total same-kind ordering and semantic equality, checked `days_between`, `add_days`, `seconds_between`, `add_seconds`. Disables upstream `d`, all deprecated temporal functions and all legacy date methods in this profile. | #3112 season-window ordering; no ambient clock, timezone or string coercion. Proposed `norn-v2.0.1-8`; qualification and tagging pending lead review. |
| S6 | An empty template literal (`` `` ``, zero parts) evaluates to `""`. Upstream's `Join` opcode sizes its buffer with `separator.len() * (parts.len() - 1)`, which underflows for zero parts: it panics when overflow checks are on (debug and test builds) and wraps in release, where the empty `""` separator makes the product 0. The separator count now saturates and the capacity sum is saturating. | Panic fix, like S1. Release-build results and metering counts are unchanged, so the semantic profile identity does not change. Found by Norn's coverage-guided `zen_expression` fuzz target (Norn-cloud/norn-platform#3243). |

The semantic patches S2 to S5 are **off by default**. The Norn profile enables them
explicitly, for example
`zen-engine = { ..., default-features = false, features = ["strict-errors", "deterministic-maps", "metering"] }`.
The general ban on randomness (`rand`) belongs to Norn's checker/admission.
S5 closes the temporal runtime surface when explicitly enabled: its `date(text)`
replaces the deprecated upstream numeric `date`, and upstream `d()` / date methods
are unavailable. Default builds retain every upstream temporal function.

Known gaps, outside this series:
- Diagnostic type-union strings in `functions/defs.rs` are built from std `HashSet`,
  so their order varies. They are diagnostics, not decision output.
- Trace maps (`HashMap<node id, trace>`) serialize in hash order. They are
  informational only.
- `GraphWalker::ITER_MAX` (1000 switch resets) still ends a walk silently. Metering
  bounds the work that leads up to it.

## Deterministic temporal contract (S5)

Enable `deterministic-temporal` alongside the Norn profile features. JSON strings
remain strings; expressions explicitly call `date(input.start)` or
`timestamp(input.observedAt)`. The public `zen_expression::temporal::Temporal`
constructors also yield checked, opaque dynamic values via `into_variable()`;
these admission helpers are unmetered and examine at most 35 bytes. No implicit
conversion, host timezone, locale, database or clock is used.

- `date(text)` accepts exactly `YYYY-MM-DD`, Gregorian calendar years
  **0001..9999**. Date output uses the same spelling.
- `timestamp(text)` accepts uppercase `YYYY-MM-DDTHH:MM:SS[.fraction]Z` or an
  explicit `+HH:MM` / `-HH:MM` offset. Fractions have **1..9 digits**; all are
  preserved exactly. UTC output uses Chrono `AutoSi` (0/3/6/9 fractional digits).
  `string(value)` and JSON serialization use the same canonical spelling.
  Leap seconds, unknown offset `-00:00`, offset-less text, invalid calendars and
  UTC normalization outside years 0001..9999 fail. Numeric epochs, named zones,
  abbreviated dates, whitespace and unsupported precision fail.
- `<`, `<=`, `>`, `>=` and equality compare same-kind values by calendar day or
  UTC instant. Date/instant pairs fail; strings are never implicitly parsed.
  `VariableType::Timestamp` metadata is available in every build so Cargo feature
  unification cannot invalidate exhaustive engine matches. Static signatures
  distinguish `Date` and `Timestamp`; runtime checks apply
  even to dynamically supplied operands.
- `days_between(start, end)` returns signed **end minus start** in calendar days.
  `add_days(date, integer)` adds signed calendar days. `seconds_between(start,
  end)` returns exact signed decimal seconds including nanoseconds;
  `add_seconds(timestamp, integer)` adds signed seconds, preserving the fraction.
  All additions use checked Chrono arithmetic. Fractional amounts, integer
  overflow and results outside years 0001..9999 fail, with no wrap or clamp.
- Legacy `d` (including zero-argument clock access), deprecated temporal
  builtins and legacy date methods are absent from the profile registry. Only
  the six S5 functions are provided; upstream default builds are unchanged.

With `metering`, parsing is charged **before** execution by string byte length
plus the ordinary opcode unit. Lexical checks and Chrono parsing are bounded by
35 bytes. Arithmetic pays shallow argument sizes plus
`cost::TEMPORAL_ARITHMETIC = 8` and the opcode unit; its work is constant regardless
of the day/second amount. Failed calls pay the same attempted charge. Comparisons
are constant work and pay the ordinary opcode unit (equality also retains its
existing shallow-size charge). This is a new semantic profile identity.
`core/expression/tests/temporal.rs` runs the identical results, rejections and
pinned fuel fixtures natively and via `wasm-bindgen-test-runner` in Norn CI.


## Releases (tags)

Every tag is immutable and sits on `norn/v2.0.1`. `git log zen-engine-v2.0.1..<tag>`
lists the full downstream delta.

| Tag | Commit | Adds |
| --- | --- | --- |
| `norn-v2.0.1-1` | `6ac6817e799a55e8c4e43ef26784088c45d9e3d8` | Compatibility series C1 to C5, S1 (`median([])`), Norn CI, this file |
| `norn-v2.0.1-2` | `1e6c6bd2ec36df15cc31680bda513dcb4cce6d5c` | C6 (`schema-resolvers`, typed `SchemaCompileError`), native pure-tree CI check, regex-backend difference tests plus the pure-regex CI job (review fixes on -1) |
| `norn-v2.0.1-3` | `526980cc0c8a6f8eb2f59f46f1eee763740fdf53` | Semantic series S2 `strict-errors` (#2), S3 `deterministic-maps` (#3), S4 `metering` (#4), all default off, with CI covering them on and off and on wasm32-unknown-unknown (T3-ZEN-1, Norn-cloud/norn-platform#2768) |
| `norn-v2.0.1-4` | `0b5aeaacff901ada6e6ef102eaa109520ded233d` | Fixes from the gpt-6-sol review of -1..-3: strict evaluation skips the table index; transform loops are metered; `flatten`/`merge`/deep builtins are charged by the data they traverse (bounded walk); sub-decision errors stay in the source chain; CI builds the zen-engine Norn profile for wasm32 |
| `norn-v2.0.1-5` | head of Norn-cloud/zen#6 (rebase-merged) | Measuring `flatten`/`merge`/`Flatten` costs is bounded by the remaining budget (outer length first, early stop, clamp); `Meter::scan_steps` diagnostic (gpt-6-sol round-2 review of -4) |
| `norn-v2.0.1-6` | `b50f240c1d125b9445895040d75031f50ffe9380` | C7 (`runtime-rng`; fixed ahash seeds in the pure profile so zen-expression builds for wasm32 without getrandom) |
| `norn-v2.0.1-7` | head of the C8/C9 PR | C8 (`json-schema`; the zen-engine Norn profile has no jsonschema and no getrandom on wasm32) and C9 (`ts-types`; no swc parser or MPL-2.0 smartstring in the Norn profile), plus CI assertions for both (T3-DEC-2, Norn-cloud/norn-platform#2670) |
| `norn-v2.0.1-8` | `c4e6683799419bc74d4850afd78e4bf743cbbbf9` | S5 deterministic temporal ordering and pure arithmetic (same-kind date/timestamp ordering, checked arithmetic 0001–9999, metered; ambient-clock temporal functions excluded) and C10 (CI Android SDK setup); reviewed in Norn-cloud/zen#10, Astra approve (Norn-cloud/norn-platform#3112) |
| `norn-v2.0.1-9` | `cc30eb5c0d2acc3f989e18fd0458fc910697b6ab` | S6: an empty template literal no longer underflows the `Join` opcode (panic under overflow checks; release results and metering counts unchanged, no profile identity change); reviewed in Norn-cloud/zen#12, gpt-6.1-sol approve (found by Norn-cloud/norn-platform#3243) |

## Feature matrix (`zen-engine`)

| Feature | Default | Gates |
| --- | --- | --- |
| `js` | on | QuickJS function nodes (`rquickjs`, `tokio` OnceCell, TypeScript stripping via `swc_ts_fast_strip`) |
| `http` | on | Native HTTP backend for function nodes (`reqwest`, `reqsign`, `sha2`, `http`, `async-trait`). Implies `js`. Non-wasm only. |
| `schema-resolvers` | on | jsonschema's HTTP + file `$ref` retrieval (`reqwest`, `tokio`, `rustls`). Non-wasm only. |
| `json-schema` | on | Norn C8: input/output node `schema` validation (`jsonschema`, which pulls getrandom `wasm_js` on wasm32). Off: a node schema is a typed `SchemaCompileError`. |
| `ts-types` | on | Norn C9: swc TypeScript parsing of function-node type declarations in workspace analysis (implied by `js`). Off: declared function types stay unresolved. |
| `regex-deprecated` | on | `regex` backend for zen-expression / zen-tmpl (upstream default) |
| `regex-lite` | off | `regex-lite` backend. Takes precedence when enabled. |
| `bindgen` | off | `rquickjs/bindgen`. Implies `js` and needs libclang. |
| `arbitrary_precision` | off | unchanged from upstream |
| `strict-errors` | off | Norn semantic patch S2: typed abort on failing table cells / switch conditions |
| `deterministic-maps` | off | Norn semantic patch S3: insertion-ordered `VariableMap` at every size (`indexmap`). Also on `zen-expression` and `zen-types`. |
| `deterministic-temporal` | off | Norn S5: pure typed temporal parsing, comparisons and checked arithmetic. Forwarded by zen-tmpl too. |
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
| `flatten`, `merge`, `Flatten` opcode | + outer length + shallow size of every direct child (`cost::nested_size_capped`) instead of the shallow size (-4). The outer length is counted first, and the child scan stops once the total exceeds the remaining budget; the result is clamped to `remaining + 1` (-5) |
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
- Measuring a charge is itself bounded by the remaining budget: `nested_size_capped`
  and `deep_size_capped` stop once they exceed it, and oversized results are clamped
  to `remaining + 1`. `Meter::scan_steps()` reports how many values the measurements
  visited. A 1M-element `flatten`/`merge` input against a budget of 100 fails at
  `used: 102` after 0 child scan steps (-5).
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

The upstream Rust OS matrix still compiles all features together, then runs the
upstream corpus with all **legacy** features explicitly listed. S5 intentionally
replaces the old temporal language, so its qualification corpus runs separately
in the Norn native/wasm jobs; it cannot share the legacy temporal expectations.
Default-feature upstream testing is retained on every OS.

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


## New-tag checklist (S5 candidate)

Published as **`norn-v2.0.1-8`** (2026-10-06). Remaining step: qualify and bump the norn-platform profile (step 4).

1. Merge the temporal fork PR into `norn/v2.0.1` only after Norn CI is green:
   default upstream suite, pure profile, dependency trees, existing semantic
   patch tests, native temporal fixtures and wasm32 temporal parity / engine build.
2. Obtain the lead's independent review and link its ship-bar verdict from the PR.
3. The lead creates an immutable annotated `norn-v2.0.1-8` on the reviewed merged
   commit and updates the release table. This worker does not create a tag.
4. Qualify the new semantic profile in norn-platform. Update its fork ledger,
   pin file, Cargo tag/lock, checker signatures, cost model and positive temporal
   relational conformance tests together, following the platform checklist.
   No platform pin or platform source is changed in this fork PR.
