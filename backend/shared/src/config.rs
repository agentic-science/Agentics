use figment::{Figment, providers::Env};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_database_url")]
    pub database_url: String,
    #[serde(default = "default_api_host")]
    pub api_host: String,
    #[serde(default = "default_api_port")]
    pub api_port: u16,
    #[serde(default = "default_storage_root")]
    pub storage_root: String,
    #[serde(default = "default_problems_root")]
    pub problems_root: String,
    #[serde(default = "default_admin_username")]
    pub admin_username: String,
    #[serde(default = "default_admin_password")]
    pub admin_password: String,
    #[serde(default = "default_worker_poll_interval_ms")]
    pub worker_poll_interval_ms: u64,
    #[serde(default = "default_runner_timeout_sec")]
    pub runner_timeout_sec: u64,
    #[serde(default = "default_runner_python_image")]
    pub runner_python_image: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_database_url() -> String {
    "postgres://llm_oj:llm_oj@127.0.0.1:5432/llm_oj".to_string()
}

fn default_api_host() -> String {
    "0.0.0.0".to_string()
}

fn default_api_port() -> u16 {
    3000
}

fn default_storage_root() -> String {
    "storage".to_string()
}

fn default_problems_root() -> String {
    "examples/problems".to_string()
}

fn default_admin_username() -> String {
    "admin".to_string()
}

fn default_admin_password() -> String {
    "llm-oj-admin".to_string()
}

fn default_worker_poll_interval_ms() -> u64 {
    3000
}

fn default_runner_timeout_sec() -> u64 {
    30
}

fn default_runner_python_image() -> String {
    "python:3.12-slim".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let config: Config = Figment::new()
            .merge(Env::prefixed("LLM_OJ_"))
            .extract()?;
        Ok(config)
    }
}
