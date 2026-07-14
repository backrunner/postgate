//! Replay Tauri commands

use crate::error::Result;
use crate::replay::{
    execute_request, Collection, CollectionNode, CollectionTree, KeyValuePair, ReplayResponse,
    RequestBody, RequestHistory, SavedRequest,
};
use crate::state::AppState;
use base64::{engine::general_purpose, Engine as _};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

/// Get all collections as a tree structure
#[tauri::command]
pub async fn get_collection_tree(state: State<'_, Arc<AppState>>) -> Result<CollectionTree> {
    let db = state.get_database().await?;

    // Get all collections
    let collections = db.get_collections().await?;

    // Get all saved requests
    let requests = db.get_saved_requests().await?;

    // Build tree structure
    let tree = build_collection_tree(collections, requests);

    Ok(tree)
}

/// Get all collections (flat list)
#[tauri::command]
pub async fn get_collections(state: State<'_, Arc<AppState>>) -> Result<Vec<Collection>> {
    let db = state.get_database().await?;
    db.get_collections().await
}

/// Create a new collection
#[tauri::command]
pub async fn create_collection(
    state: State<'_, Arc<AppState>>,
    name: String,
    parent_id: Option<String>,
) -> Result<Collection> {
    let db = state.get_database().await?;

    let now = chrono::Utc::now().timestamp_millis();
    let collection = Collection {
        id: Uuid::new_v4().to_string(),
        name,
        parent_id,
        created_at: now,
        updated_at: now,
    };

    db.save_collection(&collection).await?;

    Ok(collection)
}

/// Update a collection
#[tauri::command]
pub async fn update_collection(
    state: State<'_, Arc<AppState>>,
    id: String,
    name: Option<String>,
    parent_id: Option<Option<String>>,
) -> Result<Collection> {
    let db = state.get_database().await?;

    let mut collection = db
        .get_collection(&id)
        .await?
        .ok_or_else(|| crate::error::PostGateError::NotFound("Collection not found".into()))?;

    if let Some(name) = name {
        collection.name = name;
    }
    if let Some(parent_id) = parent_id {
        collection.parent_id = parent_id;
    }
    collection.updated_at = chrono::Utc::now().timestamp_millis();

    db.save_collection(&collection).await?;

    Ok(collection)
}

/// Delete a collection and its contents
#[tauri::command]
pub async fn delete_collection(
    state: State<'_, Arc<AppState>>,
    id: String,
    delete_contents: bool,
) -> Result<()> {
    let db = state.get_database().await?;

    if delete_contents {
        // Delete all requests in this collection
        db.delete_requests_in_collection(&id).await?;
        // Delete all child collections recursively
        db.delete_collection_recursive(&id).await?;
    } else {
        // Move contents to root (null parent)
        db.move_collection_contents_to_root(&id).await?;
    }

    db.delete_collection(&id).await?;

    Ok(())
}

/// Get all saved requests (optionally filtered by collection)
#[tauri::command]
pub async fn get_saved_requests(
    state: State<'_, Arc<AppState>>,
    collection_id: Option<String>,
) -> Result<Vec<SavedRequest>> {
    let db = state.get_database().await?;

    if let Some(collection_id) = collection_id {
        db.get_requests_in_collection(&collection_id).await
    } else {
        db.get_saved_requests().await
    }
}

