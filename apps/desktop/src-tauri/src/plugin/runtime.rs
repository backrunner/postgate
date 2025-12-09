//! JavaScript runtime for executing plugins
//!
//! This module provides a lightweight JavaScript runtime for executing plugins.
//! For simplicity, we use a subprocess-based approach with Node.js.

use crate::error::{PostGateError, Result};
use crate::plugin::types::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdout, Command};
use tokio::sync::{mpsc, oneshot, Mutex};

/// JavaScript plugin runtime using Node.js subprocess
pub struct PluginRuntime {
    plugin_id: String,
    plugin_path: PathBuf,
    process: Option<Child>,
    stdin: Option<Arc<Mutex<tokio::process::ChildStdin>>>,
    message_tx: mpsc::Sender<HostMessage>,
    pending_requests: Arc<Mutex<HashMap<String, oneshot::Sender<Option<PluginResponse>>>>>,
    pending_storage: Arc<Mutex<HashMap<u64, oneshot::Sender<StorageResult>>>>,
    storage_id: AtomicU64,
    loaded: bool,
}

impl PluginRuntime {
    /// Create a new plugin runtime
    pub fn new(plugin_id: String, plugin_path: PathBuf) -> Self {
        let (message_tx, _) = mpsc::channel(100);
        
        Self {
            plugin_id,
            plugin_path,
            process: None,
            stdin: None,
            message_tx,
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            pending_storage: Arc::new(Mutex::new(HashMap::new())),
            storage_id: AtomicU64::new(0),
            loaded: false,
        }
    }

    /// Start the plugin runtime
    pub async fn start(&mut self, config: HashMap<String, String>) -> Result<()> {
        // Find Node.js executable
        let node_path = which::which("node")
            .map_err(|_| PostGateError::Plugin("Node.js not found. Please install Node.js to use plugins.".into()))?;

        // Create the plugin wrapper script path
        let wrapper_path = self.create_wrapper_script().await?;

        // Spawn Node.js process
        let mut process = Command::new(node_path)
            .arg(&wrapper_path)
            .arg(&self.plugin_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| PostGateError::Plugin(format!("Failed to start plugin process: {}", e)))?;

        let stdin = process.stdin.take()
            .ok_or_else(|| PostGateError::Plugin("Failed to get plugin stdin".into()))?;
        let stdout = process.stdout.take()
            .ok_or_else(|| PostGateError::Plugin("Failed to get plugin stdout".into()))?;

        self.process = Some(process);
        self.stdin = Some(Arc::new(Mutex::new(stdin)));

        // Start message reader
        let pending_requests = self.pending_requests.clone();
        let pending_storage = self.pending_storage.clone();
        let plugin_id = self.plugin_id.clone();
        
        tokio::spawn(async move {
            Self::read_messages(stdout, pending_requests, pending_storage, plugin_id).await;
        });

        // Send init message
        self.send_message(&HostMessage::Init { config }).await?;
        self.loaded = true;

        Ok(())
    }

    /// Stop the plugin runtime
    pub async fn stop(&mut self) -> Result<()> {
        // Send unload message first (before taking ownership of process)
        if self.stdin.is_some() && self.loaded {
            let _ = self.send_message(&HostMessage::Unload).await;
            
            // Give plugin time to cleanup
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
        
        // Now kill the process
        if let Some(ref mut process) = self.process {
            let _ = process.kill().await;
        }
        
        self.process = None;
        self.stdin = None;
        self.loaded = false;
        
        Ok(())
    }

    /// Check if runtime is loaded
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// Handle a request through the plugin
    pub async fn handle_request(
        &self,
        request: PluginRequest,
        context: PluginRequestContext,
    ) -> Result<Option<PluginResponse>> {
        if !self.loaded {
            return Err(PostGateError::Plugin("Plugin not loaded".into()));
        }

        let (tx, rx) = oneshot::channel();
        
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(request.id.clone(), tx);
        }

        self.send_message(&HostMessage::HandleRequest { request: request.clone(), context }).await?;

        // Wait for response with timeout
        match tokio::time::timeout(tokio::time::Duration::from_secs(30), rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(PostGateError::Plugin("Plugin request channel closed".into())),
            Err(_) => {
                // Remove pending request
                let mut pending = self.pending_requests.lock().await;
                pending.remove(&request.id);
                Err(PostGateError::Plugin("Plugin request timeout".into()))
            }
        }
    }

    /// Handle response modification through the plugin
    pub async fn handle_response(
        &self,
        request: PluginRequest,
        response: PluginResponse,
        context: PluginRequestContext,
    ) -> Result<PluginResponse> {
        if !self.loaded {
            return Ok(response);
        }

        let (tx, rx) = oneshot::channel();
        
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(format!("res_{}", request.id), tx);
        }

        self.send_message(&HostMessage::HandleResponse { 
            request: request.clone(), 
            response: response.clone(), 
            context 
        }).await?;

