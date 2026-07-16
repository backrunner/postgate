use crate::error::{PostGateError, Result};
use crate::rules::{
    collect_external_include_files, parse_rules_with_external_includes,
    parse_rules_with_external_includes_and_deps, Rule, RuleAction, RuleGroup,
};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhistleImportResult {
    pub groups: Vec<RuleGroup>,
    pub rule_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WhistleGroupDraft {
    name: String,
    folder: Option<String>,
    enabled: bool,
    raw_content: String,
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

/// Import a Whistle-exported rules file while preserving file names, folders,
/// order, enabled state, and raw rule content.
#[tauri::command]
pub async fn import_whistle_rules(
    input: WhistleImportInput,
    state: State<'_, Arc<AppState>>,
) -> Result<WhistleImportResult> {
    let path = Path::new(&input.path);
    let content = tokio::fs::read_to_string(path).await?;
    let raw_content = strip_utf8_bom(&content).to_string();

    if raw_content.trim().is_empty() {
        return Err(PostGateError::InvalidState(
            "Whistle rules file is empty".into(),
        ));
    }

    let fallback_name = input
        .group_name
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| derive_whistle_group_name(path));
    let drafts = parse_whistle_import_document(&raw_content, fallback_name)?;

    let db = state.get_database().await?;
    let next_priority = db
        .get_rule_groups()
        .await?
        .iter()
        .map(|group| group.priority)
        .max()
        .unwrap_or(-1)
        .saturating_add(1);
    let now = chrono::Utc::now().timestamp_millis();
    let groups = drafts
        .into_iter()
        .enumerate()
        .map(|(index, draft)| {
            prepare_rule_group(
                RuleGroup {
                    id: Uuid::new_v4().to_string(),
                    name: draft.name,
                    folder: draft.folder,
                    enabled: draft.enabled,
                    priority: next_priority.saturating_add(index as i32),
                    rules: Vec::new(),
                    raw_content: draft.raw_content,
                    created_at: now,
                    updated_at: now,
                    inline_values: Default::default(),
                },
                now,
            )
        })
        .collect::<Result<Vec<_>>>()?;

    for group in &groups {
        db.save_rule_group(group).await?;
        state.rule_engine.upsert_group(group.clone());
    }
    crate::rule_events::notify_rule_groups_changed(&state).await;

    Ok(WhistleImportResult {
        rule_count: groups.iter().map(|group| group.rules.len()).sum(),
        groups,
    })
}

async fn persist_rule_group(group: RuleGroup, state: &Arc<AppState>) -> Result<RuleGroup> {
    let now = chrono::Utc::now().timestamp_millis();
    let group = prepare_rule_group(group, now)?;

    // Update in-memory engine
    state.rule_engine.upsert_group(group.clone());

    // Persist to database
    let db = state.get_database().await?;
    db.save_rule_group(&group).await?;

    crate::rule_events::notify_rule_groups_changed(state).await;

    Ok(group)
}

fn prepare_rule_group(group: RuleGroup, now: i64) -> Result<RuleGroup> {
    // Parse rules + inline values from raw content.
    let (rules, inline_values) = parse_rules_with_external_includes(&group.raw_content, None)?;

    Ok(RuleGroup {
        id: if group.id.is_empty() {
            Uuid::new_v4().to_string()
        } else {
            group.id
        },
        name: group.name,
        folder: group.folder,
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
    })
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

fn parse_whistle_import_document(
    raw_content: &str,
    fallback_name: String,
) -> Result<Vec<WhistleGroupDraft>> {
    let trimmed = raw_content.trim();
    let parsed = match serde_json::from_str::<Value>(trimmed) {
        Ok(value) => value,
        Err(_) => {
            return Ok(vec![WhistleGroupDraft {
                name: fallback_name,
                folder: None,
                enabled: true,
                raw_content: raw_content.to_string(),
            }]);
        }
    };

    match parsed {
        Value::Object(entries) => {
            let groups = parse_whistle_export_object(&entries);
            if groups.is_empty() {
                Err(PostGateError::InvalidState(
                    "Whistle export does not contain any rule files".into(),
                ))
            } else {
                Ok(groups)
            }
        }
        Value::Array(lines) => Ok(vec![WhistleGroupDraft {
            name: fallback_name,
            folder: None,
            enabled: true,
            raw_content: join_whistle_rule_lines(&lines),
        }]),
        Value::String(content) => Ok(vec![WhistleGroupDraft {
            name: fallback_name,
            folder: None,
            enabled: true,
            raw_content: content,
        }]),
        _ => Err(PostGateError::InvalidState(
            "Unsupported Whistle rules export format".into(),
        )),
    }
}

fn parse_whistle_export_object(entries: &Map<String, Value>) -> Vec<WhistleGroupDraft> {
    let mut current_folder = None;
    let mut groups = Vec::new();

    for name in whistle_export_order(entries) {
        if let Some(folder) = name.strip_prefix('\r') {
            current_folder = Some(folder.trim().to_string()).filter(|folder| !folder.is_empty());
            continue;
        }

        let Some(value) = entries.get(&name) else {
            continue;
        };
        let Some((raw_content, enabled)) = whistle_rule_value(value) else {
            continue;
        };
        groups.push(WhistleGroupDraft {
            folder: if name == "Default" {
                None
            } else {
                current_folder.clone()
            },
            name,
            enabled: enabled.unwrap_or(true),
            raw_content,
        });
    }

    groups
}

fn whistle_export_order(entries: &Map<String, Value>) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = HashSet::new();

    let ordered = entries.get("").and_then(|value| match value {
        Value::Array(list) => Some(list),
        Value::Object(value) => value.get("list").and_then(Value::as_array),
        _ => None,
    });
    if let Some(ordered) = ordered {
        for name in ordered.iter().filter_map(Value::as_str) {
            if entries.contains_key(name) && seen.insert(name.to_string()) {
                names.push(name.to_string());
            }
        }
    }

    for name in entries.keys().filter(|name| !name.is_empty()) {
        if seen.insert(name.clone()) {
            names.push(name.clone());
        }
    }
    names
}

fn whistle_rule_value(value: &Value) -> Option<(String, Option<bool>)> {
    match value {
        Value::String(content) => Some((content.clone(), None)),
        Value::Array(lines) => Some((join_whistle_rule_lines(lines), None)),
        Value::Object(item) => item.get("rules").and_then(|rules| match rules {
            Value::String(content) => {
                Some((content.clone(), item.get("enable").and_then(Value::as_bool)))
            }
            Value::Array(lines) => Some((
                join_whistle_rule_lines(lines),
                item.get("enable").and_then(Value::as_bool),
            )),
            _ => None,
        }),
        _ => None,
    }
}

fn join_whistle_rule_lines(lines: &[Value]) -> String {
    lines
        .iter()
        .map(|line| match line {
            Value::Null => String::new(),
            Value::String(line) => line.clone(),
            value => value.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
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

    #[test]
    fn parses_official_whistle_export_order_and_folders() {
        let export = r#"{
          "Default": "example.com host://127.0.0.1",
          "\rLocal development": "",
          "API routes": "api.example.com host://127.0.0.1:3000",
          "Frontend": ["cdn.example.com file:///tmp/dist", "app.example.com debug://"],
          "": ["Default", "\rLocal development", "API routes", "Frontend"]
        }"#;

        let groups = parse_whistle_import_document(export, "fallback".into()).unwrap();

        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].name, "Default");
        assert_eq!(groups[0].folder, None);
        assert_eq!(groups[1].name, "API routes");
        assert_eq!(groups[1].folder.as_deref(), Some("Local development"));
        assert_eq!(groups[2].name, "Frontend");
        assert_eq!(
            groups[2].raw_content,
            "cdn.example.com file:///tmp/dist\napp.example.com debug://"
        );
    }

    #[test]
    fn parses_extended_whistle_items_and_order_object() {
        let export = r#"{
          "Disabled": {"rules": ["example.com statusCode://503"], "enable": false},
          "Enabled": {"rules": "example.com statusCode://200", "enable": true},
          "": {"list": ["Enabled", "Disabled"]}
        }"#;

        let groups = parse_whistle_import_document(export, "fallback".into()).unwrap();

        assert_eq!(
            groups
                .iter()
                .map(|group| group.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Enabled", "Disabled"]
        );
        assert!(groups[0].enabled);
        assert!(!groups[1].enabled);
        assert_eq!(groups[1].raw_content, "example.com statusCode://503");
    }

    #[test]
    fn keeps_plain_text_imports_byte_for_byte_after_bom_removal() {
        let content = "# local rules\r\nexample.com host://127.0.0.1\r\n";

        let groups = parse_whistle_import_document(content, "Whistle: local".into()).unwrap();

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "Whistle: local");
        assert_eq!(groups[0].raw_content, content);
    }
}
