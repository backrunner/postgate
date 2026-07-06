use crate::error::{PostGateError, Result};
use crate::rules::{
    collect_external_include_files, parse_rules_with_external_includes,
    parse_rules_with_external_includes_and_deps, Rule, RuleAction, RuleGroup,
};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tauri::{Emitter, State};
use tokio::time::MissedTickBehavior;
use uuid::Uuid;

const EXTERNAL_RULE_WATCH_INTERVAL: Duration = Duration::from_secs(1);

/// Result of parsing rules
#[derive(Debug, Clone, Serialize)]
pub struct ParseResult {
    pub success: bool,
    pub rules: Vec<Rule>,
    pub errors: Vec<ParseError>,
    pub warnings: Vec<ParseError>,
}

/// A parse error with location info
#[derive(Debug, Clone, Serialize)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
    pub content: String,
}

/// Input for importing a Whistle-exported rules file.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhistleImportInput {
    pub path: String,
    #[serde(default)]
    pub group_name: Option<String>,
}

/// Get all rule groups
#[tauri::command]
pub async fn get_rule_groups(state: State<'_, Arc<AppState>>) -> Result<Vec<RuleGroup>> {
    // Check if in-memory engine is empty, if so, load from database
    let groups = state.rule_engine.get_all_groups();
    if groups.is_empty() {
        // Load from database
        let db = state.get_database().await?;
        let db_groups = db.get_rule_groups().await?;

        // Populate the in-memory engine
        for group in &db_groups {
            state.rule_engine.upsert_group(group.clone());
        }

        return Ok(db_groups);
    }

    Ok(groups)
}

/// Save a rule group
#[tauri::command]
pub async fn save_rule_group(
    group: RuleGroup,
    state: State<'_, Arc<AppState>>,
) -> Result<RuleGroup> {
    persist_rule_group(group, &state).await
}

/// Import a Whistle-exported rules file as a new rule group.
#[tauri::command]
pub async fn import_whistle_rules(
    input: WhistleImportInput,
    state: State<'_, Arc<AppState>>,
) -> Result<RuleGroup> {
    let path = Path::new(&input.path);
    let content = tokio::fs::read_to_string(path).await?;
    let raw_content = strip_utf8_bom(&content).to_string();

    if raw_content.trim().is_empty() {
        return Err(PostGateError::InvalidState(
            "Whistle rules file is empty".into(),
        ));
    }

    let group = RuleGroup {
        id: Uuid::new_v4().to_string(),
        name: input
            .group_name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| derive_whistle_group_name(path)),
        enabled: true,
        priority: 0,
        rules: Vec::new(),
        raw_content,
        created_at: 0,
        updated_at: 0,
        inline_values: Default::default(),
    };

    persist_rule_group(group, &state).await
}

async fn persist_rule_group(group: RuleGroup, state: &Arc<AppState>) -> Result<RuleGroup> {
    let now = chrono::Utc::now().timestamp_millis();

    // Parse rules + inline values from raw content.
    let (rules, inline_values) = parse_rules_with_external_includes(&group.raw_content, None)?;

    let group = RuleGroup {
        id: if group.id.is_empty() {
            Uuid::new_v4().to_string()
        } else {
            group.id
        },
        name: group.name,
        enabled: group.enabled,
        priority: group.priority,
        rules,
        raw_content: group.raw_content,
        created_at: if group.created_at == 0 {
            now
        } else {
            group.created_at
        },
        updated_at: now,
        inline_values,
    };

    // Update in-memory engine
    state.rule_engine.upsert_group(group.clone());

    // Persist to database
    let db = state.get_database().await?;
    db.save_rule_group(&group).await?;

    crate::rule_events::notify_rule_groups_changed(state).await;

    Ok(group)
}

fn strip_utf8_bom(content: &str) -> &str {
    content.strip_prefix('\u{feff}').unwrap_or(content)
}

fn derive_whistle_group_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|stem| format!("Whistle: {}", stem))
        .unwrap_or_else(|| "Whistle Import".to_string())
}

/// Delete a rule group
#[tauri::command]
pub async fn delete_rule_group(id: String, state: State<'_, Arc<AppState>>) -> Result<bool> {
    // Remove from in-memory engine
    let removed = state.rule_engine.remove_group(&id);

    // Delete from database
    let db = state.get_database().await?;
    let deleted = db.delete_rule_group(&id).await?;

    if deleted || removed.is_some() {
        crate::rule_events::notify_rule_groups_changed(&state).await;
    }

    Ok(deleted || removed.is_some())
}

/// Toggle a rule group's enabled state
#[tauri::command]
pub async fn toggle_rule_group(
    id: String,
    enabled: bool,
    state: State<'_, Arc<AppState>>,
) -> Result<bool> {
    // Toggle in memory
    let toggled = state.rule_engine.toggle_group(&id, enabled);

    if toggled {
        // Persist the updated state to database
        if let Some(group) = state.rule_engine.get_group(&id) {
            let db = state.get_database().await?;
            db.save_rule_group(&group).await?;
        }

        crate::rule_events::notify_rule_groups_changed(&state).await;
    }

    Ok(toggled)
}

