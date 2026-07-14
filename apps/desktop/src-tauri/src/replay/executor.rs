//! Request executor for replay functionality

use crate::error::{PostGateError, Result};
use crate::replay::types::*;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE};
use reqwest::{Client, Method};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const REPLAY_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const REPLAY_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_REDIRECTS: usize = 10;

static REPLAY_CLIENT: OnceLock<std::result::Result<Client, String>> = OnceLock::new();

struct BuiltRequestBody {
    bytes: Vec<u8>,
    content_type: Option<String>,
}

/// Execute a saved request and return the response
pub async fn execute_request(request: &SavedRequest) -> Result<ReplayResponse> {
    let client = replay_client()?;
    execute_request_with_client(request, client).await
}

fn replay_client() -> Result<&'static Client> {
    REPLAY_CLIENT
        .get_or_init(|| {
            build_replay_client(REPLAY_REQUEST_TIMEOUT).map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|error| PostGateError::Proxy(format!("Failed to create HTTP client: {error}")))
}

fn build_replay_client(timeout: Duration) -> std::result::Result<Client, reqwest::Error> {
    Client::builder()
        .connect_timeout(REPLAY_CONNECT_TIMEOUT.min(timeout))
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS))
        .build()
}

async fn execute_request_with_client(
    request: &SavedRequest,
    client: &Client,
) -> Result<ReplayResponse> {
    let start = Instant::now();

    let url = build_request_url(request)?;

    // Build request body
    let body = build_request_body(&request.body)?;

    // Create the HTTP request and validate user-provided headers up front.
    let method: Method = request
        .method
        .parse()
        .map_err(|_| PostGateError::Proxy(format!("Invalid method: {}", request.method)))?;
    let mut headers = HeaderMap::new();

    for header in &request.headers {
        if header.enabled && !header.key.is_empty() {
            let lower = header.key.trim().to_ascii_lowercase();
            // Reqwest owns framing. Replaying captured Content-Length or
            // Transfer-Encoding alongside an edited body produces duplicate
            // or contradictory entity headers.
            if lower == "content-length" || lower == "transfer-encoding" {
                continue;
            }
            let name = HeaderName::from_bytes(header.key.trim().as_bytes()).map_err(|error| {
                PostGateError::Proxy(format!("Invalid header name '{}': {error}", header.key))
            })?;
            let value = HeaderValue::from_str(&header.value).map_err(|error| {
                PostGateError::Proxy(format!(
                    "Invalid value for header '{}': {error}",
                    header.key
                ))
            })?;
            headers.append(name, value);
        }
    }

    // Add Content-Type header for body if needed
    if let Some(content_type) = body.content_type.as_deref() {
        if !headers.contains_key(CONTENT_TYPE) {
            headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_str(content_type).map_err(|error| {
                    PostGateError::Proxy(format!("Invalid body content type: {error}"))
                })?,
            );
        }
    }

    let response = client
        .request(method, url)
        .headers(headers)
        .body(body.bytes)
        .send()
        .await
        .map_err(|error| PostGateError::Proxy(format!("Request failed: {error}")))?;

    // Extract response info
    let status = response.status().as_u16();
    let status_text = response
        .status()
        .canonical_reason()
        .unwrap_or("Unknown")
        .to_string();

    let mut response_headers = HashMap::new();
    for name in response.headers().keys() {
        let values = response
            .headers()
            .get_all(name)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .collect::<Vec<_>>()
            .join("\n");
        response_headers.insert(name.as_str().to_string(), values);
    }

    let content_type = response_headers.get("content-type").cloned();

    // Reqwest transparently decodes gzip, br, deflate and zstd when the
    // corresponding response Content-Encoding is present.
    let body_bytes = response
        .bytes()
        .await
        .map_err(|error| PostGateError::Proxy(format!("Failed to read response body: {error}")))?;

    let body_size = body_bytes.len() as u64;
    let duration_ms = start.elapsed().as_millis() as u64;

    // Try to decode text, but never turn invalid UTF-8 into an unexplained
    // empty response. Binary and invalid-text responses are explicit base64.
    let (body, body_is_base64) = if is_text_content(&content_type) || content_type.is_none() {
        match String::from_utf8(body_bytes.to_vec()) {
            Ok(text) => (Some(text), false),
            Err(error) => (
                Some(base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    error.into_bytes(),
                )),
                true,
            ),
        }
    } else {
        (
            Some(base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &body_bytes,
            )),
            true,
        )
    };

    Ok(ReplayResponse {
        status,
        status_text,
        headers: response_headers,
        body,
        body_is_base64,
        body_size,
        content_type,
        duration_ms,
    })
}

