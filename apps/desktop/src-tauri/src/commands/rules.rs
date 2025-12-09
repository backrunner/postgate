use crate::error::Result;
use crate::rules::{parse_rules as parse_rules_internal, Rule, RuleGroup};
use crate::state::AppState;
use serde::Serialize;
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

/// Result of parsing rules
#[derive(Debug, Clone, Serialize)]
pub struct ParseResult {
    pub success: bool,
    pub rules: Vec<Rule>,
    pub errors: Vec<ParseError>,
}

/// A parse error with location info
#[derive(Debug, Clone, Serialize)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
    pub content: String,
}

/// Get all rule groups
#[tauri::command]
pub async fn get_rule_groups(state: State<'_, Arc<AppState>>) -> Result<Vec<RuleGroup>> {
    Ok(state.rule_engine.get_all_groups())
}

/// Save a rule group
#[tauri::command]
pub async fn save_rule_group(
    group: RuleGroup,
    state: State<'_, Arc<AppState>>,
) -> Result<RuleGroup> {
    let now = chrono::Utc::now().timestamp_millis();

    // Parse rules from raw content
    let rules = parse_rules_internal(&group.raw_content)?;

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
    };

    state.rule_engine.upsert_group(group.clone());

    Ok(group)
}

/// Delete a rule group
#[tauri::command]
pub async fn delete_rule_group(id: String, state: State<'_, Arc<AppState>>) -> Result<bool> {
    let removed = state.rule_engine.remove_group(&id);
    Ok(removed.is_some())
}

/// Toggle a rule group's enabled state
#[tauri::command]
pub async fn toggle_rule_group(
    id: String,
    enabled: bool,
    state: State<'_, Arc<AppState>>,
) -> Result<bool> {
    Ok(state.rule_engine.toggle_group(&id, enabled))
}

/// Parse rules from text (returns success/errors for validation)
#[tauri::command]
pub async fn parse_rules(content: String) -> Result<ParseResult> {
    match parse_rules_internal(&content) {
        Ok(rules) => Ok(ParseResult {
            success: true,
            rules,
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
            })
        }
    }
}
