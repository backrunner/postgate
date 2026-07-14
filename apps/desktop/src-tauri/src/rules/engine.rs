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
    compiled_rules: RwLock<CompiledRuleSet>,
}

#[derive(Default)]
struct CompiledRuleSet {
    rules: Vec<CompiledRule>,
    host_index: HashMap<String, Vec<usize>>,
    fallback: Vec<usize>,
}

/// A compiled rule optimized for fast matching
struct CompiledRule {
    rule: Arc<Rule>,
    group_enabled: bool,
    /// Shared reference to the owning group's inline value definitions.
    /// `Arc` so cloning into a `MatchedRule` is cheap.
    inline_values: Arc<HashMap<String, String>>,
}

/// Result of matching a rule, includes the rule and match details
#[derive(Debug, Clone)]
pub struct MatchedRule {
    pub rule: Arc<Rule>,
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
            compiled_rules: RwLock::new(CompiledRuleSet::default()),
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
        groups.sort_by(|left, right| {
            left.priority
                .cmp(&right.priority)
                .then_with(|| left.id.cmp(&right.id))
        });

        for group in groups {
            let inline = Arc::new(group.inline_values.clone());
            let mut rules: Vec<_> = group.rules.iter().collect();
            // Applicators execute in vector order and later actions override
            // earlier ones, matching Whistle's bottom-to-top precedence.
            // Stable sorting preserves source order for equal priorities.
            rules.sort_by_key(|rule| rule.priority);
            for rule in rules {
                compiled.push(CompiledRule {
                    rule: Arc::new((*rule).clone()),
                    group_enabled: group.enabled,
                    inline_values: Arc::clone(&inline),
                });
            }
        }

        let mut host_index: HashMap<String, Vec<usize>> = HashMap::new();
        let mut fallback = Vec::new();
        for (index, compiled_rule) in compiled.iter().enumerate() {
            let indexed_host = (!compiled_rule.rule.negated)
                .then(|| indexed_pattern_host(&compiled_rule.rule.pattern))
                .flatten();
            if let Some(host) = indexed_host {
                host_index.entry(host).or_default().push(index);
            } else {
                fallback.push(index);
            }
        }

        *self.compiled_rules.write() = CompiledRuleSet {
            rules: compiled,
            host_index,
            fallback,
        };
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
        let default_port = match protocol {
            "https" | "wss" => 443,
            _ => 80,
        };
        let authority_host = if host.contains(':') && host.parse::<std::net::IpAddr>().is_ok() {
            format!("[{host}]")
        } else {
            host.to_string()
        };
        let authority = if port == default_port {
            authority_host
        } else {
            format!("{authority_host}:{port}")
        };
        let url = format!("{}://{}{}", protocol, authority, normalized_path);

        // Strip port from host for domain matching (whistle isDomain behavior)
        let host_no_port = if host.parse::<std::net::IpAddr>().is_ok() {
            host
        } else if let Some(colon_idx) = host.rfind(':') {
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
            compiled.rules.len()
        );

        let mut candidate_indices = compiled.fallback.clone();
        for candidate_host in host_candidate_keys(host_no_port) {
            if let Some(indices) = compiled.host_index.get(&candidate_host) {
                candidate_indices.extend_from_slice(indices);
            }
        }
        candidate_indices.sort_unstable();
        candidate_indices.dedup();

