/// Library entry point for integration tests.
///
/// Exposes only the `types` module so that `tests/component_states.spec.rs` can
/// import real production types (SessionInfo, RolloutPlan, OpsRoute, etc.)
/// instead of synthetic local definitions.  Only `types` is exposed here
/// because it contains no WASM/browser-specific code and compiles for all
/// targets (needed for both `cargo test` host builds and wasm-pack WASM builds).
pub mod types;

#[cfg(target_arch = "wasm32")]
pub mod components;

#[cfg(target_arch = "wasm32")]
pub mod services;

#[cfg(target_arch = "wasm32")]
pub mod store;

#[cfg(target_arch = "wasm32")]
pub mod pages;
