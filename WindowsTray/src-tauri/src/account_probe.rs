use serde::{Deserialize, Serialize};
use url::Url;

use crate::account_models::normalize_api_base_url;
use crate::errors::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiProbeResult {
    pub normalized_base_url: String,
    pub model_ids: Vec<String>,
    pub suggested_display_name: String,
}

#[tauri::command]
pub async fn probe_api_account(
    api_key: String,
    base_url: String,
) -> Result<ApiProbeResult, String> {
    probe_api_account_inner(api_key, base_url)
        .await
        .map_err(|error| error.to_string())
}

async fn probe_api_account_inner(
    api_key: String,
    base_url: String,
) -> Result<ApiProbeResult, AppError> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Err(AppError::AccountValidationFailed("API key is required".into()));
    }

    let normalized_base_url = normalize_api_base_url(&base_url, true)
        .ok_or_else(|| AppError::AccountValidationFailed("Invalid base URL".into()))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|error| AppError::AccountVaultFailed(error.to_string()))?;

    let response = client
        .get(format!("{}/models", normalized_base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|error| AppError::AccountVaultFailed(error.to_string()))?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(AppError::AccountValidationFailed("Authentication failed (invalid API key)".into()));
    }

    if !response.status().is_success() {
        return Err(AppError::AccountVaultFailed(format!(
            "Server returned status {}",
            response.status()
        )));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|error| AppError::AccountVaultFailed(error.to_string()))?;

    let model_ids = parse_model_ids(&body);
    let suggested_display_name = suggest_display_name(&normalized_base_url);

    Ok(ApiProbeResult {
        normalized_base_url,
        model_ids,
        suggested_display_name,
    })
}

fn parse_model_ids(body: &serde_json::Value) -> Vec<String> {
    body.get("data")
        .and_then(|data| data.as_array())
        .map(|array| {
            array
                .iter()
                .filter_map(|item| item.get("id").and_then(|id| id.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn suggest_display_name(normalized_base_url: &str) -> String {
    let url = Url::parse(normalized_base_url).ok();
    let host = url.as_ref().and_then(|u| u.host_str()).unwrap_or("API");

    if host.contains("openai.com") {
        "OpenAI".to_string()
    } else if host.contains("anthropic.com") {
        "Anthropic".to_string()
    } else if host.contains("localhost") || host.contains("127.0.0.1") {
        "Local API".to_string()
    } else {
        format!("{} API", host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_base_url() {
        assert_eq!(
            normalize_api_base_url("https://api.openai.com", true),
            Some("https://api.openai.com/v1".to_string())
        );
        assert_eq!(
            normalize_api_base_url("https://api.openai.com/v1/", true),
            Some("https://api.openai.com/v1".to_string())
        );
        assert_eq!(
            normalize_api_base_url("http://localhost:11434/v1", true),
            Some("http://localhost:11434/v1".to_string())
        );
        assert_eq!(
            normalize_api_base_url("https://api.example.com/custom", true),
            Some("https://api.example.com/custom/v1".to_string())
        );
    }

    #[test]
    fn parses_model_ids() {
        let body = serde_json::json!({
            "data": [
                {"id": "gpt-4"},
                {"id": "gpt-3.5-turbo"}
            ]
        });
        assert_eq!(parse_model_ids(&body), vec!["gpt-4", "gpt-3.5-turbo"]);
    }

    #[test]
    fn suggests_display_name() {
        assert_eq!(suggest_display_name("https://api.openai.com/v1"), "OpenAI");
        assert_eq!(suggest_display_name("http://localhost:11434/v1"), "Local API");
        assert_eq!(suggest_display_name("https://api.deepseek.com/v1"), "api.deepseek.com API");
    }
}
