use super::{
    CaptureBodyEncoding, CaptureBodyInput, CaptureBodySide, CaptureBodySource, PostGateApi,
};
use crate::error::{PostGateError, Result};
use crate::replay::{
    self, KeyValuePair, ReplayResponse, RequestBody, RequestHistory, SavedRequest,
};
use uuid::Uuid;

impl PostGateApi {
    pub async fn execute_replay(&self, request: SavedRequest) -> Result<ReplayResponse> {
        let response = replay::execute_request(&request).await?;
        let db = self.state.get_database().await?;
        let history = RequestHistory {
            id: Uuid::new_v4().to_string(),
            saved_request_id: Some(request.id.clone()),
            request,
            response: Some(response.clone()),
            error: None,
            executed_at: chrono::Utc::now().timestamp_millis(),
        };
        let _ = db.save_history(&history).await;
        Ok(response)
    }

    pub async fn import_capture_to_replay(
        &self,
        id: &str,
        collection_id: Option<String>,
    ) -> Result<SavedRequest> {
        let captured = self.get_capture(id, true).await?.ok_or_else(|| {
            PostGateError::NotFound(format!("Captured request '{}' not found", id))
        })?;
        let now = chrono::Utc::now().timestamp_millis();
        let headers = captured
            .request_headers
            .unwrap_or_default()
            .into_iter()
            .map(|(key, value)| KeyValuePair {
                key,
                value,
                enabled: true,
                description: None,
            })
            .collect();
        let url_obj = url::Url::parse(&captured.url).ok();
        let query_params = url_obj
            .as_ref()
            .map(|url| {
                url.query_pairs()
                    .map(|(key, value)| KeyValuePair {
                        key: key.to_string(),
                        value: value.to_string(),
                        enabled: true,
                        description: None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let base_url = url_obj
            .map(|mut url| {
                url.set_query(None);
                url.to_string()
            })
            .unwrap_or_else(|| captured.url.clone());

        let body = self
            .get_capture_body(CaptureBodyInput {
                id: id.to_string(),
                side: CaptureBodySide::Request,
                source: CaptureBodySource::Auto,
                encoding: CaptureBodyEncoding::Auto,
                max_bytes: Some(1024 * 1024),
                redact: true,
            })
            .await?
            .map(|body| match body.encoding.as_str() {
                "utf8" => RequestBody::Raw {
                    content: body.content,
                    content_type: body
                        .content_type
                        .unwrap_or_else(|| "text/plain".to_string()),
                },
                _ => RequestBody::Binary {
                    file_name: None,
                    data: Some(body.content),
                },
            })
            .unwrap_or(RequestBody::None);

        let request = SavedRequest {
            id: Uuid::new_v4().to_string(),
            name: format!("{} {}", captured.method, captured.path),
            collection_id,
            method: captured.method,
            url: base_url,
            headers,
            query_params,
            body,
            created_at: now,
            updated_at: now,
        };
        let db = self.state.get_database().await?;
        db.save_request(&request).await?;
        Ok(request)
    }
}