/// Get a single saved request
#[tauri::command]
pub async fn get_saved_request(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<Option<SavedRequest>> {
    let db = state.get_database().await?;
    db.get_saved_request(&id).await
}

/// Create a new saved request
#[tauri::command]
pub async fn create_saved_request(
    state: State<'_, Arc<AppState>>,
    request: SavedRequest,
) -> Result<SavedRequest> {
    let db = state.get_database().await?;

    let now = chrono::Utc::now().timestamp_millis();
    let mut request = request;
    request.id = Uuid::new_v4().to_string();
    request.created_at = now;
    request.updated_at = now;

    db.save_request(&request).await?;

    Ok(request)
}

/// Update a saved request
#[tauri::command]
pub async fn update_saved_request(
    state: State<'_, Arc<AppState>>,
    request: SavedRequest,
) -> Result<SavedRequest> {
    let db = state.get_database().await?;

    let mut request = request;
    request.updated_at = chrono::Utc::now().timestamp_millis();

    db.save_request(&request).await?;

    Ok(request)
}

/// Delete a saved request
#[tauri::command]
pub async fn delete_saved_request(state: State<'_, Arc<AppState>>, id: String) -> Result<()> {
    let db = state.get_database().await?;
    db.delete_request(&id).await
}

/// Move a request to a different collection
#[tauri::command]
pub async fn move_request(
    state: State<'_, Arc<AppState>>,
    request_id: String,
    collection_id: Option<String>,
) -> Result<()> {
    let db = state.get_database().await?;
    db.move_request(&request_id, collection_id.as_deref()).await
}

/// Duplicate a saved request
#[tauri::command]
pub async fn duplicate_request(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<SavedRequest> {
    let db = state.get_database().await?;

    let original = db
        .get_saved_request(&id)
        .await?
        .ok_or_else(|| crate::error::PostGateError::NotFound("Request not found".into()))?;

    let now = chrono::Utc::now().timestamp_millis();
    let duplicate = SavedRequest {
        id: Uuid::new_v4().to_string(),
        name: format!("{} (copy)", original.name),
        created_at: now,
        updated_at: now,
        ..original
    };

    db.save_request(&duplicate).await?;

    Ok(duplicate)
}

/// Execute a request and return the response
#[tauri::command]
pub async fn execute_saved_request(
    state: State<'_, Arc<AppState>>,
    request: SavedRequest,
) -> Result<ReplayResponse> {
    let execution = execute_request(&request).await;
    let (response, error) = match &execution {
        Ok(response) => (Some(response.clone()), None),
        Err(error) => (None, Some(error.to_string())),
    };

    // Save both successful and failed sends so Sender history is a reliable
    // record of what the user attempted.
    let db = state.get_database().await?;
    let history = RequestHistory {
        id: Uuid::new_v4().to_string(),
        saved_request_id: (!request.id.is_empty()).then(|| request.id.clone()),
        request: request.clone(),
        response,
        error,
        executed_at: chrono::Utc::now().timestamp_millis(),
    };
    let _ = db.save_history(&history).await;

    execution
}

/// Get request history
#[tauri::command]
pub async fn get_request_history(
    state: State<'_, Arc<AppState>>,
    limit: Option<i32>,
) -> Result<Vec<RequestHistory>> {
    let db = state.get_database().await?;
    db.get_history(limit.unwrap_or(50)).await
}

/// Clear request history
#[tauri::command]
pub async fn clear_request_history(state: State<'_, Arc<AppState>>) -> Result<()> {
    let db = state.get_database().await?;
    db.clear_history().await
}

/// Import request data from capture
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ImportCaptureInput {
    pub id: Option<String>,
    pub method: String,
    pub url: String,
    pub path: String,
    pub request_headers: Option<HashMap<String, String>>,
}

/// Import a request from captured data
#[tauri::command]
pub async fn import_from_capture(
    state: State<'_, Arc<AppState>>,
    captured_request: ImportCaptureInput,
    collection_id: Option<String>,
) -> Result<SavedRequest> {
    let now = chrono::Utc::now().timestamp_millis();

    let request_headers = captured_request.request_headers.unwrap_or_default();
    let content_type = header_value(&request_headers, "content-type").map(str::to_string);
    let body_bytes = load_captured_request_body(&state, captured_request.id.as_deref()).await?;
    let body = request_body_from_capture(body_bytes.as_deref(), content_type.as_deref());

    // Convert headers to KeyValuePairs
    let headers = request_headers
        .into_iter()
        .map(|(key, value)| KeyValuePair {
            key,
            value,
            enabled: true,
            description: None,
        })
        .collect();

    // Parse query params from URL
    let url_obj = url::Url::parse(&captured_request.url).ok();
    let query_params = url_obj
        .as_ref()
        .map(|u| {
            u.query_pairs()
                .map(|(k, v)| KeyValuePair {
                    key: k.to_string(),
                    value: v.to_string(),
                    enabled: true,
                    description: None,
                })
                .collect()
        })
        .unwrap_or_default();

    // Get base URL without query string
    let base_url = url_obj
        .map(|mut u| {
            u.set_query(None);
            u.to_string()
        })
        .unwrap_or(captured_request.url.clone());

    let request = SavedRequest {
        id: Uuid::new_v4().to_string(),
        name: format!("{} {}", captured_request.method, captured_request.path),
        collection_id,
        method: captured_request.method,
        url: base_url,
        headers,
        query_params,
        body,
        created_at: now,
        updated_at: now,
    };

    let db = state.get_database().await?;
    db.save_request(&request).await?;

    Ok(request)
}

async fn load_captured_request_body(
    state: &State<'_, Arc<AppState>>,
    capture_id: Option<&str>,
) -> Result<Option<Vec<u8>>> {
    let Some(capture_id) = capture_id else {
        return Ok(None);
    };

    if let Some(body) = state.body_storage.get_request_body(capture_id).await {
        return Ok(Some(body.data.to_vec()));
    }

    let storage = state.get_captured_storage().await?;
    storage
        .get_body(capture_id, true)
        .await
        .map(|body| body.map(|bytes| bytes.to_vec()))
}

fn request_body_from_capture(body: Option<&[u8]>, content_type: Option<&str>) -> RequestBody {
    let Some(body) = body else {
        return RequestBody::None;
    };
    if body.is_empty() {
        return RequestBody::None;
    }

    let content_type = content_type.unwrap_or("application/octet-stream");
    if looks_textual_content_type(content_type) {
        if let Ok(content) = std::str::from_utf8(body) {
            return RequestBody::Raw {
                content: content.to_string(),
                content_type: content_type.to_string(),
            };
        }
    }

    RequestBody::Binary {
        file_name: None,
        data: Some(general_purpose::STANDARD.encode(body)),
    }
}

fn looks_textual_content_type(content_type: &str) -> bool {
    let content_type = content_type.to_ascii_lowercase();
    content_type.starts_with("text/")
        || content_type.contains("json")
        || content_type.contains("xml")
        || content_type.contains("javascript")
        || content_type.contains("ecmascript")
        || content_type.contains("x-www-form-urlencoded")
        || content_type.contains("graphql")
}

fn header_value<'a>(headers: &'a HashMap<String, String>, name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find_map(|(key, value)| key.eq_ignore_ascii_case(name).then_some(value.as_str()))
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    #[test]
    fn request_body_from_capture_preserves_text_body_as_raw() {
        let body = request_body_from_capture(
            Some(br#"{"ok":true}"#),
            Some("application/json; charset=utf-8"),
        );

        match body {
            RequestBody::Raw {
                content,
                content_type,
            } => {
                assert_eq!(content, r#"{"ok":true}"#);
                assert_eq!(content_type, "application/json; charset=utf-8");
            }
            other => panic!("expected raw body, got {other:?}"),
        }
    }

    #[test]
    fn request_body_from_capture_uses_binary_for_non_text_body() {
        let bytes = [0, 255, 16];
        let body = request_body_from_capture(Some(&bytes), Some("application/octet-stream"));

        match body {
            RequestBody::Binary { file_name, data } => {
                assert!(file_name.is_none());
                assert_eq!(data.as_deref(), Some("AP8Q"));
            }
            other => panic!("expected binary body, got {other:?}"),
        }
    }

    #[test]
    fn header_value_matches_case_insensitively() {
        let headers = HashMap::from([("Content-Type".to_string(), "text/plain".to_string())]);

        assert_eq!(header_value(&headers, "content-type"), Some("text/plain"));
    }
}

// Helper function to build collection tree
fn build_collection_tree(
    collections: Vec<Collection>,
    requests: Vec<SavedRequest>,
) -> CollectionTree {
    use std::collections::HashMap;

    // Group requests by collection
    let mut requests_by_collection: HashMap<Option<String>, Vec<SavedRequest>> = HashMap::new();
    for request in requests {
        requests_by_collection
            .entry(request.collection_id.clone())
            .or_default()
            .push(request);
    }

    // Group collections by parent
    let mut collections_by_parent: HashMap<Option<String>, Vec<Collection>> = HashMap::new();
    for collection in collections {
        collections_by_parent
            .entry(collection.parent_id.clone())
            .or_default()
            .push(collection);
    }

    // Build tree recursively
    fn build_node(
        collection: Collection,
        collections_by_parent: &HashMap<Option<String>, Vec<Collection>>,
        requests_by_collection: &HashMap<Option<String>, Vec<SavedRequest>>,
    ) -> CollectionNode {
        let children = collections_by_parent
            .get(&Some(collection.id.clone()))
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|c| build_node(c, collections_by_parent, requests_by_collection))
            .collect();

        let requests = requests_by_collection
            .get(&Some(collection.id.clone()))
            .cloned()
            .unwrap_or_default();

        CollectionNode {
            collection,
            children,
            requests,
        }
    }

    let root_collections: Vec<CollectionNode> = collections_by_parent
        .get(&None)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|c| build_node(c, &collections_by_parent, &requests_by_collection))
        .collect();

    let root_requests = requests_by_collection
        .get(&None)
        .cloned()
        .unwrap_or_default();

    CollectionTree {
        collections: root_collections,
        root_requests,
    }
}