/// Parse rules from text (returns success/errors for validation)
#[tauri::command]
pub async fn parse_rules(content: String) -> Result<ParseResult> {
    match parse_rules_with_external_includes_and_deps(&content, None) {
        Ok(parsed) => Ok(ParseResult {
            success: true,
            warnings: collect_parse_warnings(&content, &parsed.rules),
            rules: parsed.rules,
            errors: vec![],
        }),
        Err(e) => {
            // Extract line number from error if possible
            let error_msg = e.to_string();
            Ok(ParseResult {
                success: false,
                rules: vec![],
                errors: vec![ParseError {
                    line: 1, // Default to line 1 if we can't determine
                    message: error_msg,
                    content: String::new(),
                }],
                warnings: vec![],
            })
        }
    }
}

pub fn start_external_rule_file_watcher(state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let mut known_files: HashMap<std::path::PathBuf, Option<SystemTime>> = HashMap::new();
        let mut initialized = false;
        let mut ticker = tokio::time::interval(EXTERNAL_RULE_WATCH_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            ticker.tick().await;

            match refresh_external_rule_files(&state, &mut known_files, initialized).await {
                Ok(changed) => {
                    if changed {
                        tracing::info!("External rule files changed; refreshed rule engine");
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to refresh external rule files: {}", e);
                }
            }

            initialized = true;
        }
    });
}

async fn refresh_external_rule_files(
    state: &Arc<AppState>,
    known_files: &mut HashMap<std::path::PathBuf, Option<SystemTime>>,
    initialized: bool,
) -> Result<bool> {
    ensure_rule_groups_loaded(state).await?;

    let groups = state.rule_engine.get_all_groups();
    let current_files = external_rule_file_snapshot(&groups);

    if !initialized {
        *known_files = current_files;
        return Ok(false);
    }

    if *known_files == current_files {
        return Ok(false);
    }

    for mut group in groups {
        match parse_rules_with_external_includes(&group.raw_content, None) {
            Ok((rules, inline_values)) => {
                group.rules = rules;
                group.inline_values = inline_values;
                state.rule_engine.upsert_group(group);
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to refresh rule group from external files; keeping previous parsed rules: {}",
                    e
                );
            }
        }
    }

    *known_files = current_files;

    if let Err(e) = state.app_handle.emit("rules-external-files-changed", ()) {
        tracing::warn!("Failed to emit rules-external-files-changed: {}", e);
    }

    Ok(true)
}

async fn ensure_rule_groups_loaded(state: &Arc<AppState>) -> Result<()> {
    if !state.rule_engine.get_all_groups().is_empty() {
        return Ok(());
    }

    let db = state.get_database().await?;
    let groups = db.get_rule_groups().await?;
    for group in groups {
        state.rule_engine.upsert_group(group);
    }
    Ok(())
}

fn external_rule_file_snapshot(
    groups: &[RuleGroup],
) -> HashMap<std::path::PathBuf, Option<SystemTime>> {
    groups
        .iter()
        .flat_map(|group| collect_external_include_files(&group.raw_content, None))
        .map(|path| {
            let modified = std::fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .ok();
            (path, modified)
        })
        .collect()
}

fn collect_parse_warnings(content: &str, rules: &[Rule]) -> Vec<ParseError> {
    rules
        .iter()
        .flat_map(|rule| {
            rule.actions.iter().filter_map(move |action| {
                if let RuleAction::Unsupported { protocol, value } = action {
                    Some(ParseError {
                        line: find_rule_line(content, &rule.raw_line),
                        message: format!("Unsupported Whistle protocol: {}://{}", protocol, value),
                        content: rule.raw_line.clone(),
                    })
                } else {
                    None
                }
            })
        })
        .collect()
}

fn find_rule_line(content: &str, raw_line: &str) -> usize {
    let needle = raw_line.trim();
    content
        .lines()
        .position(|line| line.trim() == needle)
        .map(|index| index + 1)
        .unwrap_or(0)
}

/// Check if any enabled rule group has a debug:// action
#[tauri::command]
pub async fn has_active_debug_rules(state: State<'_, Arc<AppState>>) -> Result<bool> {
    Ok(state.rule_engine.has_active_debug_rules())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_parse_warnings_for_unsupported_protocols() {
        let content = "\nexample.com host://127.0.0.1\napi.example.com style://dark\n";
        let rules = parse_rules_with_external_includes(content, None).unwrap().0;

        let warnings = collect_parse_warnings(content, &rules);

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 3);
        assert_eq!(
            warnings[0].message,
            "Unsupported Whistle protocol: style://dark"
        );
        assert_eq!(warnings[0].content, "api.example.com style://dark");
    }
}
