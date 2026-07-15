use serde::{Deserialize, Serialize};
use tauri::{ipc::Channel, AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use time::format_description::well_known::Rfc3339;
use tokio::sync::Mutex;
use url::Url;

use crate::error::{PostGateError, Result};

const STABLE_ENDPOINT: &str =
    "https://github.com/backrunner/postgate/releases/latest/download/latest.json";
const BETA_ENDPOINT: &str =
    "https://github.com/backrunner/postgate/releases/download/beta/latest.json";

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReleaseChannel {
    Stable,
    Beta,
}

impl ReleaseChannel {
    fn endpoint(self) -> &'static str {
        match self {
            Self::Stable => STABLE_ENDPOINT,
            Self::Beta => BETA_ENDPOINT,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMetadata {
    version: String,
    date: Option<String>,
    body: Option<String>,
    channel: ReleaseChannel,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum DownloadEvent {
    #[serde(rename_all = "camelCase")]
    Started {
        content_length: Option<u64>,
    },
    #[serde(rename_all = "camelCase")]
    Progress {
        chunk_length: usize,
    },
    Finished,
}

struct PendingUpdate {
    update: Update,
    bytes: Option<Vec<u8>>,
}

#[derive(Default)]
struct ChannelUpdaterInner {
    generation: u64,
    pending: Option<PendingUpdate>,
}

#[derive(Default)]
pub struct ChannelUpdaterState {
    inner: Mutex<ChannelUpdaterInner>,
}

fn update_error(error: impl ToString) -> PostGateError {
    PostGateError::Update(error.to_string())
}

#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    state: State<'_, ChannelUpdaterState>,
    channel: ReleaseChannel,
) -> Result<Option<UpdateMetadata>> {
    let generation = {
        let mut inner = state.inner.lock().await;
        inner.generation = inner.generation.wrapping_add(1);
        inner.pending = None;
        inner.generation
    };
    let endpoint = Url::parse(channel.endpoint()).map_err(update_error)?;
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(update_error)?
        .build()
        .map_err(update_error)?;
    let update = updater.check().await.map_err(update_error)?;

    let metadata = update.as_ref().map(|update| UpdateMetadata {
        version: update.version.clone(),
        date: update.date.and_then(|date| date.format(&Rfc3339).ok()),
        body: update.body.clone(),
        channel,
    });

    let mut inner = state.inner.lock().await;
    if inner.generation != generation {
        return Ok(None);
    }
    inner.pending = update.map(|update| PendingUpdate {
        update,
        bytes: None,
    });

    Ok(metadata)
}

#[tauri::command]
pub async fn download_update(
    state: State<'_, ChannelUpdaterState>,
    on_event: Channel<DownloadEvent>,
) -> Result<()> {
    let (version, download_url, update) = {
        let inner = state.inner.lock().await;
        let pending = inner
            .pending
            .as_ref()
            .ok_or_else(|| PostGateError::InvalidState("No update is available".into()))?;
        if pending.bytes.is_some() {
            return Ok(());
        }
        (
            pending.update.version.clone(),
            pending.update.download_url.clone(),
            pending.update.clone(),
        )
    };

    let progress_channel = on_event.clone();
    let finished_channel = on_event;
    let mut started = false;
    let bytes = update
        .download(
            move |chunk_length, content_length| {
                if !started {
                    started = true;
                    let _ = progress_channel.send(DownloadEvent::Started { content_length });
                }
                let _ = progress_channel.send(DownloadEvent::Progress { chunk_length });
            },
            move || {
                let _ = finished_channel.send(DownloadEvent::Finished);
            },
        )
        .await
        .map_err(update_error)?;

    let mut inner = state.inner.lock().await;
    let current = inner
        .pending
        .as_mut()
        .filter(|pending| {
            pending.update.version == version && pending.update.download_url == download_url
        })
        .ok_or_else(|| {
            PostGateError::InvalidState(
                "The selected update channel changed during download".into(),
            )
        })?;
    current.bytes = Some(bytes);

    Ok(())
}

#[tauri::command]
pub async fn install_update(state: State<'_, ChannelUpdaterState>) -> Result<()> {
    let mut inner = state.inner.lock().await;
    let current = inner
        .pending
        .as_ref()
        .ok_or_else(|| PostGateError::InvalidState("No update is available".into()))?;
    let bytes = current.bytes.as_ref().ok_or_else(|| {
        PostGateError::InvalidState("Download the update before installing".into())
    })?;

    current.update.install(bytes).map_err(update_error)?;
    inner.pending = None;
    Ok(())
}

#[tauri::command]
pub async fn clear_pending_update(state: State<'_, ChannelUpdaterState>) -> Result<()> {
    let mut inner = state.inner.lock().await;
    inner.generation = inner.generation.wrapping_add(1);
    inner.pending = None;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_channels_use_independent_manifests() {
        assert_eq!(ReleaseChannel::Stable.endpoint(), STABLE_ENDPOINT);
        assert_eq!(ReleaseChannel::Beta.endpoint(), BETA_ENDPOINT);
        assert_ne!(
            ReleaseChannel::Stable.endpoint(),
            ReleaseChannel::Beta.endpoint()
        );
    }
}
