use super::types::{Rule, RuleGroup};
use dashmap::DashMap;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Rule engine for matching and applying rules
pub struct RuleEngine {
    /// All rule groups, indexed by ID
    groups: DashMap<String, RuleGroup>,
    /// Cached compiled rules for fast matching
    compiled_rules: RwLock<Vec<CompiledRule>>,
}

/// A compiled rule optimized for fast matching
struct CompiledRule {
    rule: Rule,
    group_id: String,
    group_enabled: bool,
}

impl RuleEngine {
    /// Create a new rule engine
    pub fn new() -> Self {
        Self {
            groups: DashMap::new(),
            compiled_rules: RwLock::new(Vec::new()),
        }
    }

    /// Add or update a rule group
    pub fn upsert_group(&self, group: RuleGroup) {
        self.groups.insert(group.id.clone(), group);
        self.rebuild_cache();
    }

    /// Remove a rule group
    pub fn remove_group(&self, group_id: &str) -> Option<RuleGroup> {
        let removed = self.groups.remove(group_id).map(|(_, g)| g);
        if removed.is_some() {
            self.rebuild_cache();
        }
        removed
    }

    /// Get a rule group by ID
    pub fn get_group(&self, group_id: &str) -> Option<RuleGroup> {
        self.groups.get(group_id).map(|g| g.clone())
    }

    /// Get all rule groups
    pub fn get_all_groups(&self) -> Vec<RuleGroup> {
        self.groups.iter().map(|r| r.value().clone()).collect()
    }

    /// Toggle a rule group's enabled state
    pub fn toggle_group(&self, group_id: &str, enabled: bool) -> bool {
        if let Some(mut group) = self.groups.get_mut(group_id) {
            group.enabled = enabled;
            drop(group);
            self.rebuild_cache();
            true
        } else {
            false
        }
    }

    /// Rebuild the compiled rules cache
    fn rebuild_cache(&self) {
        let mut compiled = Vec::new();

        // Collect all groups and sort by priority
        let mut groups: Vec<_> = self.groups.iter().map(|r| r.value().clone()).collect();
        groups.sort_by(|a, b| b.priority.cmp(&a.priority));

        for group in groups {
            for rule in &group.rules {
                compiled.push(CompiledRule {
                    rule: rule.clone(),
                    group_id: group.id.clone(),
                    group_enabled: group.enabled,
                });
            }
        }

        // Sort compiled rules by priority
        compiled.sort_by(|a, b| b.rule.priority.cmp(&a.rule.priority));

        *self.compiled_rules.write() = compiled;
    }

    /// Match a request against all rules
    pub fn match_request(
        &self,
        method: &str,
        host: &str,
        path: &str,
        _headers: &HashMap<String, String>,
    ) -> Vec<Rule> {
        let compiled = self.compiled_rules.read();
        let url = format!("{}{}", host, path);

        compiled
            .iter()
            .filter(|cr| {
                cr.group_enabled
                    && cr.rule.enabled
                    && (cr.rule.pattern.matches(&url) || cr.rule.pattern.matches_host(host))
            })
            .map(|cr| cr.rule.clone())
            .collect()
    }

    /// Get statistics about the rule engine
    pub fn stats(&self) -> RuleEngineStats {
        let groups = self.groups.len();
        let compiled = self.compiled_rules.read();
        let total_rules = compiled.len();
        let enabled_rules = compiled
            .iter()
            .filter(|r| r.group_enabled && r.rule.enabled)
            .count();

        RuleEngineStats {
            groups,
            total_rules,
            enabled_rules,
        }
    }
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the rule engine
#[derive(Debug, Clone, serde::Serialize)]
pub struct RuleEngineStats {
    pub groups: usize,
    pub total_rules: usize,
    pub enabled_rules: usize,
}
