use super::types::{Rule, RuleAction, RuleGroup};
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
    group_enabled: bool,
    /// Shared reference to the owning group's inline value definitions.
    /// `Arc` so cloning into a `MatchedRule` is cheap.
    inline_values: Arc<HashMap<String, String>>,
}

/// Result of matching a rule, includes the rule and match details
#[derive(Debug, Clone)]
pub struct MatchedRule {
    pub rule: Rule,
    /// The remaining path after the matched prefix (for whistle-compatible path forwarding)
    pub remaining_path: String,
    /// Inline `{name}` definitions from the matched rule's group, used by the
    /// applicator to resolve value references. An empty map if the group has
    /// no inline definitions.
    pub inline_values: Arc<HashMap<String, String>>,
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

        let mut groups: Vec<_> = self.groups.iter().map(|r| r.value().clone()).collect();
        groups.sort_by_key(|group| std::cmp::Reverse(group.priority));

        for group in groups {
            let inline = Arc::new(group.inline_values.clone());
            for rule in &group.rules {
                compiled.push(CompiledRule {
                    rule: rule.clone(),
                    group_enabled: group.enabled,
                    inline_values: Arc::clone(&inline),
                });
            }
        }

        compiled.sort_by_key(|compiled| std::cmp::Reverse(compiled.rule.priority));

