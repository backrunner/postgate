//! External rule-file includes.
//!
//! Whistle supports `@file-path-or-url` for importing additional rules. PostGate
//! implements the local-file form here and also accepts `includeFile://...` as a
//! readable alias for the same behaviour.

use super::parser::parse_rules_with_inline;
use super::types::Rule;
use crate::error::{PostGateError, Result};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_INCLUDE_DEPTH: usize = 16;

#[derive(Debug, Clone)]
pub struct ExpandedRuleSet {
    pub rules: Vec<Rule>,
    pub inline_values: HashMap<String, String>,
    pub dependencies: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
struct ExpandedContent {
    content: String,
    dependencies: Vec<PathBuf>,
}

/// Parse rules after expanding local external rule-file directives.
pub fn parse_rules_with_external_includes(
    content: &str,
    base_dir: Option<&Path>,
) -> Result<(Vec<Rule>, HashMap<String, String>)> {
    let parsed = parse_rules_with_external_includes_and_deps(content, base_dir)?;
    Ok((parsed.rules, parsed.inline_values))
}

/// Parse rules after expanding local external rule-file directives, returning
/// the files read during expansion so callers can watch them for changes.
pub fn parse_rules_with_external_includes_and_deps(
    content: &str,
    base_dir: Option<&Path>,
) -> Result<ExpandedRuleSet> {
    let expanded = expand_external_includes(content, base_dir)?;
    let (rules, inline_values) = parse_rules_with_inline(&expanded.content)?;
    Ok(ExpandedRuleSet {
        rules,
        inline_values,
        dependencies: expanded.dependencies,
    })
}

/// Collect local include dependencies without failing if one of the files is
/// missing. Used by the background watcher so deleted files are still observed.
pub fn collect_external_include_files(content: &str, base_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut dependencies = Vec::new();
    let mut stack = Vec::new();
    collect_include_files(content, base_dir, &mut stack, &mut dependencies, 0);
    dedupe_paths(dependencies)
}

fn expand_external_includes(content: &str, base_dir: Option<&Path>) -> Result<ExpandedContent> {
    let mut stack = Vec::new();
    let mut dependencies = Vec::new();
    let content = expand_content(content, base_dir, &mut stack, &mut dependencies, 0)?;
    Ok(ExpandedContent {
        content,
        dependencies: dedupe_paths(dependencies),
    })
}

fn expand_content(
    content: &str,
    base_dir: Option<&Path>,
    stack: &mut Vec<PathBuf>,
    dependencies: &mut Vec<PathBuf>,
    depth: usize,
) -> Result<String> {
    if depth > MAX_INCLUDE_DEPTH {
        return Err(PostGateError::RuleParse(format!(
            "External rule include depth exceeded {}",
            MAX_INCLUDE_DEPTH
        )));
    }

    let mut output = String::with_capacity(content.len());
    let mut in_inline_value = false;

    for line in strip_utf8_bom(content).lines() {
        if update_inline_value_state(line, &mut in_inline_value) {
            output.push_str(line);
            output.push('\n');
            continue;
        }

        if !in_inline_value {
            if let Some(spec) = external_include_spec(line) {
                let path = resolve_local_rule_path(&spec, base_dir)?;
                let identity = canonical_or_absolute(&path);

                if stack.contains(&identity) {
                    return Err(PostGateError::RuleParse(format!(
                        "External rule include cycle detected at {}",
                        path.display()
                    )));
                }

                let included = fs::read_to_string(&path).map_err(|e| {
                    PostGateError::RuleParse(format!(
                        "Failed to read external rule file {}: {}",
                        path.display(),
                        e
                    ))
                })?;

                dependencies.push(identity.clone());
                stack.push(identity);
                let included_base = path.parent();
                let expanded =
                    expand_content(&included, included_base, stack, dependencies, depth + 1)?;
                stack.pop();

                output.push_str(&expanded);
                if !output.ends_with('\n') {
                    output.push('\n');
                }
                continue;
            }
        }

        output.push_str(line);
        output.push('\n');
    }

    Ok(output)
}

fn collect_include_files(
    content: &str,
    base_dir: Option<&Path>,
    stack: &mut Vec<PathBuf>,
    dependencies: &mut Vec<PathBuf>,
    depth: usize,
) {
    if depth > MAX_INCLUDE_DEPTH {
        return;
    }

    let mut in_inline_value = false;

    for line in strip_utf8_bom(content).lines() {
        if update_inline_value_state(line, &mut in_inline_value) {
            continue;
        }

        if in_inline_value {
            continue;
        }

        let Some(spec) = external_include_spec(line) else {
            continue;
        };
        let Ok(path) = resolve_local_rule_path(&spec, base_dir) else {
            continue;
        };
        let identity = canonical_or_absolute(&path);
        dependencies.push(identity.clone());

        if stack.contains(&identity) {
            continue;
        }

        let Ok(included) = fs::read_to_string(&path) else {
            continue;
        };

        stack.push(identity);
        collect_include_files(&included, path.parent(), stack, dependencies, depth + 1);
        stack.pop();
    }
}

fn external_include_spec(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }

