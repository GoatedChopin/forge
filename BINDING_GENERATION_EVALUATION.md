# Binding Generation System Evaluation

**Date:** 2026-03-13
**Crate:** `forge-codegen` (~1,858 lines)
**Purpose:** Generates type-safe frontend bindings from Rust source code via `syn` AST parsing

## Overview

The binding generation system parses Rust source files using the `syn` crate (no compilation required), extracts decorated items (`#[forge::model]`, `#[forge::query]`, `#[forge::mutation]`, etc.), and generates frontend bindings for two targets:

- **SvelteKit** — TypeScript bindings (`types.ts`, `api.ts`, `reactive.svelte.ts`, etc.)
- **Dioxus** — Rust bindings (`types.rs`, `api.rs`, `mod.rs`)

## Identified Weaknesses

### 1. Test Coverage Gaps

- **Dioxus API generator has zero tests** — the entire Rust binding generation for functions/hooks is untested.
- No integration tests for end-to-end codegen pipelines.
- No tests for error/edge-case scenarios in the parser (only happy-path tests exist).
- Total: only 29 tests across the entire codegen crate.

**Test distribution:**
| Component | Tests |
|-----------|-------|
| TypeScript API | 11 |
| TypeScript Types | 4 |
| TypeScript Reactive | 2 |
| TypeScript Mod | 2 |
| Client | 2 |
| Stores | 1 |
| Dioxus Types | 1 |
| **Dioxus API** | **0** |
| Parser | 6 |

### 2. Fragile Parser Fallbacks

- Type parsing uses `.unwrap()` with a silent fallback to `"String"` — if a type can't be parsed, it becomes `String` with no warning to the user.
- Context argument detection is string-based (`type.contains("Context")`) — any custom type with "Context" in its name gets incorrectly filtered out.
- Unparseable files are silently skipped with only debug-level logging.

### 3. Limited Rust Type Support

The system does not handle:
- Complex generics beyond `Option<T>` and `Vec<T>`
- Path-qualified types (e.g., `crate::module::Type`)
- Associated types or trait objects
- Lifetime parameters or generic constraints
- `#[serde(rename = "...")]` field-level attributes — generated bindings won't match actual serialization

### 4. No Validation Layer

- No check that all referenced custom types actually exist in the schema.
- No circular dependency detection between types.
- No validation that `Upload` types only appear where they're valid (in mutations/jobs, not model fields — where it silently maps to `Bytea`).
- No validation that parsed functions are actually valid RPC candidates.

### 5. Incomplete Feature Support

- **Doc comments are not preserved** in generated code — API documentation is lost.
- **Enum values** always lowercase to snake_case in SQL; no support for explicit string representations in TypeScript.
- **No incremental generation** — always does a full regeneration, which will scale poorly.
- **No schema versioning** — no way to detect breaking changes between generations.

### 6. Performance Concerns

- Linear scan of all `.rs` files with no caching between runs.
- Repeated `String::push_str` allocations (no pre-sized buffers or `Write` trait usage).
- No parallel file processing.
- Will likely struggle with very large projects (1000+ source files).

### 7. Dioxus Generator is Underdeveloped

- Only 1 test (for types only).
- Generated hooks don't expose all runtime controls available from `forge-dioxus`.
- Significantly less mature than the SvelteKit/TypeScript path.

### 8. Error Handling Inconsistencies

- Workspace-level lint allows `clippy::unwrap_used` for the entire codegen crate.
- Mix of `?` propagation, silent fallbacks, and unwraps — no consistent strategy.
- Error type exists (`Error::Io`, `Error::Serialization`, `Error::Template`) but isn't used in many fallible paths.

### 9. Simple Pluralization Heuristic

- Table name pluralization uses basic rules that fail on irregular nouns: "person" → "persons" (should be "people"), "mouse" → "mouses".

### 10. Tight Coupling to Runtime Packages

- TypeScript output assumes `@forge-rs/svelte` exists.
- Dioxus output assumes `forge-dioxus` crate exists.
- No abstraction layer — changes to runtime package APIs require codegen changes.

## Strengths

- **Deterministic output** via `BTreeMap` ordering.
- **Clean separation** of concerns: parsing, schema registry, and per-target generators.
- **Extensible architecture** via `FrontendTarget` system for adding new frameworks.
- **First-class Svelte 5 runes support** with reactive subscriptions.
- **Automatic Upload detection** switching to multipart form-data.
- **Graceful degradation** — doesn't crash on bad files.
- **No compilation required** — uses AST parsing only.

## Priority Recommendations

| Priority | Action |
|----------|--------|
| **P0** | Add tests for Dioxus API generation |
| **P0** | Add validation that referenced custom types exist |
| **P1** | Replace silent type-parsing fallbacks with warnings/errors |
| **P1** | Support `#[serde(rename)]` attributes |
| **P1** | Fix Context argument detection to be attribute-based, not string-based |
| **P2** | Preserve doc comments in generated bindings |
| **P2** | Add incremental generation / caching |
| **P2** | Add circular dependency detection |
| **P3** | Improve pluralization (use a crate like `inflector`) |
| **P3** | Parallelize file parsing for large projects |
