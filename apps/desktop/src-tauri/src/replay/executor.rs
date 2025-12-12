//! Request executor for replay functionality

use crate::error::{PostGateError, Result};
use crate::replay::types::*;
use bytes::Bytes;
use hyper::body::Incoming;
use hyper::{Method, Request, Response};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use http_body_util::{BodyExt, Full};
use std::collections::HashMap;
use std::time::Instant;

/// Execute a saved request and return the response
pub async fn execute_request(request: &SavedRequest) -> Result<ReplayResponse> {
    let start = Instant::now();
    
    // Build the URL with query parameters
    let mut url = request.url.clone();
    let enabled_params: Vec<_> = request.query_params
        .iter()
        .filter(|p| p.enabled)
        .collect();
    
    if !enabled_params.is_empty() {
        let query_string = enabled_params
            .iter()
            .map(|p| format!("{}={}", urlencoding::encode(&p.key), urlencoding::encode(&p.value)))
            .collect::<Vec<_>>()
            .join("&");
        
        if url.contains('?') {
            url = format!("{}&{}", url, query_string);
        } else {
            url = format!("{}?{}", url, query_string);
        }
    }
    
    // Parse the URL
    let uri: hyper::Uri = url.parse()
        .map_err(|e| PostGateError::Proxy(format!("Invalid URL: {}", e)))?;
    
    // Determine the host and scheme
    let scheme = uri.scheme_str().unwrap_or("http");
    let host = uri.host()
        .ok_or_else(|| PostGateError::Proxy("URL missing host".into()))?;
    let port = uri.port_u16().unwrap_or(if scheme == "https" { 443 } else { 80 });
    
    // Build request body
    let body_bytes = build_request_body(&request.body)?;
    
    // Create the HTTP request
    let method: Method = request.method.parse()
        .map_err(|_| PostGateError::Proxy(format!("Invalid method: {}", request.method)))?;
    
    let mut req_builder = Request::builder()
        .method(method)
        .uri(&url);
    
    // Add headers
    for header in &request.headers {
        if header.enabled && !header.key.is_empty() {
            req_builder = req_builder.header(&header.key, &header.value);
        }
    }
    
    // Add Host header if not present
    let has_host = request.headers.iter().any(|h| h.enabled && h.key.to_lowercase() == "host");
    if !has_host {
        req_builder = req_builder.header("Host", format!("{}:{}", host, port));
    }
    
    // Add Content-Type header for body if needed
    if let Some(content_type) = get_body_content_type(&request.body) {
        let has_content_type = request.headers.iter()
            .any(|h| h.enabled && h.key.to_lowercase() == "content-type");
        if !has_content_type {
            req_builder = req_builder.header("Content-Type", content_type);
        }
    }
    
    // Add Content-Length
    if !body_bytes.is_empty() {
        req_builder = req_builder.header("Content-Length", body_bytes.len().to_string());
    }
    
    let req = req_builder
        .body(Full::new(Bytes::from(body_bytes)))
        .map_err(|e| PostGateError::Proxy(format!("Failed to build request: {}", e)))?;
    
    // Create HTTP client based on scheme
    let response = if scheme == "https" {
        execute_https_request(req, host, port).await?
    } else {
        execute_http_request(req).await?
    };
    
    let duration_ms = start.elapsed().as_millis() as u64;
    
    // Extract response info
    let status = response.status().as_u16();
    let status_text = response.status().canonical_reason().unwrap_or("Unknown").to_string();
    
    let mut headers = HashMap::new();
    for (name, value) in response.headers() {
        if let Ok(v) = value.to_str() {
            headers.insert(name.to_string(), v.to_string());
        }
    }
    
    let content_type = headers.get("content-type").cloned();
    
    // Collect body
    let body_bytes = response.into_body()
        .collect()
        .await
        .map_err(|e| PostGateError::Proxy(format!("Failed to read response body: {}", e)))?
        .to_bytes();
    
    let body_size = body_bytes.len() as u64;
    
    // Try to decode as text, fallback to base64
    let body = if is_text_content(&content_type) {
        String::from_utf8(body_bytes.to_vec()).ok()
    } else {
        Some(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &body_bytes))
    };
    
    Ok(ReplayResponse {
        status,
        status_text,
        headers,
        body,
        body_size,
        content_type,
        duration_ms,
    })
}

