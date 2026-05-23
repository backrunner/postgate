use crate::debug::ScriptInjector;
use crate::proxy::headers::{build_forward_response_headers, flat_to_headermap, headermap_to_flat};
use crate::rules::{
    apply_response_rules_with_values, persist_response_writes, MatchedRule, ResolveCtx,
    ResponseModification, ResponseWriteContext,
};
use bytes::Bytes;
use hyper::header::{HeaderMap, HeaderName, HeaderValue};
use std::collections::HashMap;

pub struct DirectResponseContext<'a, 'r> {
    pub matched_rules: &'a [MatchedRule],
    pub url: &'a str,
    pub method: &'a str,
    pub request_headers: &'a HashMap<String, String>,
    pub status: u16,
    pub response_headers: HashMap<String, String>,
    pub body: Bytes,
    pub resolve_ctx: &'a ResolveCtx<'r>,
    pub force_res_write: bool,
    pub debug_port: u16,
}

pub struct FinalizedDirectResponse {
    pub status: u16,
    pub headers: HeaderMap,
    pub flat_headers: HashMap<String, String>,
    pub body: Bytes,
}

/// Run response-stage rules for responses authored inside the proxy itself
/// (status/file/redirect/plugin handleRequest). Whistle wraps `res.response`
/// in its response inspector, so these direct responses still observe
/// resHeaders/resBody/resWrite/debug/resDelay/resSpeed.
pub async fn finalize_direct_response(
    ctx: DirectResponseContext<'_, '_>,
) -> FinalizedDirectResponse {
    let content_type = ctx.response_headers.get("content-type").cloned();
    let response_modification = apply_response_rules_with_values(
        ctx.matched_rules,
        ctx.url,
        ctx.method,
        ctx.request_headers,
        &ctx.response_headers,
        ctx.status,
        Some(&ctx.body),
        content_type.as_deref(),
        ctx.resolve_ctx,
    );

    if let Some(delay_ms) = response_modification.delay_ms {
        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
    }

    let final_status = response_modification.status_code.unwrap_or(ctx.status);
    let mut final_body = response_modification.body.clone().unwrap_or(ctx.body);

    if response_modification.inject_debug {
        let injector = ScriptInjector::new(ctx.debug_port);
        if let Ok(html) = String::from_utf8(final_body.to_vec()) {
            if !ScriptInjector::is_already_injected(&html) {
                final_body = Bytes::from(injector.inject_into_html(&html));
            }
        }
    }

    if let Some(speed_kbps) = response_modification.speed_kbps {
        final_body = super::throttle::apply_throttle(final_body, Some(speed_kbps)).await;
    }

    let mut modified_for_finalize = ResponseModification {
        headers: response_modification.headers.clone(),
        headers_to_remove: response_modification.headers_to_remove.clone(),
        cookies: response_modification.cookies.clone(),
        ..Default::default()
    };
    modified_for_finalize.status_code = response_modification.status_code;

    let base_headers = flat_to_headermap(&ctx.response_headers);
    let mut final_header_map = build_forward_response_headers(
        &base_headers,
        &ctx.response_headers,
        &modified_for_finalize,
        Some(final_body.len()),
    );
    final_header_map.remove("content-encoding");
    final_header_map.remove("transfer-encoding");
    final_header_map.remove("content-length");
    if let Ok(value) = HeaderValue::from_str(&final_body.len().to_string()) {
        final_header_map.insert(HeaderName::from_static("content-length"), value);
    }
    let final_flat_headers = headermap_to_flat(&final_header_map);

    persist_response_writes(
        &response_modification.write_files,
        ResponseWriteContext {
            method: ctx.method,
            status: final_status,
            headers: &final_flat_headers,
            body: &final_body,
            force: ctx.force_res_write,
        },
    );

    FinalizedDirectResponse {
        status: final_status,
        headers: final_header_map,
        flat_headers: final_flat_headers,
        body: final_body,
    }
}