        *self.compiled_rules.write() = compiled;
    }

    /// Match a request against all rules
    ///
    /// # Arguments
    /// * `method` - HTTP method (GET, POST, etc.)
    /// * `host` - Request host (without port)
    /// * `path` - Request path (including leading /)
    /// * `protocol` - Protocol (http, https, ws, wss)
    /// * `port` - Request port
    /// * `headers` - Request headers
    pub fn match_request(
        &self,
        method: &str,
        host: &str,
        path: &str,
        protocol: &str,
        port: u16,
        headers: &HashMap<String, String>,
    ) -> Vec<MatchedRule> {
        self.match_request_with_client_ip(method, host, path, protocol, port, headers, None)
    }

    /// Match a request and include client IP context for whistle `i:` /
    /// `clientIp:` filters.
    #[allow(clippy::too_many_arguments)]
    pub fn match_request_with_client_ip(
        &self,
        method: &str,
        host: &str,
        path: &str,
        protocol: &str,
        port: u16,
        headers: &HashMap<String, String>,
        client_ip: Option<&str>,
    ) -> Vec<MatchedRule> {
        let compiled = self.compiled_rules.read();

        // Whistle URL normalization: ensure trailing / after bare hostname
        let normalized_path = if path.is_empty() { "/" } else { path };
        let url = format!("{}://{}{}", protocol, host, normalized_path);

        // Strip port from host for domain matching (whistle isDomain behavior)
        let host_no_port = if let Some(colon_idx) = host.rfind(':') {
            let maybe_port = &host[colon_idx + 1..];
            if maybe_port.chars().all(|c| c.is_ascii_digit()) {
                &host[..colon_idx]
            } else {
                host
            }
        } else {
            host
        };

        tracing::debug!(
            "RuleEngine::match_request - url: {}, total_rules: {}",
            url,
            compiled.len()
        );

        compiled
            .iter()
            .filter_map(|cr| {
                if !cr.group_enabled || !cr.rule.enabled {
                    return None;
                }

                // Check pattern match and get remaining path
                let match_result = cr.rule.pattern.match_with_remainder(&url, port);
                let host_match = if !match_result.matched {
                    // Try host-only match (also try without port)
                    cr.rule.pattern.matches_host(host_no_port)
                        || (host != host_no_port && cr.rule.pattern.matches_host(host))
                } else {
                    false
                };

                let mut matched = match_result.matched || host_match;

                // Apply negation (whistle ! prefix)
                if cr.rule.negated {
                    matched = !matched;
                }

                if !matched {
                    return None;
                }

                // Check filter conditions if present
                if let Some(filters) = &cr.rule.filters {
                    if !filters
                        .matches_request(method, protocol, port, headers, &url, client_ip, None)
                    {
                        return None;
                    }
                }

                tracing::debug!(
                    "Rule MATCHED - pattern: {:?}, remaining_path: {}",
                    cr.rule.pattern,
                    match_result.remaining_path
                );

                let remaining_path = if match_result.matched {
                    match_result.remaining_path
                } else {
                    normalized_path.to_string()
                };

                Some(MatchedRule {
                    rule: cr.rule.clone(),
                    remaining_path,
                    inline_values: Arc::clone(&cr.inline_values),
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
                && cr
                    .rule
                    .actions
                    .iter()
                    .any(|a| matches!(a, RuleAction::Debug { .. }))
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
            inline_values: HashMap::new(),
        }
    }

    fn make_rule(pattern: Pattern, actions: Vec<RuleAction>) -> Rule {
        Rule {
            id: "test".to_string(),
            pattern,
            filters: None,
            actions,
            enabled: true,
            priority: 0,
            raw_line: String::new(),
            negated: false,
        }
    }

    #[test]
    fn test_filter_method_matching() {
        let engine = RuleEngine::new();

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
            negated: false,
        };

        engine.upsert_group(create_test_group("test-group", vec![rule]));

        let matches =
            engine.match_request("POST", "example.com", "/", "https", 443, &HashMap::new());
        assert_eq!(matches.len(), 1);

        let matches =
            engine.match_request("GET", "example.com", "/", "https", 443, &HashMap::new());
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_filter_protocol_matching() {
        let engine = RuleEngine::new();

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
            negated: false,
        };

        engine.upsert_group(create_test_group("test-group", vec![rule]));

        let matches =
            engine.match_request("GET", "example.com", "/", "https", 443, &HashMap::new());
        assert_eq!(matches.len(), 1);

        let matches = engine.match_request("GET", "example.com", "/", "http", 80, &HashMap::new());
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_no_filter_matches_all() {
        let engine = RuleEngine::new();

        let rule = make_rule(
            Pattern::Domain("example.com".to_string()),
            vec![RuleAction::StatusCode { code: 200 }],
        );

        engine.upsert_group(create_test_group("test-group", vec![rule]));

        let matches =
            engine.match_request("GET", "example.com", "/", "https", 443, &HashMap::new());
        assert_eq!(matches.len(), 1);

        let matches = engine.match_request("POST", "example.com", "/", "http", 80, &HashMap::new());
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_url_pattern_matching() {
        use crate::rules::parser::parse_rules;

        let engine = RuleEngine::new();

        let rules =
            parse_rules("https://v.qq.com/biu/u/history/ http://127.0.0.1:3000/browser").unwrap();
        assert_eq!(rules.len(), 1);

        engine.upsert_group(create_test_group("test-group", rules));

        // Exact path match
        let matches = engine.match_request(
            "GET",
            "v.qq.com",
            "/biu/u/history/",
            "https",
            443,
            &HashMap::new(),
        );
        assert_eq!(matches.len(), 1, "Should match exact path");

        // Subpath
        let matches = engine.match_request(
            "GET",
            "v.qq.com",
            "/biu/u/history/page/1",
            "https",
            443,
            &HashMap::new(),
        );
        assert_eq!(matches.len(), 1, "Should match subpath");

        // Non-matching path
        let matches = engine.match_request(
            "GET",
            "v.qq.com",
            "/other/path",
            "https",
            443,
            &HashMap::new(),
        );
        assert_eq!(matches.len(), 0, "Should not match different path");
    }

    #[test]
    fn test_url_normalization_empty_path() {
        let engine = RuleEngine::new();

        let rule = make_rule(
            Pattern::Domain("example.com".to_string()),
            vec![RuleAction::StatusCode { code: 200 }],
        );

        engine.upsert_group(create_test_group("test-group", vec![rule]));

        // Empty path should be normalized to /
        let matches = engine.match_request("GET", "example.com", "", "https", 443, &HashMap::new());
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_negated_rule() {
        let engine = RuleEngine::new();

        let rule = Rule {
            negated: true,
            ..make_rule(
                Pattern::Domain("example.com".to_string()),
                vec![RuleAction::StatusCode { code: 200 }],
            )
        };

        engine.upsert_group(create_test_group("test-group", vec![rule]));

        // example.com should NOT match (negated)
        let matches =
            engine.match_request("GET", "example.com", "/", "https", 443, &HashMap::new());
        assert_eq!(matches.len(), 0);

        // other.com SHOULD match (negated domain doesn't match → inverted → matches)
        let matches = engine.match_request("GET", "other.com", "/", "https", 443, &HashMap::new());
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_port_pattern_matching() {
        let engine = RuleEngine::new();

        let rule = make_rule(
            Pattern::Port(8080),
            vec![RuleAction::StatusCode { code: 200 }],
        );

        engine.upsert_group(create_test_group("test-group", vec![rule]));

        let matches =
            engine.match_request("GET", "example.com", "/", "http", 8080, &HashMap::new());
        assert_eq!(matches.len(), 1);

        let matches =
            engine.match_request("GET", "example.com", "/", "https", 443, &HashMap::new());
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_client_ip_filter_matching() {
        let engine = RuleEngine::new();

        let rule = Rule {
            id: "test".to_string(),
            pattern: Pattern::Domain("example.com".to_string()),
            filters: Some(RuleFilters {
                client_ips: vec!["127.0.0.1".to_string()],
                ..Default::default()
            }),
            actions: vec![RuleAction::StatusCode { code: 200 }],
            enabled: true,
            priority: 0,
            raw_line: "example.com clientIp:127.0.0.1 statusCode://200".to_string(),
            negated: false,
        };

        engine.upsert_group(create_test_group("test-group", vec![rule]));

        let matches = engine.match_request_with_client_ip(
            "GET",
            "example.com",
            "/",
            "https",
            443,
            &HashMap::new(),
            Some("127.0.0.1"),
        );
        assert_eq!(matches.len(), 1);

        let matches = engine.match_request_with_client_ip(
            "GET",
            "example.com",
            "/",
            "https",
            443,
            &HashMap::new(),
            Some("10.0.0.1"),
        );
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_vqq_include_filter_full_engine() {
        use crate::rules::parser::parse_rules;

        let engine = RuleEngine::new();

        let raw = r#"v.qq.com localhost:8080 includeFilter:///https?:\/\/v.qq.com\/(@|packages|common|node_modules|src|x\/(cover|page|skeleton)|__vite_hmr)/i
https://v.qq.com/assets/ http://localhost:8080/assets/"#;
        let rules = parse_rules(raw).unwrap();

        engine.upsert_group(create_test_group("vqq", rules));

        let headers = HashMap::new();

        // Should match rule 1 (v.qq.com domain + includeFilter matches x/cover)
        let matches = engine.match_request(
            "GET",
            "v.qq.com",
            "/x/cover/kcaoffbyy2l0b45/p4102qbmz2h.html",
            "https",
            443,
            &headers,
        );
        assert!(
            matches.len() >= 1,
            "Should match the v.qq.com rule with includeFilter for /x/cover/ path"
        );

        // Should NOT match rule 1 (includeFilter rejects), should NOT match rule 2 (path /other/)
        let matches = engine.match_request(
            "GET",
            "v.qq.com",
            "/other/stuff.html",
            "https",
            443,
            &headers,
        );
        assert_eq!(
            matches.len(),
            0,
            "Should NOT match any rule for /other/ path"
        );

        // Should match rule 2 (https://v.qq.com/assets/ prefix)
        let matches = engine.match_request(
            "GET",
            "v.qq.com",
            "/assets/js/main.js",
            "https",
            443,
            &headers,
        );
        assert!(matches.len() >= 1, "Should match the assets rule");
    }
}
