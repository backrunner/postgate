use rmcp::{model::Resource, ErrorData};
use serde::Serialize;

pub(super) fn resource(uri: &str, name: &str) -> Resource {
    Resource::new(uri, name).with_mime_type("application/json")
}

pub(super) fn to_json<T: Serialize>(value: T) -> Result<String, String> {
    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}

pub(super) fn to_value<T: Serialize>(
    result: crate::error::Result<T>,
) -> Result<serde_json::Value, ErrorData> {
    let value = result.map_err(to_error_data)?;
    serde_json::to_value(value).map_err(|e| ErrorData::internal_error(e.to_string(), None))
}

pub(super) fn to_error_data(error: crate::error::PostGateError) -> ErrorData {
    ErrorData::internal_error(error.to_string(), None)
}

impl From<crate::error::PostGateError> for String {
    fn from(error: crate::error::PostGateError) -> Self {
        error.to_string()
    }
}
