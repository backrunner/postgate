use super::{PostGateApi, RuleParseIssue, RuleParseResult};
use crate::error::{PostGateError, Result};
use crate::rules::{parse_rules_with_external_includes, RuleGroup};
use uuid::Uuid;

impl PostGateApi {
    pub async fn list_rule_groups(&self) -> Result<Vec<RuleGroup>> {
        let groups = self.state.rule_engine.get_all_groups();
        if !groups.is_empty() {
            return Ok(groups);
        }

        let db = self.state.get_database().await?;
        let db_groups = db.get_rule_groups().await?;
        for group in &db_groups {
            self.state.rule_engine.upsert_group(group.clone());
        }
        Ok(db_groups)
    }

    pub async fn get_rule_group(&self, id: &str) -> Result<Option<RuleGroup>> {
        if let Some(group) = self.state.rule_engine.get_group(id) {
            return Ok(Some(group));
        }
        Ok(self
            .list_rule_groups()
            .await?
            .into_iter()
            .find(|group| group.id == id))
    }

    pub fn validate_rules(&self, content: &str) -> RuleParseResult {
        match parse_rules_with_external_includes(content, None) {
            Ok((rules, _inline_values)) => RuleParseResult {
                success: true,
                rules,
                errors: vec![],
                warnings: vec![],
            },
            Err(e) => RuleParseResult {
                success: false,
                rules: vec![],
                errors: vec![RuleParseIssue {
                    line: 1,
                    message: e.to_string(),
                    content: String::new(),
                }],
                warnings: vec![],
            },
        }
    }

    pub async fn save_rule_group(&self, mut group: RuleGroup) -> Result<RuleGroup> {
        let now = chrono::Utc::now().timestamp_millis();
        let (rules, inline_values) = parse_rules_with_external_includes(&group.raw_content, None)?;
        if group.id.is_empty() {
            group.id = Uuid::new_v4().to_string();
        }
        if group.created_at == 0 {
            group.created_at = now;
        }
        group.updated_at = now;
        group.rules = rules;
        group.inline_values = inline_values;

        self.state.rule_engine.upsert_group(group.clone());
        let db = self.state.get_database().await?;
        db.save_rule_group(&group).await?;
        Ok(group)
    }

    pub async fn append_rule_lines(&self, id: &str, lines: &[String]) -> Result<RuleGroup> {
        let mut group = self
            .get_rule_group(id)
            .await?
            .ok_or_else(|| PostGateError::NotFound(format!("Rule group '{}' not found", id)))?;
        if !group.raw_content.ends_with('\n') {
            group.raw_content.push('\n');
        }
        group.raw_content.push_str(&lines.join("\n"));
        group.raw_content.push('\n');
        self.save_rule_group(group).await
    }

    pub async fn toggle_rule_group(&self, id: &str, enabled: bool) -> Result<bool> {
        let toggled = self.state.rule_engine.toggle_group(id, enabled);
        if toggled {
            if let Some(group) = self.state.rule_engine.get_group(id) {
                let db = self.state.get_database().await?;
                db.save_rule_group(&group).await?;
            }
        }
        Ok(toggled)
    }

    pub async fn delete_rule_group(&self, id: &str) -> Result<bool> {
        let removed = self.state.rule_engine.remove_group(id);
        let db = self.state.get_database().await?;
        db.delete_rule_group(id).await?;
        Ok(removed.is_some())
    }
}