fn build_request_url(request: &SavedRequest) -> Result<url::Url> {
    let mut url = url::Url::parse(request.url.trim())
        .map_err(|error| PostGateError::Proxy(format!("Invalid URL: {error}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(PostGateError::Proxy(format!(
            "Unsupported URL scheme: {}",
            url.scheme()
        )));
    }
    url.set_fragment(None);
    {
        let mut query = url.query_pairs_mut();
        for parameter in request
            .query_params
            .iter()
            .filter(|parameter| parameter.enabled)
        {
            query.append_pair(&parameter.key, &parameter.value);
        }
    }
    Ok(url)
}

/// Build request body bytes and the Content-Type implied by the body editor.
fn build_request_body(body: &RequestBody) -> Result<BuiltRequestBody> {
    match body {
        RequestBody::None => Ok(BuiltRequestBody {
            bytes: Vec::new(),
            content_type: None,
        }),

        RequestBody::Raw {
            content,
            content_type,
        } => Ok(BuiltRequestBody {
            bytes: content.as_bytes().to_vec(),
            content_type: Some(content_type.clone()),
        }),

        RequestBody::Json { content } => Ok(BuiltRequestBody {
            bytes: content.as_bytes().to_vec(),
            content_type: Some("application/json".to_string()),
        }),

        RequestBody::UrlEncoded { fields } => {
            let encoded = fields
                .iter()
                .filter(|f| f.enabled)
                .map(|f| {
                    format!(
                        "{}={}",
                        urlencoding::encode(&f.key),
                        urlencoding::encode(&f.value)
                    )
                })
                .collect::<Vec<_>>()
                .join("&");
            Ok(BuiltRequestBody {
                bytes: encoded.into_bytes(),
                content_type: Some("application/x-www-form-urlencoded".to_string()),
            })
        }

        RequestBody::FormData { fields } => {
            // Simple multipart/form-data encoding
            let boundary = format!(
                "----PostGateBoundary{}",
                uuid::Uuid::new_v4().to_string().replace("-", "")
            );
            let mut body = Vec::new();

            for field in fields.iter().filter(|f| f.enabled) {
                body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());

                match field.field_type {
                    FormDataFieldType::Text => {
                        body.extend_from_slice(
                            format!(
                                "Content-Disposition: form-data; name=\"{}\"\r\n\r\n",
                                field.key
                            )
                            .as_bytes(),
                        );
                        body.extend_from_slice(field.value.as_bytes());
                    }
                    FormDataFieldType::File => {
                        let filename = field.file_name.as_deref().unwrap_or("file");
                        let content_type = field
                            .content_type
                            .as_deref()
                            .unwrap_or("application/octet-stream");
                        body.extend_from_slice(
                            format!(
                                "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
                                field.key, filename, content_type
                            ).as_bytes()
                        );
                        // Value should be base64 encoded file content
                        if let Ok(decoded) = base64::Engine::decode(
                            &base64::engine::general_purpose::STANDARD,
                            &field.value,
                        ) {
                            body.extend_from_slice(&decoded);
                        }
                    }
                }
                body.extend_from_slice(b"\r\n");
            }

            body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());
            Ok(BuiltRequestBody {
                bytes: body,
                content_type: Some(format!("multipart/form-data; boundary={}", boundary)),
            })
        }

        RequestBody::Binary { data, .. } => {
            let bytes = if let Some(data) = data {
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data)
                    .map_err(|e| PostGateError::Proxy(format!("Invalid base64 body: {}", e)))
            } else {
                Ok(Vec::new())
            }?;
            Ok(BuiltRequestBody {
                bytes,
                content_type: Some("application/octet-stream".to_string()),
            })
        }
    }
}

