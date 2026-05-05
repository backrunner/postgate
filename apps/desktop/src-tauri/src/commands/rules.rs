use crate::error::Result;
use crate::rules::{parse_rules as parse_rules_internal, parse_rules_with_inline, Rule, RuleGroup};
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
    let now = chrono::Utc::now().timestamp_millis();

    // Parse rules + inline values from raw content.
    let (rules, inline_values) = parse_rules_with_inline(&group.raw_content)?;

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

    Ok(group)
}

/// Delete a rule group
#[tauri::command]
pub async fn delete_rule_group(id: String, state: State<'_, Arc<AppState>>) -> Result<bool> {
    // Remove from in-memory engine
    let removed = state.rule_engine.remove_group(&id);

    // Delete from database
    let db = state.get_database().await?;
    db.delete_rule_group(&id).await?;

    Ok(removed.is_some())
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
    }

    Ok(toggled)
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

/// Check if any enabled rule group has a debug:// action
#[tauri::command]
pub async fn has_active_debug_rules(state: State<'_, Arc<AppState>>) -> Result<bool> {
    Ok(state.rule_engine.has_active_debug_rules())
}