    if let Some(value) = trimmed.strip_prefix('@') {
        return non_empty_spec(value);
    }

    for prefix in [
        "includeFile://",
        "includefile://",
        "include-file://",
        "ruleFile://",
        "rulefile://",
        "rulesFile://",
        "rulesfile://",
    ] {
        if let Some(value) = trimmed.strip_prefix(prefix) {
            return non_empty_spec(value);
        }
    }

    None
}

fn update_inline_value_state(line: &str, in_inline_value: &mut bool) -> bool {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("```") {
        return false;
    }

    if *in_inline_value {
        *in_inline_value = false;
        return true;
    }

    let name = trimmed.trim_start_matches('`').trim();
    if name.is_empty() {
        return false;
    }

    *in_inline_value = true;
    true
}

fn non_empty_spec(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let spec = if is_wrapped_in_matching_quotes(trimmed) {
        strip_matching_quotes(trimmed)
    } else {
        strip_matching_quotes(strip_unquoted_inline_comment(trimmed).trim())
    };
    if spec.is_empty() {
        None
    } else {
        Some(spec.to_string())
    }
}

fn is_wrapped_in_matching_quotes(value: &str) -> bool {
    if value.len() < 2 {
        return false;
    }

    let bytes = value.as_bytes();
    let first = bytes[0];
    let last = bytes[value.len() - 1];
    (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'')
}

fn strip_matching_quotes(value: &str) -> &str {
    if is_wrapped_in_matching_quotes(value) {
        return &value[1..value.len() - 1];
    }
    value
}

fn strip_unquoted_inline_comment(value: &str) -> &str {
    for (idx, _) in value.match_indices('#') {
        if idx > 0 && value[..idx].ends_with(char::is_whitespace) {
            return &value[..idx];
        }
    }
    value
}

fn resolve_local_rule_path(spec: &str, base_dir: Option<&Path>) -> Result<PathBuf> {
    if spec.starts_with("http://") || spec.starts_with("https://") {
        return Err(PostGateError::RuleParse(format!(
            "Remote external rule includes are not supported yet: {}",
            spec
        )));
    }

    let raw_path = if spec.starts_with("file://") {
        let url = url::Url::parse(spec).map_err(|e| {
            PostGateError::RuleParse(format!("Invalid file URL in external rule include: {}", e))
        })?;
        url.to_file_path().map_err(|_| {
            PostGateError::RuleParse(format!(
                "Invalid file URL in external rule include: {}",
                spec
            ))
        })?
    } else {
        PathBuf::from(expand_home(spec))
    };

    if raw_path.is_absolute() {
        Ok(raw_path)
    } else if let Some(base_dir) = base_dir {
        Ok(base_dir.join(raw_path))
    } else {
        Ok(env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(raw_path))
    }
}

fn expand_home(value: &str) -> String {
    if value == "~" {
        return env::var("HOME").unwrap_or_else(|_| value.to_string());
    }

    if let Some(rest) = value.strip_prefix("~/") {
        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home)
                .join(rest)
                .to_string_lossy()
                .into_owned();
        }
    }

    value.to_string()
}

fn canonical_or_absolute(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path)
        }
    })
}

fn dedupe_paths(mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.sort();
    paths.dedup();
    paths
}