        // Wait for response with timeout
        match tokio::time::timeout(tokio::time::Duration::from_secs(30), rx).await {
            Ok(Ok(Some(modified))) => Ok(modified),
            Ok(Ok(None)) => Ok(response),
            Ok(Err(_)) => Ok(response),
            Err(_) => {
                let mut pending = self.pending_requests.lock().await;
                pending.remove(&format!("res_{}", request.id));
                Ok(response)
            }
        }
    }

    /// Send a message to the plugin
    async fn send_message(&self, message: &HostMessage) -> Result<()> {
        if let Some(ref stdin) = self.stdin {
            let json = serde_json::to_string(message)
                .map_err(|e| PostGateError::Plugin(format!("Failed to serialize message: {}", e)))?;
            
            let mut stdin_guard = stdin.lock().await;
            
            stdin_guard.write_all(json.as_bytes()).await
                .map_err(|e| PostGateError::Plugin(format!("Failed to write to plugin: {}", e)))?;
            stdin_guard.write_all(b"\n").await
                .map_err(|e| PostGateError::Plugin(format!("Failed to write newline: {}", e)))?;
            stdin_guard.flush().await
                .map_err(|e| PostGateError::Plugin(format!("Failed to flush stdin: {}", e)))?;
        }
        
        Ok(())
    }

    /// Read messages from plugin stdout
    async fn read_messages(
        stdout: ChildStdout,
        pending_requests: Arc<Mutex<HashMap<String, oneshot::Sender<Option<PluginResponse>>>>>,
        pending_storage: Arc<Mutex<HashMap<u64, oneshot::Sender<StorageResult>>>>,
        plugin_id: String,
    ) {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<PluginMessage>(&line) {
                Ok(message) => {
                    match message {
                        PluginMessage::Log { level, message, args } => {
                            let entry = PluginLogEntry {
                                plugin_id: plugin_id.clone(),
                                level,
                                message,
                                timestamp: chrono::Utc::now().timestamp_millis(),
                                args,
                            };
                            Self::handle_log(entry);
                        }
                        PluginMessage::Response { request_id, response } => {
                            let mut pending = pending_requests.lock().await;
                            if let Some(tx) = pending.remove(&request_id) {
                                let _ = tx.send(response);
                            }
                        }
                        PluginMessage::ModifiedResponse { request_id, response } => {
                            let mut pending = pending_requests.lock().await;
                            if let Some(tx) = pending.remove(&format!("res_{}", request_id)) {
                                let _ = tx.send(Some(response));
                            }
                        }
                        PluginMessage::Storage { id, op: _ } => {
                            // Handle storage operation (would need access to storage)
                            let mut pending = pending_storage.lock().await;
                            if let Some(tx) = pending.remove(&id) {
                                let _ = tx.send(StorageResult {
                                    success: false,
                                    value: None,
                                    error: Some("Storage not implemented yet".into()),
                                });
                            }
                        }
                        PluginMessage::RegisterPanel { panel } => {
                            tracing::info!("Plugin {} registered panel: {}", plugin_id, panel.id);
                            // TODO: Emit to frontend
                        }
                        PluginMessage::UnregisterPanel { panel_id } => {
                            tracing::info!("Plugin {} unregistered panel: {}", plugin_id, panel_id);
                            // TODO: Emit to frontend
                        }
                        PluginMessage::Toast { message, toast_type } => {
                            tracing::info!("Plugin {} toast [{}]: {}", plugin_id, toast_type.unwrap_or_default(), message);
                            // TODO: Emit to frontend
                        }
                        PluginMessage::Loaded => {
                            tracing::info!("Plugin {} loaded successfully", plugin_id);
                        }
                        PluginMessage::Error { message } => {
                            tracing::error!("Plugin {} error: {}", plugin_id, message);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to parse plugin message: {} - {}", e, line);
                }
            }
        }
    }

    /// Handle a log entry from plugin
    fn handle_log(entry: PluginLogEntry) {
        match entry.level {
            LogLevel::Debug => tracing::debug!("[plugin:{}] {}", entry.plugin_id, entry.message),
            LogLevel::Info => tracing::info!("[plugin:{}] {}", entry.plugin_id, entry.message),
            LogLevel::Warn => tracing::warn!("[plugin:{}] {}", entry.plugin_id, entry.message),
            LogLevel::Error => tracing::error!("[plugin:{}] {}", entry.plugin_id, entry.message),
        }
    }

    /// Create the wrapper script that bootstraps plugins
    async fn create_wrapper_script(&self) -> Result<PathBuf> {
        // Get temp directory
        let temp_dir = std::env::temp_dir().join("postgate_plugins");
        tokio::fs::create_dir_all(&temp_dir).await
            .map_err(|e| PostGateError::Plugin(format!("Failed to create temp dir: {}", e)))?;

        let wrapper_path = temp_dir.join("plugin_wrapper.mjs");
        
        // Write wrapper script
        let wrapper_script = include_str!("plugin_wrapper.mjs");
        tokio::fs::write(&wrapper_path, wrapper_script).await
            .map_err(|e| PostGateError::Plugin(format!("Failed to write wrapper script: {}", e)))?;

        Ok(wrapper_path)
    }
}

impl Drop for PluginRuntime {
    fn drop(&mut self) {
        if let Some(ref mut process) = self.process {
            let _ = process.start_kill();
        }
    }
}
