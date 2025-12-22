use super::types::{Rule, RuleAction, RuleGroup};
use dashmap::DashMap;
use parking_lot::RwLock;
use std::collections::HashMap;

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
    group_enabled: bool,
}

/// Result of matching a rule, includes the rule and match details
#[derive(Debug, Clone)]
pub struct MatchedRule {
    pub rule: Rule,
    /// The remaining path after the matched prefix (for whistle-compatible path forwarding)
    pub remaining_path: String,
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
                    group_enabled: group.enabled,
                });
            }
        }

        // Sort compiled rules by priority
        compiled.sort_by(|a, b| b.rule.priority.cmp(&a.rule.priority));

        *self.compiled_rules.write() = compiled;
    }

    /// Match a request against all rules
    /// 
    /// # Arguments
    /// * `method` - HTTP method (GET, POST, etc.)
    /// * `host` - Request host
    /// * `path` - Request path
    /// * `protocol` - Protocol (http, https, ws, wss)
    /// * `port` - Request port
    /// * `headers` - Request headers
    /// 
    /// # Returns
    /// Vector of matched rules with their remaining paths for whistle-compatible forwarding
    pub fn match_request(
        &self,
        method: &str,
        host: &str,
        path: &str,
        protocol: &str,
        port: u16,
        headers: &HashMap<String, String>,
    ) -> Vec<MatchedRule> {
        let compiled = self.compiled_rules.read();
        let url = format!("{}://{}{}", protocol, host, path);
        
        tracing::debug!(
            "RuleEngine::match_request - url: {}, total_rules: {}",
            url,
            compiled.len()
        );

        compiled
            .iter()
            .filter_map(|cr| {
                // Check if group and rule are enabled
                if !cr.group_enabled || !cr.rule.enabled {
                    tracing::trace!(
                        "Rule skipped (disabled) - group_enabled: {}, rule_enabled: {}, pattern: {:?}",
                        cr.group_enabled,
                        cr.rule.enabled,
                        cr.rule.pattern
                    );
                    return None;
                }

                // Check pattern match and get remaining path
                let match_result = cr.rule.pattern.match_with_remainder(&url);
                let host_match = if !match_result.matched {
                    // Try host-only match
                    cr.rule.pattern.matches_host(host)
                } else {
                    false
                };
                
                tracing::trace!(
                    "Rule check - pattern: {:?}, url_match: {}, host_match: {}",
                    cr.rule.pattern,
                    match_result.matched,
                    host_match
                );
                
                if !match_result.matched && !host_match {
                    return None;
                }

                // Check filter conditions if present
                if let Some(filters) = &cr.rule.filters {
                    if !filters.matches(method, protocol, port, headers, &url) {
                        tracing::trace!("Rule skipped (filter mismatch) - pattern: {:?}", cr.rule.pattern);
                        return None;
                    }
                }

                tracing::debug!(
                    "Rule MATCHED - pattern: {:?}, remaining_path: {}",
                    cr.rule.pattern,
                    match_result.remaining_path
                );

                // Use the remaining path from pattern match, or full path if host-only match
                let remaining_path = if match_result.matched {
                    match_result.remaining_path
                } else {
                    // Host-only match: keep the full path
                    path.to_string()
                };

                Some(MatchedRule {
                    rule: cr.rule.clone(),
                    remaining_path,
                })
            })
            .collect()
    }

    /// Check if any enabled rule has a debug action
    pub fn has_active_debug_rules(&self) -> bool {
        let compiled = self.compiled_rules.read();
        compiled.iter().any(|cr| {
            cr.group_enabled
                && cr.rule.enabled
                && cr.rule.actions.iter().any(|a| matches!(a, RuleAction::Debug { .. }))
        })
    }
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::types::{Pattern, RuleFilters};

    fn create_test_group(id: &str, rules: Vec<Rule>) -> RuleGroup {
        RuleGroup {
            id: id.to_string(),
            name: "Test".to_string(),
            rules,
            enabled: true,
            priority: 0,
            raw_content: String::new(),
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn test_filter_method_matching() {
        let engine = RuleEngine::new();
        
        // Create a rule with method filter
        let rule = Rule {
            id: "test".to_string(),
            pattern: Pattern::Domain("example.com".to_string()),
            filters: Some(RuleFilters {
                methods: vec!["POST".to_string()],
                ..Default::default()
            }),
            actions: vec![RuleAction::StatusCode { code: 200 }],
            enabled: true,
            priority: 0,
            raw_line: "example.com m:POST statusCode://200".to_string(),
        };

        engine.upsert_group(create_test_group("test-group", vec![rule]));

        // POST should match
        let matches = engine.match_request(
            "POST", "example.com", "/", "https", 443, &HashMap::new()
        );
        assert_eq!(matches.len(), 1);

        // GET should NOT match
        let matches = engine.match_request(
            "GET", "example.com", "/", "https", 443, &HashMap::new()
        );
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_filter_protocol_matching() {
        let engine = RuleEngine::new();
        
        // Create a rule with protocol filter
        let rule = Rule {
            id: "test".to_string(),
            pattern: Pattern::Domain("example.com".to_string()),
            filters: Some(RuleFilters {
                protocols: vec!["https".to_string()],
                ..Default::default()
            }),
            actions: vec![RuleAction::StatusCode { code: 200 }],
            enabled: true,
            priority: 0,
            raw_line: "example.com p:https statusCode://200".to_string(),
        };

        engine.upsert_group(create_test_group("test-group", vec![rule]));

        // HTTPS should match
        let matches = engine.match_request(
            "GET", "example.com", "/", "https", 443, &HashMap::new()
        );
        assert_eq!(matches.len(), 1);

        // HTTP should NOT match
        let matches = engine.match_request(
            "GET", "example.com", "/", "http", 80, &HashMap::new()
        );
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_no_filter_matches_all() {
        let engine = RuleEngine::new();
        
        // Create a rule without filters
        let rule = Rule {
            id: "test".to_string(),
            pattern: Pattern::Domain("example.com".to_string()),
            filters: None,
            actions: vec![RuleAction::StatusCode { code: 200 }],
            enabled: true,
            priority: 0,
            raw_line: "example.com statusCode://200".to_string(),
        };

        engine.upsert_group(create_test_group("test-group", vec![rule]));

        // All methods/protocols should match
        let matches = engine.match_request(
            "GET", "example.com", "/", "https", 443, &HashMap::new()
        );
        assert_eq!(matches.len(), 1);

        let matches = engine.match_request(
            "POST", "example.com", "/", "http", 80, &HashMap::new()
        );
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_url_pattern_matching() {
        use crate::rules::parser::parse_rules;
        
        let engine = RuleEngine::new();
        
        // Parse the actual whistle rule format
        let rules = parse_rules("https://v.qq.com/biu/u/history/ http://127.0.0.1:3000/browser").unwrap();
        assert_eq!(rules.len(), 1, "Should parse 1 rule");
        
        // Debug: print pattern info
        eprintln!("Parsed pattern: {:?}", rules[0].pattern);
        eprintln!("Parsed actions: {:?}", rules[0].actions);
        
        engine.upsert_group(create_test_group("test-group", rules));
        
        // Test 1: Exact path match
        let matches = engine.match_request(
            "GET", "v.qq.com", "/biu/u/history/", "https", 443, &HashMap::new()
        );
        eprintln!("Test 1 - exact path: {} matches", matches.len());
        assert_eq!(matches.len(), 1, "Should match exact path");
        
        // Test 2: Path with query string (like the real case)
        let matches = engine.match_request(
            "GET", "v.qq.com", "/biu/u/history/?selectTab=history&subTabId=all", "https", 443, &HashMap::new()
        );
        eprintln!("Test 2 - path with query: {} matches", matches.len());
        assert_eq!(matches.len(), 1, "Should match path with query");
        
        // Test 3: Subpath
        let matches = engine.match_request(
            "GET", "v.qq.com", "/biu/u/history/page/1", "https", 443, &HashMap::new()
        );
        eprintln!("Test 3 - subpath: {} matches", matches.len());
        assert_eq!(matches.len(), 1, "Should match subpath");
        
        // Test 4: Non-matching path
        let matches = engine.match_request(
            "GET", "v.qq.com", "/other/path", "https", 443, &HashMap::new()
        );
        eprintln!("Test 4 - non-matching: {} matches", matches.len());
        assert_eq!(matches.len(), 0, "Should not match different path");
    }
}