        candidate_indices
            .into_iter()
            .filter_map(|cr| {
                let cr = &compiled.rules[cr];
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
                    rule: Arc::clone(&cr.rule),
                    remaining_path,
                    inline_values: Arc::clone(&cr.inline_values),
                })
            })
            .collect()
    }

    /// Check if any enabled rule has a debug action
    pub fn has_active_debug_rules(&self) -> bool {
        let compiled = self.compiled_rules.read();
        compiled.rules.iter().any(|cr| {
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

fn indexed_pattern_host(pattern: &super::types::Pattern) -> Option<String> {
    use super::types::Pattern;

    match pattern {
        Pattern::Domain(host) => Some(normalize_index_host(host)),
        Pattern::Url { host, .. } | Pattern::NoSchema { host, .. }
            if !host.contains(['*', '?']) =>
        {
            Some(normalize_index_host(host))
        }
        Pattern::Exact(url) => {
            let parsed = url::Url::parse(url).ok()?;
            Some(normalize_index_host(parsed.host_str()?))
        }
        _ => None,
    }
}

fn normalize_index_host(host: &str) -> String {
    if let Some(rest) = host.strip_prefix('[') {
        if let Some((host, _)) = rest.split_once(']') {
            return host.to_ascii_lowercase();
        }
    }
    if let Some((host, port)) = host.rsplit_once(':') {
        if port.chars().all(|character| character.is_ascii_digit()) {
            return host.to_ascii_lowercase();
        }
    }
    host.trim_end_matches('.').to_ascii_lowercase()
}

fn host_candidate_keys(host: &str) -> Vec<String> {
    let host = normalize_index_host(host);
    if host.parse::<std::net::IpAddr>().is_ok() {
        return vec![host];
    }

    let mut candidates = vec![host.clone()];
    for (index, character) in host.char_indices() {
        if character == '.' && index + 1 < host.len() {
            candidates.push(host[index + 1..].to_string());
        }
    }
    candidates
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
    fn test_url_pattern_preserves_non_default_port() {
        use crate::rules::parser::parse_rules;

        let engine = RuleEngine::new();
        let rules = parse_rules("https://example.com:8443/api statusCode://204").unwrap();
        engine.upsert_group(create_test_group("ports", rules));

        assert_eq!(
            engine
                .match_request(
                    "GET",
                    "example.com",
                    "/api/users",
                    "https",
                    8443,
                    &HashMap::new(),
                )
                .len(),
            1
        );
        assert!(engine
            .match_request(
                "GET",
                "example.com",
                "/api/users",
                "https",
                443,
                &HashMap::new(),
            )
            .is_empty());

        let bare_engine = RuleEngine::new();
        let bare_rules = parse_rules("example.com:8443 statusCode://204").unwrap();
        bare_engine.upsert_group(create_test_group("bare-ports", bare_rules));
        assert_eq!(
            bare_engine
                .match_request("GET", "example.com", "/", "https", 8443, &HashMap::new(),)
                .len(),
            1
        );
    }

    #[test]
    fn test_higher_priorities_are_applied_last() {
        let engine = RuleEngine::new();

        let mut low_rule = make_rule(
            Pattern::Domain("example.com".to_string()),
            vec![RuleAction::StatusCode { code: 201 }],
        );
        low_rule.priority = 1;
        low_rule.raw_line = "low-rule".into();
        let mut high_rule = make_rule(
            Pattern::Domain("example.com".to_string()),
            vec![RuleAction::StatusCode { code: 202 }],
        );
        high_rule.priority = 10;
        high_rule.raw_line = "high-rule".into();

        let mut low_group = create_test_group("a-low", vec![high_rule]);
        low_group.priority = 1;
        let mut high_group = create_test_group("z-high", vec![low_rule]);
        high_group.priority = 10;
        engine.upsert_group(low_group);
        engine.upsert_group(high_group);

        let matches =
            engine.match_request("GET", "example.com", "/", "https", 443, &HashMap::new());
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].rule.raw_line, "high-rule");
        assert_eq!(matches[1].rule.raw_line, "low-rule");
    }

    #[test]
    fn test_domain_rules_use_host_index() {
        let engine = RuleEngine::new();
        let rules = (0..1_000)
            .map(|index| {
                make_rule(
                    Pattern::Domain(format!("host-{index}.example")),
                    vec![RuleAction::StatusCode { code: 200 }],
                )
            })
            .collect();
        engine.upsert_group(create_test_group("indexed", rules));

        let compiled = engine.compiled_rules.read();
        assert_eq!(compiled.host_index.len(), 1_000);
        assert!(compiled.fallback.is_empty());
        drop(compiled);

        let matches = engine.match_request(
            "GET",
            "host-777.example",
            "/api",
            "https",
            443,
            &HashMap::new(),
        );
        assert_eq!(matches.len(), 1);
    }

    #[test]
    #[ignore = "manual throughput benchmark"]
    fn benchmark_indexed_rule_matching() {
        let engine = RuleEngine::new();
        let rules = (0..10_000)
            .map(|index| {
                make_rule(
                    Pattern::Domain(format!("host-{index}.example")),
                    vec![RuleAction::StatusCode { code: 200 }],
                )
            })
            .collect();
        engine.upsert_group(create_test_group("benchmark", rules));

        let iterations = 100_000;
        let started = std::time::Instant::now();
        for _ in 0..iterations {
            let matches = engine.match_request(
                "GET",
                "host-7777.example",
                "/assets/app.js",
                "https",
                443,
                &HashMap::new(),
            );
            assert_eq!(matches.len(), 1);
        }
        let elapsed = started.elapsed();
        println!(
            "indexed rule matching: {} requests in {:?} ({:.0} req/s)",
            iterations,
            elapsed,
            iterations as f64 / elapsed.as_secs_f64()
        );
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
            !matches.is_empty(),
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
        assert!(!matches.is_empty(), "Should match the assets rule");
    }
}
