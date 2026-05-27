use agentics_domain::models::request::RegisterAgentResponse;
use anyhow::Result;
use serde_json::{Map, Value, json};

use crate::cli::OutputFormat;
use crate::config::ResolvedSettings;

use super::format::pretty_json;

/// Renders register agent for user-facing output.
pub(crate) fn render_register_agent(
    response: &RegisterAgentResponse,
    saved_token: bool,
    settings: &ResolvedSettings,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => {
            let mut body = Map::new();
            body.insert("agent_id".to_string(), json!(response.agent_id));
            body.insert("display_name".to_string(), json!(response.display_name));
            if !saved_token {
                body.insert("token".to_string(), json!(response.token));
            }
            body.insert("created_at".to_string(), json!(response.created_at));
            body.insert("saved_token".to_string(), json!(saved_token));
            body.insert("config_path".to_string(), json!(settings.config_path));
            body.insert(
                "api_base_url".to_string(),
                json!(settings.api_base_url.to_string()),
            );
            pretty_json(&Value::Object(body))
        }
        OutputFormat::Table => {
            let mut lines = vec![
                format!("Registered agent {}", response.display_name),
                format!("agent_id: {}", response.agent_id),
            ];
            if !saved_token {
                lines.push(format!("token: {}", response.token));
            }
            lines.push(format!("saved_token: {saved_token}"));
            lines.push(format!("config: {}", settings.config_path.display()));
            Ok(lines.join("\n"))
        }
    }
}

/// Renders auth status for user-facing output.
pub(crate) fn render_auth_status(
    settings: &ResolvedSettings,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(&json!({
            "api_base_url": settings.api_base_url.to_string(),
            "api_base_url_source": settings.api_base_url_source.to_string(),
            "token_configured": settings.token_configured(),
            "token_source": settings.token_source.to_string(),
            "config_path": settings.config_path,
        })),
        OutputFormat::Table => Ok(format!(
            "api_base_url: {} ({})\ntoken: {}\ntoken_source: {}\nconfig: {}",
            settings.api_base_url,
            settings.api_base_url_source,
            if settings.token_configured() {
                "configured"
            } else {
                "missing"
            },
            settings.token_source,
            settings.config_path.display()
        )),
    }
}

/// Renders config set for user-facing output.
pub(crate) fn render_config_set(
    key: &str,
    settings: &ResolvedSettings,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(&json!({
            "updated": key,
            "config_path": settings.config_path,
        })),
        OutputFormat::Table => Ok(format!(
            "updated: {key}\nconfig: {}",
            settings.config_path.display()
        )),
    }
}
