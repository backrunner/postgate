//! Replay Tauri commands

use crate::error::Result;
use crate::replay::{
    execute_request, Collection, CollectionNode, CollectionTree, ReplayResponse, RequestHistory,
    SavedRequest,
};
use crate::state::AppState;
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
    let response = execute_request(&request).await?;

    // Optionally save to history
    let db = state.get_database().await?;
    let history = RequestHistory {
        id: Uuid::new_v4().to_string(),
        saved_request_id: Some(request.id.clone()),
        request: request.clone(),
        response: Some(response.clone()),
        error: None,
        executed_at: chrono::Utc::now().timestamp_millis(),
    };
    let _ = db.save_history(&history).await;

    Ok(response)
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
    pub method: String,
    pub url: String,
    pub path: String,
    pub request_headers: Option<std::collections::HashMap<String, String>>,
}

/// Import a request from captured data
#[tauri::command]
pub async fn import_from_capture(
    state: State<'_, Arc<AppState>>,
    captured_request: ImportCaptureInput,
    collection_id: Option<String>,
) -> Result<SavedRequest> {
    let now = chrono::Utc::now().timestamp_millis();

    // Convert headers to KeyValuePairs
    let headers = captured_request
        .request_headers
        .unwrap_or_default()
        .into_iter()
        .map(|(key, value)| crate::replay::KeyValuePair {
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
                .map(|(k, v)| crate::replay::KeyValuePair {
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
        body: crate::replay::RequestBody::None, // Body would need to be loaded separately
        created_at: now,
        updated_at: now,
    };

    let db = state.get_database().await?;
    db.save_request(&request).await?;

    Ok(request)
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
