//! Replay types and data structures

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A collection of saved requests (like a folder)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A saved request that can be replayed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedRequest {
    pub id: String,
    pub name: String,
    pub collection_id: Option<String>,
    pub method: String,
    pub url: String,
    pub headers: Vec<KeyValuePair>,
    pub query_params: Vec<KeyValuePair>,
    pub body: RequestBody,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Key-value pair with enabled flag (for headers, params)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyValuePair {
    pub key: String,
    pub value: String,
    pub enabled: bool,
    #[serde(default)]
    pub description: Option<String>,
}

/// Request body types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RequestBody {
    #[serde(rename = "none")]
    None,
    
    #[serde(rename = "raw")]
    Raw {
        content: String,
        #[serde(rename = "contentType")]
        content_type: String,
    },
    
    #[serde(rename = "json")]
    Json {
        content: String,
    },
    
    #[serde(rename = "form-data")]
    FormData {
        fields: Vec<FormDataField>,
    },
    
    #[serde(rename = "x-www-form-urlencoded")]
    UrlEncoded {
        fields: Vec<KeyValuePair>,
    },
    
    #[serde(rename = "binary")]
    Binary {
        #[serde(rename = "fileName")]
        file_name: Option<String>,
        data: Option<String>, // Base64 encoded
    },
}

impl Default for RequestBody {
    fn default() -> Self {
        RequestBody::None
    }
}

/// Form data field (can be text or file)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormDataField {
    pub key: String,
    pub value: String,
    #[serde(rename = "type")]
    pub field_type: FormDataFieldType,
    pub enabled: bool,
    #[serde(rename = "fileName")]
    pub file_name: Option<String>,
    #[serde(rename = "contentType")]
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FormDataFieldType {
    Text,
    File,
}

/// Response from executing a request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: Option<String>, // Base64 encoded for binary
    #[serde(rename = "bodySize")]
    pub body_size: u64,
    #[serde(rename = "contentType")]
    pub content_type: Option<String>,
    #[serde(rename = "durationMs")]
    pub duration_ms: u64,
}

/// Request execution history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestHistory {
    pub id: String,
    pub saved_request_id: Option<String>,
    pub request: SavedRequest,
    pub response: Option<ReplayResponse>,
    pub error: Option<String>,
    pub executed_at: i64,
}

/// Result of tree structure for collections and requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionTree {
    pub collections: Vec<CollectionNode>,
    pub root_requests: Vec<SavedRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionNode {
    pub collection: Collection,
    pub children: Vec<CollectionNode>,
    pub requests: Vec<SavedRequest>,
}
