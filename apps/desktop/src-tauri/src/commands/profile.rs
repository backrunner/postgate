use crate::cert::CertificateAuthority;
use crate::error::{PostGateError, Result};
use crate::replay::{Collection, SavedRequest};
use crate::rules::{parse_rules_with_external_includes, RuleGroup};
use crate::state::AppState;
use crate::values::ValueEntry;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::State;

const PROFILE_FORMAT: &str = "postgate.profile";
const PROFILE_VERSION: u16 = 1;
const SYNC_FILE_NAME: &str = "postgate-profile.json";
const SYNC_CONFIG_FILE_NAME: &str = "sync-config.json";
const ICLOUD_RELATIVE_PATH: &str = "Documents/PostGate";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileOptions {
    #[serde(default = "default_true")]
    pub include_rules: bool,
    #[serde(default = "default_true")]
    pub include_values: bool,
    #[serde(default = "default_true")]
    pub include_replay: bool,
    #[serde(default = "default_true")]
    pub include_certificate: bool,
    #[serde(default = "default_true")]
    pub include_app_settings: bool,
    #[serde(default = "default_true")]
    pub include_sync_settings: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOptions {
    #[serde(default)]
    pub profile_options: ProfileOptions,
    #[serde(default = "default_true")]
    pub replace_existing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileExportInput {
    pub path: String,
    #[serde(default)]
    pub app_settings: Option<AppSettings>,
    #[serde(default)]
    pub sync_settings: Option<SyncSettings>,
    #[serde(default)]
    pub options: ProfileOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileImportInput {
    pub path: String,
    #[serde(default)]
    pub options: ImportOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSnapshot {
    pub format: String,
    pub version: u16,
    pub exported_at: i64,
    #[serde(default)]
    pub app_settings: Option<AppSettings>,
    #[serde(default)]
    pub sync_settings: Option<SyncSettings>,
    #[serde(default)]
    pub rules: Vec<RuleGroupBackup>,
    #[serde(default)]
    pub values: Vec<ValueEntry>,
    #[serde(default)]
    pub replay: Option<ReplayBackup>,
    #[serde(default)]
    pub certificate: Option<CertificateBackup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleGroupBackup {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub folder: Option<String>,
    pub enabled: bool,
    pub priority: i32,
    pub raw_content: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReplayBackup {
    #[serde(default)]
    pub collections: Vec<Collection>,
    #[serde(default)]
    pub saved_requests: Vec<SavedRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateBackup {
    pub cert_pem: String,
    pub key_pem: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default)]
    pub proxy: Option<ProxySettings>,
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub columns: Option<serde_json::Value>,
    #[serde(default)]
    pub updates: Option<UpdateSettings>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxySettings {
    pub port: u16,
    #[serde(default = "default_true")]
    pub enable_http2: bool,
    #[serde(default)]
    pub enable_quic: bool,
    #[serde(default)]
    pub quic_port: Option<u16>,
    #[serde(default = "default_debug_port")]
    pub debug_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettings {
    pub auto_check: bool,
    pub auto_download: bool,
    #[serde(default)]
    pub channel: Option<crate::commands::updater::ReleaseChannel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncSettings {
    pub enabled: bool,
    pub provider: SyncProvider,
    #[serde(default)]
    pub remote_path: Option<String>,
    #[serde(default)]
    pub webdav: Option<WebDavSettings>,
    #[serde(default)]
    pub cloudkit_change_tag: Option<String>,
    #[serde(default)]
    pub last_synced_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SyncProvider {
    #[default]
    Cloudkit,
    Icloud,
    Webdav,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WebDavSettings {
    pub endpoint: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummary {
    pub exported_at: i64,
    pub rule_groups: usize,
    pub values: usize,
    pub collections: usize,
    pub saved_requests: usize,
    pub includes_certificate: bool,
    pub includes_app_settings: bool,
    pub includes_sync_settings: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub summary: ProfileSummary,
    #[serde(default)]
    pub app_settings: Option<AppSettings>,
    #[serde(default)]
    pub sync_settings: Option<SyncSettings>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub config: SyncSettings,
    pub local_path: String,
    pub remote_available: bool,
    pub remote_change_tag: Option<String>,
}

struct RemoteSyncState {
    available: bool,
    change_tag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPullResult {
    pub import_result: ImportResult,
    pub path: String,
}

struct RemoteSnapshot {
    snapshot: ProfileSnapshot,
    change_tag: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_debug_port() -> u16 {
    9229
}

impl Default for ProfileOptions {
    fn default() -> Self {
        Self {
            include_rules: true,
            include_values: true,
            include_replay: true,
            include_certificate: true,
            include_app_settings: true,
            include_sync_settings: true,
        }
    }
}

impl ProfileOptions {
    fn for_sync() -> Self {
        Self {
            include_certificate: false,
            include_sync_settings: false,
            ..Self::default()
        }
    }
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            profile_options: ProfileOptions::default(),
            replace_existing: true,
        }
    }
}

#[tauri::command]
pub async fn export_profile(
    input: ProfileExportInput,
    state: State<'_, Arc<AppState>>,
) -> Result<ProfileSummary> {
    let snapshot = build_snapshot(
        &state,
        input.options,
        input.app_settings,
        input.sync_settings,
    )
    .await?;
    write_snapshot(Path::new(&input.path), &snapshot).await?;
    Ok(snapshot_summary(&snapshot))
}

#[tauri::command]
pub async fn inspect_profile(path: String) -> Result<ProfileSummary> {
    let snapshot = read_snapshot(Path::new(&path)).await?;
    Ok(snapshot_summary(&snapshot))
}

#[tauri::command]
pub async fn import_profile(
    input: ProfileImportInput,
    state: State<'_, Arc<AppState>>,
) -> Result<ImportResult> {
    let snapshot = read_snapshot(Path::new(&input.path)).await?;
    apply_snapshot(&state, &snapshot, &input.options).await
}

#[tauri::command]
pub async fn get_sync_status(state: State<'_, Arc<AppState>>) -> Result<SyncStatus> {
    let config = read_sync_config(&state).await?.unwrap_or_default();
    let local_path = sync_location(&state, &config)?;
    let remote = if config.enabled {
        sync_remote_state(&config, &local_path).await?
    } else {
        RemoteSyncState {
            available: false,
            change_tag: None,
        }
    };

    Ok(SyncStatus {
        config,
        local_path,
        remote_available: remote.available,
        remote_change_tag: remote.change_tag,
    })
}

#[tauri::command]
pub async fn save_sync_settings(
    settings: SyncSettings,
    state: State<'_, Arc<AppState>>,
) -> Result<SyncStatus> {
    if settings.enabled {
        validate_sync_provider_available(&settings)?;
    }
    validate_sync_settings(&settings)?;
    let local_path = sync_location(&state, &settings)?;
    let remote = if settings.enabled {
        sync_remote_state(&settings, &local_path).await?
    } else {
        RemoteSyncState {
            available: false,
            change_tag: None,
        }
    };
    write_sync_config(&state, &settings).await?;

    Ok(SyncStatus {
        config: settings,
        local_path,
        remote_available: remote.available,
        remote_change_tag: remote.change_tag,
    })
}

#[tauri::command]
pub async fn push_sync_profile(
    app_settings: Option<AppSettings>,
    state: State<'_, Arc<AppState>>,
) -> Result<ProfileSummary> {
    let mut sync_settings = read_sync_config(&state).await?.unwrap_or_default();
    validate_sync_enabled(&sync_settings)?;
    validate_sync_provider_available(&sync_settings)?;
    validate_sync_settings(&sync_settings)?;

    let snapshot = build_snapshot(&state, ProfileOptions::for_sync(), app_settings, None).await?;
    let path = sync_location(&state, &sync_settings)?;
    let change_tag = write_sync_snapshot(&sync_settings, &path, &snapshot).await?;
    if change_tag.is_some() {
        sync_settings.cloudkit_change_tag = change_tag;
    }
    sync_settings.last_synced_at = Some(chrono::Utc::now().timestamp_millis());
    write_sync_config(&state, &sync_settings).await?;
    Ok(snapshot_summary(&snapshot))
}

#[tauri::command]
pub async fn pull_sync_profile(state: State<'_, Arc<AppState>>) -> Result<SyncPullResult> {
    let mut sync_settings = read_sync_config(&state).await?.unwrap_or_default();
    validate_sync_enabled(&sync_settings)?;
    validate_sync_provider_available(&sync_settings)?;
    validate_sync_settings(&sync_settings)?;
    let path = sync_location(&state, &sync_settings)?;
    let remote = read_sync_snapshot(&sync_settings, &path).await?;

    let options = ImportOptions {
        profile_options: ProfileOptions::for_sync(),
        replace_existing: true,
    };
    let import_result = apply_snapshot(&state, &remote.snapshot, &options).await?;
    if remote.change_tag.is_some() {
        sync_settings.cloudkit_change_tag = remote.change_tag;
    }
    sync_settings.last_synced_at = Some(chrono::Utc::now().timestamp_millis());
    write_sync_config(&state, &sync_settings).await?;

    Ok(SyncPullResult {
        import_result,
        path,
    })
}

async fn build_snapshot(
    state: &Arc<AppState>,
    options: ProfileOptions,
    app_settings: Option<AppSettings>,
    sync_settings: Option<SyncSettings>,
) -> Result<ProfileSnapshot> {
    let db = state.get_database().await?;

    let rules = if options.include_rules {
        db.get_rule_groups()
            .await?
            .into_iter()
            .map(RuleGroupBackup::from)
            .collect()
    } else {
        Vec::new()
    };

    let values = if options.include_values {
        state.ensure_values_loaded().await?;
        db.list_values().await?
    } else {
        Vec::new()
    };

    let replay = if options.include_replay {
        Some(ReplayBackup {
            collections: db.get_collections().await?,
            saved_requests: db.get_saved_requests().await?,
        })
    } else {
        None
    };

    let certificate = if options.include_certificate {
        let ca = state.get_or_init_ca()?;
        Some(CertificateBackup {
            cert_pem: ca.get_ca_pem().to_string(),
            key_pem: ca.get_ca_key_pem(),
        })
    } else {
        None
    };

    Ok(ProfileSnapshot {
        format: PROFILE_FORMAT.to_string(),
        version: PROFILE_VERSION,
        exported_at: chrono::Utc::now().timestamp_millis(),
        app_settings: options
            .include_app_settings
            .then_some(app_settings)
            .flatten(),
        sync_settings: options
            .include_sync_settings
            .then_some(sync_settings)
            .flatten(),
        rules,
        values,
        replay,
        certificate,
    })
}

async fn apply_snapshot(
    state: &Arc<AppState>,
    snapshot: &ProfileSnapshot,
    options: &ImportOptions,
) -> Result<ImportResult> {
    validate_snapshot(snapshot)?;
    let db = state.get_database().await?;
    let profile_options = &options.profile_options;

    let restored_groups = if profile_options.include_rules {
        Some(
            snapshot
                .rules
                .iter()
                .map(RuleGroupBackup::to_rule_group)
                .collect::<Result<Vec<_>>>()?,
        )
    } else {
        None
    };
    if profile_options.include_values && !options.replace_existing {
        state.ensure_values_loaded().await?;
    }
    let replay = profile_options
        .include_replay
        .then_some(snapshot.replay.as_ref())
        .flatten();

    db.restore_profile_data(
        restored_groups.as_deref(),
        profile_options
            .include_values
            .then_some(snapshot.values.as_slice()),
        replay.map(|value| value.collections.as_slice()),
        replay.map(|value| value.saved_requests.as_slice()),
        options.replace_existing,
    )
    .await?;

    if let Some(restored_groups) = restored_groups {
        if options.replace_existing {
            for group in state.rule_engine.get_all_groups() {
                state.rule_engine.remove_group(&group.id);
            }
        }
        for group in restored_groups {
            state.rule_engine.upsert_group(group);
        }
        crate::rule_events::notify_rule_groups_changed(state).await;
    }

    if profile_options.include_values {
        if options.replace_existing {
            state.values_store.clear();
        }
        for value in &snapshot.values {
            state
                .values_store
                .insert(value.name.clone(), value.content.clone());
        }
    }

    if profile_options.include_certificate {
        if let Some(certificate) = &snapshot.certificate {
            let ca =
                CertificateAuthority::load_from_pem(&certificate.cert_pem, &certificate.key_pem)?;
            ca.save_to_files(&state.data_dir)?;
            let mut ca_guard = state.ca.write();
            *ca_guard = Some(ca);
        }
    }

    if profile_options.include_sync_settings {
        if let Some(sync_settings) = &snapshot.sync_settings {
            write_sync_config(state, sync_settings).await?;
        }
    }

    Ok(ImportResult {
        summary: snapshot_summary(snapshot),
        app_settings: profile_options
            .include_app_settings
            .then_some(snapshot.app_settings.clone())
            .flatten(),
        sync_settings: profile_options
            .include_sync_settings
            .then_some(snapshot.sync_settings.clone())
            .flatten(),
    })
}

fn validate_snapshot(snapshot: &ProfileSnapshot) -> Result<()> {
    if snapshot.format != PROFILE_FORMAT {
        return Err(PostGateError::InvalidState(format!(
            "Unsupported profile format '{}'",
            snapshot.format
        )));
    }

    if snapshot.version > PROFILE_VERSION {
        return Err(PostGateError::InvalidState(format!(
            "Profile version {} is newer than this app supports",
            snapshot.version
        )));
    }

    if let Some(proxy) = snapshot
        .app_settings
        .as_ref()
        .and_then(|settings| settings.proxy.as_ref())
    {
        if proxy.port == 0 || proxy.debug_port == 0 || proxy.quic_port == Some(0) {
            return Err(PostGateError::InvalidState(
                "Profile contains an invalid proxy port".into(),
            ));
        }
    }

    if let Some(sync_settings) = &snapshot.sync_settings {
        validate_sync_settings(sync_settings)?;
    }

    if let Some(certificate) = &snapshot.certificate {
        CertificateAuthority::load_from_pem(&certificate.cert_pem, &certificate.key_pem)?;
    }

    Ok(())
}

async fn read_snapshot(path: &Path) -> Result<ProfileSnapshot> {
    let content = tokio::fs::read_to_string(path).await?;
    let snapshot: ProfileSnapshot = serde_json::from_str(&content)?;
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

async fn write_snapshot(path: &Path, snapshot: &ProfileSnapshot) -> Result<()> {
    let content = serde_json::to_string_pretty(snapshot)?;
    write_restricted_file_atomically(path, content.as_bytes()).await
}

fn snapshot_summary(snapshot: &ProfileSnapshot) -> ProfileSummary {
    let (collections, saved_requests) = snapshot
        .replay
        .as_ref()
        .map(|replay| (replay.collections.len(), replay.saved_requests.len()))
        .unwrap_or((0, 0));

    ProfileSummary {
        exported_at: snapshot.exported_at,
        rule_groups: snapshot.rules.len(),
        values: snapshot.values.len(),
        collections,
        saved_requests,
        includes_certificate: snapshot.certificate.is_some(),
        includes_app_settings: snapshot.app_settings.is_some(),
        includes_sync_settings: snapshot.sync_settings.is_some(),
    }
}

async fn read_sync_config(state: &Arc<AppState>) -> Result<Option<SyncSettings>> {
    let path = sync_config_path(state)?;
    if !path.exists() {
        return Ok(None);
    }

    let content = tokio::fs::read_to_string(path).await?;
    Ok(Some(serde_json::from_str(&content)?))
}

async fn write_sync_config(state: &Arc<AppState>, settings: &SyncSettings) -> Result<()> {
    validate_sync_settings(settings)?;
    let path = sync_config_path(state)?;
    let content = serde_json::to_vec_pretty(settings)?;
    write_restricted_file_atomically(&path, &content).await
}

async fn write_restricted_file_atomically(path: &Path, content: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    tokio::fs::create_dir_all(parent).await?;
    let temporary = tempfile::Builder::new()
        .prefix(".postgate-write-")
        .tempfile_in(parent)?
        .into_temp_path();
    tokio::fs::write(&temporary, content).await?;
    restrict_file_permissions(&temporary).await?;
    tokio::fs::OpenOptions::new()
        .write(true)
        .open(&temporary)
        .await?
        .sync_all()
        .await?;
    temporary.persist(path).map_err(|error| error.error)?;

    #[cfg(unix)]
    std::fs::File::open(parent)?.sync_all()?;

    Ok(())
}

fn sync_config_path(state: &Arc<AppState>) -> Result<PathBuf> {
    Ok(state.data_dir.join(SYNC_CONFIG_FILE_NAME))
}

fn sync_location(state: &Arc<AppState>, settings: &SyncSettings) -> Result<String> {
    match settings.provider {
        SyncProvider::Cloudkit => Ok(cloudkit_location().to_string()),
        SyncProvider::Icloud => {
            let base = settings
                .remote_path
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(PathBuf::from)
                .or_else(default_icloud_directory)
                .unwrap_or_else(|| state.data_dir.join("sync"));
            Ok(base.join(SYNC_FILE_NAME).to_string_lossy().to_string())
        }
        SyncProvider::Webdav => {
            let webdav = settings.webdav.as_ref().ok_or_else(|| {
                PostGateError::InvalidState("WebDAV settings are required".into())
            })?;
            Ok(webdav_snapshot_url(
                &webdav.endpoint,
                settings.remote_path.as_deref(),
            ))
        }
    }
}

fn validate_sync_settings(settings: &SyncSettings) -> Result<()> {
    if settings.enabled && settings.provider == SyncProvider::Webdav {
        let webdav = settings
            .webdav
            .as_ref()
            .ok_or_else(|| PostGateError::InvalidState("WebDAV settings are required".into()))?;
        if webdav.endpoint.trim().is_empty() {
            return Err(PostGateError::InvalidState(
                "WebDAV endpoint is required".into(),
            ));
        }
        let parsed = url::Url::parse(webdav.endpoint.trim())
            .map_err(|e| PostGateError::InvalidState(format!("Invalid WebDAV endpoint: {}", e)))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(PostGateError::InvalidState(
                "WebDAV endpoint must start with http:// or https://".into(),
            ));
        }
    }

    Ok(())
}

fn validate_sync_enabled(settings: &SyncSettings) -> Result<()> {
    if !settings.enabled {
        return Err(PostGateError::InvalidState("Sync is disabled".into()));
    }
    Ok(())
}

fn validate_sync_provider_available(settings: &SyncSettings) -> Result<()> {
    if matches!(
        settings.provider,
        SyncProvider::Cloudkit | SyncProvider::Icloud
    ) && !cfg!(target_os = "macos")
    {
        return Err(PostGateError::InvalidState(
            "CloudKit and iCloud Drive sync are only available on macOS".into(),
        ));
    }
    Ok(())
}

#[cfg(unix)]
async fn restrict_file_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await?;
    Ok(())
}

#[cfg(not(unix))]
async fn restrict_file_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

async fn sync_remote_state(settings: &SyncSettings, location: &str) -> Result<RemoteSyncState> {
    match settings.provider {
        SyncProvider::Cloudkit => {
            let change_tag = cloudkit_snapshot_change_tag().await?;
            Ok(RemoteSyncState {
                available: change_tag.is_some(),
                change_tag,
            })
        }
        SyncProvider::Icloud => Ok(RemoteSyncState {
            available: Path::new(location).exists(),
            change_tag: None,
        }),
        SyncProvider::Webdav => {
            let response = webdav_request(settings, Method::HEAD, location)?
                .send()
                .await
                .map_err(|e| PostGateError::Storage(format!("WebDAV check failed: {}", e)))?;

            if response.status().is_success() {
                Ok(RemoteSyncState {
                    available: true,
                    change_tag: None,
                })
            } else if response.status().as_u16() == 404 {
                Ok(RemoteSyncState {
                    available: false,
                    change_tag: None,
                })
            } else {
                Err(PostGateError::Storage(format!(
                    "WebDAV check failed with status {}",
                    response.status()
                )))
            }
        }
    }
}

async fn read_sync_snapshot(settings: &SyncSettings, location: &str) -> Result<RemoteSnapshot> {
    match settings.provider {
        SyncProvider::Cloudkit => read_cloudkit_snapshot().await,
        SyncProvider::Icloud => Ok(RemoteSnapshot {
            snapshot: read_snapshot(Path::new(location)).await?,
            change_tag: None,
        }),
        SyncProvider::Webdav => {
            let response = webdav_request(settings, Method::GET, location)?
                .send()
                .await
                .map_err(|e| PostGateError::Storage(format!("WebDAV download failed: {}", e)))?;
            if !response.status().is_success() {
                return Err(PostGateError::Storage(format!(
                    "WebDAV download failed with status {}",
                    response.status()
                )));
            }
            let content = response.text().await.map_err(|e| {
                PostGateError::Storage(format!("Failed to read WebDAV response: {}", e))
            })?;
            let snapshot: ProfileSnapshot = serde_json::from_str(&content)?;
            validate_snapshot(&snapshot)?;
            Ok(RemoteSnapshot {
                snapshot,
                change_tag: None,
            })
        }
    }
}

async fn write_sync_snapshot(
    settings: &SyncSettings,
    location: &str,
    snapshot: &ProfileSnapshot,
) -> Result<Option<String>> {
    match settings.provider {
        SyncProvider::Cloudkit => {
            write_cloudkit_snapshot(snapshot, settings.cloudkit_change_tag.clone())
                .await
                .map(Some)
        }
        SyncProvider::Icloud => {
            write_snapshot(Path::new(location), snapshot).await?;
            Ok(None)
        }
        SyncProvider::Webdav => {
            let content = serde_json::to_string_pretty(snapshot)?;
            ensure_webdav_collections(settings).await?;
            let response = webdav_request(settings, Method::PUT, location)?
                .header("content-type", "application/json")
                .body(content)
                .send()
                .await
                .map_err(|e| PostGateError::Storage(format!("WebDAV upload failed: {}", e)))?;

            if !response.status().is_success() {
                return Err(PostGateError::Storage(format!(
                    "WebDAV upload failed with status {}",
                    response.status()
                )));
            }

            Ok(None)
        }
    }
}

fn cloudkit_location() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        crate::cloudkit_sync::LOCATION
    }
    #[cfg(not(target_os = "macos"))]
    {
        "cloudkit://unavailable"
    }
}

async fn cloudkit_snapshot_change_tag() -> Result<Option<String>> {
    #[cfg(target_os = "macos")]
    {
        crate::cloudkit_sync::change_tag().await
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(PostGateError::InvalidState(
            "CloudKit sync is only available on macOS".into(),
        ))
    }
}

async fn read_cloudkit_snapshot() -> Result<RemoteSnapshot> {
    #[cfg(target_os = "macos")]
    {
        let remote = crate::cloudkit_sync::pull().await?;
        let snapshot: ProfileSnapshot = serde_json::from_slice(&remote.payload)?;
        validate_snapshot(&snapshot)?;
        Ok(RemoteSnapshot {
            snapshot,
            change_tag: Some(remote.change_tag),
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(PostGateError::InvalidState(
            "CloudKit sync is only available on macOS".into(),
        ))
    }
}

async fn write_cloudkit_snapshot(
    snapshot: &ProfileSnapshot,
    expected_change_tag: Option<String>,
) -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        let payload = serde_json::to_vec_pretty(snapshot)?;
        crate::cloudkit_sync::push(payload, expected_change_tag).await
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (snapshot, expected_change_tag);
        Err(PostGateError::InvalidState(
            "CloudKit sync is only available on macOS".into(),
        ))
    }
}

async fn ensure_webdav_collections(settings: &SyncSettings) -> Result<()> {
    if settings.provider != SyncProvider::Webdav {
        return Ok(());
    }

    let webdav = settings
        .webdav
        .as_ref()
        .ok_or_else(|| PostGateError::InvalidState("WebDAV settings are required".into()))?;
    let mkcol = Method::from_bytes(b"MKCOL")
        .map_err(|e| PostGateError::Storage(format!("Invalid WebDAV method: {}", e)))?;

    for url in webdav_collection_urls(&webdav.endpoint, settings.remote_path.as_deref()) {
        let response = webdav_request(settings, mkcol.clone(), &url)?
            .send()
            .await
            .map_err(|e| PostGateError::Storage(format!("WebDAV MKCOL failed: {}", e)))?;
        let code = response.status().as_u16();
        if !(response.status().is_success() || matches!(code, 405 | 409)) {
            return Err(PostGateError::Storage(format!(
                "WebDAV MKCOL failed for {} with status {}",
                url,
                response.status()
            )));
        }
    }

    Ok(())
}

fn webdav_request(
    settings: &SyncSettings,
    method: Method,
    url: &str,
) -> Result<reqwest::RequestBuilder> {
    let client = reqwest::Client::new();
    let builder = client.request(method, url);

    if let Some(webdav) = &settings.webdav {
        if !webdav.username.is_empty() || !webdav.password.is_empty() {
            return Ok(builder.basic_auth(&webdav.username, Some(&webdav.password)));
        }
    }

    Ok(builder)
}

fn webdav_snapshot_url(endpoint: &str, remote_path: Option<&str>) -> String {
    let mut base = endpoint.trim().trim_end_matches('/').to_string();
    let path = remote_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(SYNC_FILE_NAME)
        .trim_start_matches('/');

    if path.ends_with(".json") {
        append_encoded_webdav_path(&mut base, path);
    } else {
        append_encoded_webdav_path(&mut base, path.trim_end_matches('/'));
        base.push('/');
        base.push_str(SYNC_FILE_NAME);
    }

    base
}

fn webdav_collection_urls(endpoint: &str, remote_path: Option<&str>) -> Vec<String> {
    let path = remote_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("");
    if path.is_empty() {
        return Vec::new();
    }

    let mut segments: Vec<&str> = path
        .trim_start_matches('/')
        .trim_end_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments
        .last()
        .is_some_and(|segment| segment.ends_with(".json"))
    {
        segments.pop();
    }

    let mut current = endpoint.trim().trim_end_matches('/').to_string();
    segments
        .into_iter()
        .map(|segment| {
            current.push('/');
            append_encoded_webdav_segment(&mut current, segment);
            current.clone()
        })
        .collect()
}

fn append_encoded_webdav_path(base: &mut String, path: &str) {
    for segment in path.split('/').filter(|segment| !segment.is_empty()) {
        base.push('/');
        append_encoded_webdav_segment(base, segment);
    }
}

fn append_encoded_webdav_segment(base: &mut String, segment: &str) {
    let encoded = urlencoding::encode(segment);
    base.push_str(encoded.as_ref());
}

fn default_icloud_directory() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from).map(|home| {
        home.join("Library/Mobile Documents/com~apple~CloudDocs")
            .join(ICLOUD_RELATIVE_PATH)
    })
}

impl From<RuleGroup> for RuleGroupBackup {
    fn from(group: RuleGroup) -> Self {
        Self {
            id: group.id,
            name: group.name,
            folder: group.folder,
            enabled: group.enabled,
            priority: group.priority,
            raw_content: group.raw_content,
            created_at: group.created_at,
            updated_at: group.updated_at,
        }
    }
}

impl RuleGroupBackup {
    fn to_rule_group(&self) -> Result<RuleGroup> {
        let (rules, inline_values) = parse_rules_with_external_includes(&self.raw_content, None)?;
        Ok(RuleGroup {
            id: self.id.clone(),
            name: self.name.clone(),
            folder: self.folder.clone(),
            enabled: self.enabled,
            priority: self.priority,
            rules,
            raw_content: self.raw_content.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            inline_values,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn webdav_settings(enabled: bool, endpoint: &str) -> SyncSettings {
        SyncSettings {
            enabled,
            provider: SyncProvider::Webdav,
            remote_path: None,
            webdav: Some(WebDavSettings {
                endpoint: endpoint.to_string(),
                username: String::new(),
                password: String::new(),
            }),
            cloudkit_change_tag: None,
            last_synced_at: None,
        }
    }

    fn empty_snapshot() -> ProfileSnapshot {
        ProfileSnapshot {
            format: PROFILE_FORMAT.to_string(),
            version: PROFILE_VERSION,
            exported_at: 0,
            app_settings: None,
            sync_settings: None,
            rules: Vec::new(),
            values: Vec::new(),
            replay: None,
            certificate: None,
        }
    }

    #[test]
    fn disabled_webdav_does_not_require_an_endpoint() {
        assert!(validate_sync_settings(&webdav_settings(false, "")).is_ok());
    }

    #[test]
    fn enabled_webdav_requires_an_http_endpoint() {
        assert!(validate_sync_settings(&webdav_settings(true, "")).is_err());
        assert!(validate_sync_settings(&webdav_settings(true, "ftp://example.com")).is_err());
        assert!(validate_sync_settings(&webdav_settings(true, "https://dav.example.com")).is_ok());
    }

    #[test]
    fn disabled_sync_cannot_push_or_pull() {
        assert!(validate_sync_enabled(&SyncSettings::default()).is_err());
    }

    #[test]
    fn sync_profiles_exclude_credentials_and_certificate_keys() {
        let options = ProfileOptions::for_sync();
        assert!(options.include_rules);
        assert!(options.include_values);
        assert!(options.include_replay);
        assert!(options.include_app_settings);
        assert!(!options.include_certificate);
        assert!(!options.include_sync_settings);
    }

    #[test]
    fn cloudkit_is_the_default_sync_provider() {
        let value = serde_json::to_value(SyncSettings::default()).expect("sync settings");
        assert_eq!(value["provider"], "cloudkit");
    }

    #[test]
    fn profile_rejects_zero_proxy_ports() {
        let mut snapshot = empty_snapshot();
        snapshot.app_settings = Some(AppSettings {
            proxy: Some(ProxySettings {
                port: 0,
                enable_http2: true,
                enable_quic: false,
                quic_port: None,
                debug_port: 9229,
            }),
            ..AppSettings::default()
        });

        assert!(validate_snapshot(&snapshot).is_err());
    }

    #[tokio::test]
    async fn restricted_writes_atomically_replace_existing_files() {
        let root = tempfile::tempdir().expect("temp directory");
        let path = root.path().join("sync-config.json");
        std::fs::write(&path, b"old").expect("existing file");

        write_restricted_file_atomically(&path, b"new")
            .await
            .expect("atomic write");

        assert_eq!(std::fs::read(&path).expect("written file"), b"new");
        assert!(root
            .path()
            .read_dir()
            .expect("directory")
            .all(|entry| !entry
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .starts_with(".postgate-write-")));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path)
                    .expect("metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn icloud_sync_is_rejected_outside_macos() {
        let settings = SyncSettings {
            enabled: true,
            provider: SyncProvider::Icloud,
            ..SyncSettings::default()
        };
        assert!(validate_sync_provider_available(&settings).is_err());
    }
}
