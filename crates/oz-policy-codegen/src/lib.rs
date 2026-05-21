//! Track-B codegen pipeline.
//!
//! Phase 3 surface:
//! * `sandbox` — sandboxed `cargo build --target wasm32-unknown-unknown` +
//!   `stellar contract optimize` driver. Produces reproducible WASM and
//!   caches by `sha256(Cargo.toml || "\0" || src/lib.rs)`. (Stream B.)
//! * `render` — turns a `PolicySpec` into a [`sandbox::RenderedCrate`] via
//!   askama templates. See `templates/` at the workspace root. (Stream A.)
//! * `context` — pure-data render-context structs consumed by askama. The
//!   `is_symbol_short_safe` classifier lives here and is the single source
//!   of truth for the `symbol_short!` 9-ASCII-char rule.
//!
//! Phase 1 placeholder (`placeholder.rs`) is retained because external
//! callers (Phase 1 binary completion tests) still reference its symbol; it
//! will be removed in Phase 9 cleanup.
//!
//! See `docs/codegen-dependency-mode.md` for the rationale behind generated
//! crates depending on `stellar-accounts = "=0.7.1"` as a library rather than
//! re-implementing the trait pattern.

#![forbid(unsafe_code)]

pub mod context;
pub mod placeholder;
pub mod render;
pub mod sandbox;

pub use render::render_contract;
pub use sandbox::{compile, CompiledArtifact, RenderedCrate, SandboxError};