fn strip_utf8_bom(content: &str) -> &str {
    content.strip_prefix('\u{feff}').unwrap_or(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::RuleAction;
    use tempfile::tempdir;

    #[test]
    fn parses_whistle_at_file_include() {
        let dir = tempdir().unwrap();
        let included = dir.path().join("extra.rules");
        fs::write(&included, "api.example.com statusCode://204\n").unwrap();

        let content = format!(
            "example.com host://127.0.0.1:8080\n@{}\n",
            included.display()
        );
        let parsed = parse_rules_with_external_includes_and_deps(&content, None).unwrap();

        assert_eq!(parsed.rules.len(), 2);
        assert_eq!(
            parsed.dependencies,
            vec![fs::canonicalize(included).unwrap()]
        );
        assert!(matches!(
            parsed.rules[1].actions.first(),
            Some(RuleAction::StatusCode { code: 204 })
        ));
    }

    #[test]
    fn parses_include_file_alias() {
        let dir = tempdir().unwrap();
        let included = dir.path().join("extra.rules");
        fs::write(&included, "api.example.com host://localhost:3000\n").unwrap();

        let content = format!("includeFile://{}\n", included.display());
        let parsed = parse_rules_with_external_includes_and_deps(&content, None).unwrap();

        assert_eq!(parsed.rules.len(), 1);
        assert!(matches!(
            parsed.rules[0].actions.first(),
            Some(RuleAction::Host { target }) if target == "localhost:3000"
        ));
    }

    #[test]
    fn resolves_nested_relative_includes_from_included_file_dir() {
        let dir = tempdir().unwrap();
        let nested_dir = dir.path().join("nested");
        fs::create_dir(&nested_dir).unwrap();
        fs::write(
            nested_dir.join("inner.rules"),
            "inner.example.com statusCode://202\n",
        )
        .unwrap();
        let outer = dir.path().join("outer.rules");
        fs::write(&outer, "@nested/inner.rules\n").unwrap();

        let content = format!("@{}\n", outer.display());
        let parsed = parse_rules_with_external_includes_and_deps(&content, None).unwrap();

        assert_eq!(parsed.rules.len(), 1);
        assert!(matches!(
            parsed.rules[0].actions.first(),
            Some(RuleAction::StatusCode { code: 202 })
        ));
    }

    #[test]
    fn rejects_include_cycles() {
        let dir = tempdir().unwrap();
        let first = dir.path().join("first.rules");
        let second = dir.path().join("second.rules");
        fs::write(&first, format!("@{}\n", second.display())).unwrap();
        fs::write(&second, format!("@{}\n", first.display())).unwrap();

        let content = format!("@{}\n", first.display());
        let err = parse_rules_with_external_includes_and_deps(&content, None)
            .unwrap_err()
            .to_string();

        assert!(err.contains("cycle detected"));
    }

    #[test]
    fn ignores_include_directives_inside_inline_values() {
        let dir = tempdir().unwrap();
        let included = dir.path().join("extra.rules");
        fs::write(&included, "api.example.com statusCode://204\n").unwrap();
        let content = format!(
            "``` payload\n@{}\n```\nexample.com host://127.0.0.1\n",
            included.display()
        );

        let parsed = parse_rules_with_external_includes_and_deps(&content, None).unwrap();

        assert_eq!(parsed.rules.len(), 1);
        assert!(parsed.dependencies.is_empty());
        assert!(parsed.inline_values["payload"].contains('@'));
    }

    #[test]
    fn empty_fence_does_not_hide_later_includes() {
        let dir = tempdir().unwrap();
        let included = dir.path().join("extra.rules");
        fs::write(&included, "api.example.com statusCode://204\n").unwrap();
        let content = format!("```\n@{}\n", included.display());

        let parsed = parse_rules_with_external_includes_and_deps(&content, None).unwrap();

        assert_eq!(parsed.rules.len(), 1);
        assert_eq!(parsed.dependencies.len(), 1);
    }

    #[test]
    fn strips_unquoted_inline_comments_from_include_directives() {
        let dir = tempdir().unwrap();
        let included = dir.path().join("extra.rules");
        fs::write(&included, "api.example.com statusCode://204\n").unwrap();
        let content = format!("@{}  # shared local rules\n", included.display());

        let parsed = parse_rules_with_external_includes_and_deps(&content, None).unwrap();

        assert_eq!(parsed.rules.len(), 1);
        assert!(matches!(
            parsed.rules[0].actions.first(),
            Some(RuleAction::StatusCode { code: 204 })
        ));
    }

    #[test]
    fn keeps_hashes_inside_quoted_include_paths() {
        let dir = tempdir().unwrap();
        let included = dir.path().join("extra # dev.rules");
        fs::write(&included, "api.example.com statusCode://204\n").unwrap();
        let content = format!("@\"{}\"\n", included.display());

        let parsed = parse_rules_with_external_includes_and_deps(&content, None).unwrap();

        assert_eq!(parsed.rules.len(), 1);
        assert_eq!(
            parsed.dependencies,
            vec![fs::canonicalize(included).unwrap()]
        );
    }

    #[test]
    fn allows_quoted_include_paths_with_trailing_comments() {
        let dir = tempdir().unwrap();
        let included = dir.path().join("extra.rules");
        fs::write(&included, "api.example.com statusCode://204\n").unwrap();
        let content = format!("@\"{}\" # shared local rules\n", included.display());

        let parsed = parse_rules_with_external_includes_and_deps(&content, None).unwrap();

        assert_eq!(parsed.rules.len(), 1);
        assert_eq!(
            parsed.dependencies,
            vec![fs::canonicalize(included).unwrap()]
        );
    }
}
