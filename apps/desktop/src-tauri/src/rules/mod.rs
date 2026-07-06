mod applicator;
mod engine;
mod external;
mod parser;
mod types;

#[allow(unused_imports)]
pub use applicator::{
    apply_request_rules, apply_request_rules_with_values, apply_response_rules,
    apply_response_rules_with_values, capture_enabled, feature, is_enabled, persist_request_writes,
    persist_response_writes, remote_resource_urls_for_request, remote_resource_urls_for_response,
    remote_resource_urls_for_response_context, rules_require_request_body,
    rules_require_response_body, should_abort, ProxyCreds, RequestWriteContext, ResolveCtx,
    ResolvedResource, ResolvedResources, ResponseModification, ResponseWriteContext, UpstreamProxy,
    UpstreamProxyKind,
};
#[allow(unused_imports)]
pub use engine::{MatchedRule, RuleEngine};
#[allow(unused_imports)]
pub use external::{
    collect_external_include_files, parse_rules_with_external_includes,
    parse_rules_with_external_includes_and_deps, ExpandedRuleSet,
};
pub use parser::{parse_rules, parse_rules_with_inline};
#[allow(unused_imports)]
pub use types::{Rule, RuleAction, RuleGroup};
