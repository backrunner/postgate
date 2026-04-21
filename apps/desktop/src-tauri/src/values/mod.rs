//! Whistle-compatible "Values" store.
//!
//! Values are named reusable payloads (mock JSON, HTML fragments, scripts, …)
//! that rules reference via `{name}` (plain) or `` `{name}` `` (template).
//! This module provides:
//!
//! * [`ValueEntry`] — the persisted representation (mirrors the SQLite row)
//! * [`resolver`] — resolves an action argument against the in-memory store,
//!   inline per-group definitions, and the current request context
//!
//! See https://wproxy.org/whistle/data.html for the reference behaviour.

pub mod resolver;

use serde::{Deserialize, Serialize};

/// A single stored value.
///
/// `name` is the key used in rule references (e.g. `test.html`,
/// `mock/users.json`). `/` inside the name is purely a UI folder separator —
/// the backend treats the whole string as an opaque key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueEntry {
    pub name: String,
    pub content: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[allow(unused_imports)]
pub use resolver::{resolve, resolve_str, RequestCtx};
