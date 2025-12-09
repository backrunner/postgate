//! Plugin system for PostGate
//!
//! This module provides a plugin architecture that allows extending PostGate
//! functionality through JavaScript plugins. Plugins can intercept and modify
//! requests/responses, add UI panels, and more.

mod manager;
mod runtime;
mod types;

pub use manager::PluginManager;
pub use types::*;
