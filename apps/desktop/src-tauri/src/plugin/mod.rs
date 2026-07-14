//! Plugin system for PostGate
//!
//! This module provides a plugin architecture that allows extending PostGate
//! functionality through JavaScript plugins. Plugins can intercept and modify
//! requests/responses, add UI panels, and more.
//!
//! The runtime uses Deno Core (embedded V8) for executing JavaScript plugins,
//! eliminating the need for external Node.js installation.

mod manager;
mod ops;
mod runtime;
mod storage;
mod types;

pub(crate) use manager::plugin_identity;
pub use manager::PluginManager;
pub use storage::PluginStorage;
pub use types::*;