/// Check if content type indicates text content
fn is_text_content(content_type: &Option<String>) -> bool {
    content_type
        .as_ref()
        .map(|ct| {
            let ct = ct.to_ascii_lowercase();
            ct.contains("text/")
                || ct.contains("json")
                || ct.contains("xml")
                || ct.contains("javascript")
                || ct.contains("html")
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::sync::oneshot;

    fn saved_request(url: String, body: RequestBody) -> SavedRequest {
        SavedRequest {
            id: "request-1".into(),
            name: "Test".into(),
            collection_id: None,
            method: "POST".into(),
            url,
            headers: Vec::new(),
            query_params: Vec::new(),
            body,
            created_at: 0,
            updated_at: 0,
        }
    }

    async fn read_http_request(stream: &mut TcpStream) -> Vec<u8> {
        let mut request = Vec::new();
        let mut buffer = [0u8; 1024];

        loop {
            let read = stream.read(&mut buffer).await.unwrap();
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);

            let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
            else {
                continue;
            };
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            if request.len() >= header_end + 4 + content_length {
                break;
            }
        }

        request
    }

    async fn spawn_single_response(response: Vec<u8>) -> (String, oneshot::Receiver<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (request_tx, request_rx) = oneshot::channel();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let _ = request_tx.send(request);
            stream.write_all(&response).await.unwrap();
        });
        (format!("http://127.0.0.1:{port}/test"), request_rx)
    }

    #[test]
    fn raw_body_content_type_does_not_leak_static_string() {
        let built = build_request_body(&RequestBody::Raw {
            content: "hello".to_string(),
            content_type: "text/plain; charset=utf-8".to_string(),
        })
        .unwrap();

        assert_eq!(built.bytes, b"hello");
        assert_eq!(
            built.content_type.as_deref(),
            Some("text/plain; charset=utf-8")
        );
    }

    #[test]
    fn form_data_body_sets_boundary_content_type() {
        let built = build_request_body(&RequestBody::FormData {
            fields: vec![FormDataField {
                key: "name".to_string(),
                value: "codex".to_string(),
                field_type: FormDataFieldType::Text,
                enabled: true,
                file_name: None,
                content_type: None,
            }],
        })
        .unwrap();

        let content_type = built.content_type.expect("form-data content type");
        let boundary = content_type
            .strip_prefix("multipart/form-data; boundary=")
            .expect("multipart boundary");
        let body = String::from_utf8(built.bytes).unwrap();

        assert!(body.starts_with(&format!("--{}\r\n", boundary)));
        assert!(body.contains("Content-Disposition: form-data; name=\"name\""));
        assert!(body.contains("codex"));
        assert!(body.ends_with(&format!("--{}--\r\n", boundary)));
    }

    #[test]
    fn text_content_detection_is_case_insensitive() {
        assert!(is_text_content(&Some(
            "Application/JSON; Charset=UTF-8".to_string()
        )));
    }

    #[test]
    fn request_url_appends_encoded_query_and_drops_fragment() {
        let mut request = saved_request(
            "https://example.com/path?existing=1#section".into(),
            RequestBody::None,
        );
        request.query_params.push(KeyValuePair {
            key: "a b".into(),
            value: "x/y".into(),
            enabled: true,
            description: None,
        });

        let url = build_request_url(&request).unwrap();
        assert_eq!(url.fragment(), None);
        assert_eq!(
            url.query_pairs().collect::<Vec<_>>(),
            vec![
                ("existing".into(), "1".into()),
                ("a b".into(), "x/y".into())
            ]
        );
    }

    #[tokio::test]
    async fn execute_request_decodes_gzip_json() {
        let json = br#"{"ok":true,"message":"compressed"}"#;
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(json).unwrap();
        let compressed = encoder.finish().unwrap();
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: Application/JSON; Charset=UTF-8\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            compressed.len()
        )
        .into_bytes();
        response.extend_from_slice(&compressed);
        let (url, _request_rx) = spawn_single_response(response).await;
        let client = build_replay_client(Duration::from_secs(2)).unwrap();
        let request = saved_request(url, RequestBody::None);

        let result = execute_request_with_client(&request, &client)
            .await
            .unwrap();

        assert_eq!(
            result.body.as_deref(),
            Some(std::str::from_utf8(json).unwrap())
        );
        assert!(!result.body_is_base64);
        assert_eq!(result.body_size, json.len() as u64);
    }

    #[tokio::test]
    async fn execute_request_replaces_captured_content_length() {
        let response = b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".to_vec();
        let (url, request_rx) = spawn_single_response(response).await;
        let client = build_replay_client(Duration::from_secs(2)).unwrap();
        let mut request = saved_request(
            url,
            RequestBody::Raw {
                content: "hello".into(),
                content_type: "text/plain".into(),
            },
        );
        request.headers.push(KeyValuePair {
            key: "Content-Length".into(),
            value: "999".into(),
            enabled: true,
            description: None,
        });

        execute_request_with_client(&request, &client)
            .await
            .unwrap();
        let wire = String::from_utf8(request_rx.await.unwrap()).unwrap();
        let lower = wire.to_ascii_lowercase();

        assert_eq!(lower.matches("content-length:").count(), 1);
        assert!(lower.contains("content-length: 5"));
        assert!(wire.ends_with("hello"));
    }

    #[tokio::test]
    async fn execute_request_honors_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(Duration::from_secs(1)).await;
        });
        let client = build_replay_client(Duration::from_millis(50)).unwrap();
        let request = saved_request(format!("http://127.0.0.1:{port}/slow"), RequestBody::None);

        let started = Instant::now();
        let error = execute_request_with_client(&request, &client)
            .await
            .unwrap_err()
            .to_string();

        assert!(started.elapsed() < Duration::from_millis(500));
        assert!(error.contains("Request failed"));
    }
}