/// Execute an HTTP request
async fn execute_http_request(
    req: Request<Full<Bytes>>,
) -> Result<Response<Incoming>> {
    let client: Client<_, Full<Bytes>> = Client::builder(TokioExecutor::new())
        .build_http();
    
    client.request(req)
        .await
        .map_err(|e| PostGateError::Proxy(format!("HTTP request failed: {}", e)))
}

/// Execute an HTTPS request
async fn execute_https_request(
    req: Request<Full<Bytes>>,
    _host: &str,
    _port: u16,
) -> Result<Response<Incoming>> {
    use hyper_rustls::HttpsConnectorBuilder;
    
    let https = HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_or_http()
        .enable_http1()
        .build();
    
    let client: Client<_, Full<Bytes>> = Client::builder(TokioExecutor::new())
        .build(https);
    
    client.request(req)
        .await
        .map_err(|e| PostGateError::Proxy(format!("HTTPS request failed: {}", e)))
}

/// Build request body bytes from RequestBody
fn build_request_body(body: &RequestBody) -> Result<Vec<u8>> {
    match body {
        RequestBody::None => Ok(Vec::new()),
        
        RequestBody::Raw { content, .. } => Ok(content.as_bytes().to_vec()),
        
        RequestBody::Json { content } => Ok(content.as_bytes().to_vec()),
        
        RequestBody::UrlEncoded { fields } => {
            let encoded = fields
                .iter()
                .filter(|f| f.enabled)
                .map(|f| format!("{}={}", urlencoding::encode(&f.key), urlencoding::encode(&f.value)))
                .collect::<Vec<_>>()
                .join("&");
            Ok(encoded.into_bytes())
        }
        
        RequestBody::FormData { fields } => {
            // Simple multipart/form-data encoding
            let boundary = format!("----PostGateBoundary{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
            let mut body = Vec::new();
            
            for field in fields.iter().filter(|f| f.enabled) {
                body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
                
                match field.field_type {
                    FormDataFieldType::Text => {
                        body.extend_from_slice(
                            format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", field.key).as_bytes()
                        );
                        body.extend_from_slice(field.value.as_bytes());
                    }
                    FormDataFieldType::File => {
                        let filename = field.file_name.as_deref().unwrap_or("file");
                        let content_type = field.content_type.as_deref().unwrap_or("application/octet-stream");
                        body.extend_from_slice(
                            format!(
                                "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
                                field.key, filename, content_type
                            ).as_bytes()
                        );
                        // Value should be base64 encoded file content
                        if let Ok(decoded) = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &field.value) {
                            body.extend_from_slice(&decoded);
                        }
                    }
                }
                body.extend_from_slice(b"\r\n");
            }
            
            body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());
            Ok(body)
        }
        
        RequestBody::Binary { data, .. } => {
            if let Some(data) = data {
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data)
                    .map_err(|e| PostGateError::Proxy(format!("Invalid base64 body: {}", e)))
            } else {
                Ok(Vec::new())
            }
        }
    }
}

/// Get content type for request body
fn get_body_content_type(body: &RequestBody) -> Option<&'static str> {
    match body {
        RequestBody::None => None,
        RequestBody::Raw { content_type, .. } => Some(Box::leak(content_type.clone().into_boxed_str())),
        RequestBody::Json { .. } => Some("application/json"),
        RequestBody::UrlEncoded { .. } => Some("application/x-www-form-urlencoded"),
        RequestBody::FormData { .. } => None, // Will be set with boundary
        RequestBody::Binary { .. } => Some("application/octet-stream"),
    }
}

/// Check if content type indicates text content
fn is_text_content(content_type: &Option<String>) -> bool {
    content_type.as_ref().map(|ct| {
        ct.contains("text/") || 
        ct.contains("json") || 
        ct.contains("xml") ||
        ct.contains("javascript") ||
        ct.contains("html")
    }).unwrap_or(false)
}
