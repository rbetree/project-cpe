//! 短信推送服务模块
//!
//! 为 PushPlus、Server酱 Turbo、PushDeer、Bark、ntfy 等轻量推送服务
//! 提供统一的短信转发入口。

use std::sync::Arc;

use chrono::Utc;
use reqwest::{Client, RequestBuilder, StatusCode};
use serde_json::{json, Value};

use crate::config::{ConfigManager, SmsPushConfig, SmsPushProvider};
use crate::db::SmsMessage;

pub struct SmsPushSender {
    client: Client,
    config_manager: Arc<ConfigManager>,
}

impl SmsPushSender {
    pub fn new(config_manager: Arc<ConfigManager>) -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("Failed to create HTTP client"),
            config_manager,
        }
    }

    fn get_config(&self) -> SmsPushConfig {
        self.config_manager.get_sms_push()
    }

    pub async fn forward_sms(&self, message: &SmsMessage) -> Result<(), String> {
        let config = self.get_config();

        if !config.enabled {
            return Ok(());
        }

        let title = render_sms_push_template(&config.title_template, message);
        let body = render_sms_push_template(&config.body_template, message);

        self.send_with_config(&config, &title, &body).await.map(|_| ())
    }

    pub async fn test_sms_push(&self) -> Result<String, String> {
        let config = self.get_config();

        if !config.enabled {
            return Err("短信推送服务未启用".to_string());
        }

        let test_message = SmsMessage {
            id: 0,
            direction: "incoming".to_string(),
            phone_number: "+8613800138000".to_string(),
            content: "这是一条测试短信 (SMS Push Test)".to_string(),
            timestamp: Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            status: "received".to_string(),
            pdu: None,
        };

        let title = render_sms_push_template(&config.title_template, &test_message);
        let body = render_sms_push_template(&config.body_template, &test_message);

        self.send_with_config(&config, &title, &body).await
    }

    async fn send_with_config(
        &self,
        config: &SmsPushConfig,
        title: &str,
        body: &str,
    ) -> Result<String, String> {
        validate_config(config)?;

        let request = build_request(&self.client, config, title, body)?;
        let response = request
            .send()
            .await
            .map_err(|e| format!("Failed to send SMS push: {}", e))?;

        let status = response.status();
        let response_body = response.text().await.unwrap_or_default();
        validate_provider_response(config.provider, status, &response_body)?;

        let response_preview = preview_response(&response_body);
        if response_preview.is_empty() {
            Ok(format!("短信推送测试成功 (status: {})", status))
        } else {
            Ok(format!("短信推送测试成功 (status: {}) {}", status, response_preview))
        }
    }
}

fn validate_config(config: &SmsPushConfig) -> Result<(), String> {
    let credential = config.credential.trim();
    let topic = config.topic.trim();

    match config.provider {
        SmsPushProvider::Pushplus
        | SmsPushProvider::Serverchan
        | SmsPushProvider::Pushdeer
        | SmsPushProvider::Bark => {
            if credential.is_empty() {
                return Err("当前推送服务缺少凭证".to_string());
            }
        }
        SmsPushProvider::Ntfy => {
            if topic.is_empty() {
                return Err("ntfy 主题不能为空".to_string());
            }
        }
    }

    Ok(())
}

fn build_request(
    client: &Client,
    config: &SmsPushConfig,
    title: &str,
    body: &str,
) -> Result<RequestBuilder, String> {
    let credential = config.credential.trim();
    let topic = config.topic.trim();

    match config.provider {
        SmsPushProvider::Pushplus => {
            let endpoint = resolve_endpoint(&config.server_url, "https://www.pushplus.plus/send");
            let mut payload = json!({
                "token": credential,
                "title": title,
                "content": body,
                "template": "markdown",
            });

            if !topic.is_empty() {
                payload["topic"] = json!(topic);
            }

            Ok(client.post(endpoint).json(&payload))
        }
        SmsPushProvider::Serverchan => {
            let base = resolve_base_url(&config.server_url, "https://sctapi.ftqq.com");
            let endpoint = format!("{}/{}.send", base, credential);

            Ok(client.post(endpoint).form(&[
                ("title", title),
                ("text", title),
                ("desp", body),
            ]))
        }
        SmsPushProvider::Pushdeer => {
            let endpoint = resolve_endpoint(&config.server_url, "https://api2.pushdeer.com/message/push");
            Ok(client.post(endpoint).form(&[
                ("pushkey", credential),
                ("text", title),
                ("desp", body),
                ("type", "markdown"),
            ]))
        }
        SmsPushProvider::Bark => {
            let endpoint = resolve_endpoint(&config.server_url, "https://api.day.app/push");
            let mut form_fields = vec![
                ("device_key", credential),
                ("title", title),
                ("body", body),
            ];

            if !topic.is_empty() {
                form_fields.push(("group", topic));
            }

            Ok(client.post(endpoint).form(&form_fields))
        }
        SmsPushProvider::Ntfy => {
            let endpoint = format!("{}/", resolve_base_url(&config.server_url, "https://ntfy.sh"));
            let mut request = client.post(endpoint).json(&json!({
                "topic": topic,
                "title": title,
                "message": body,
                "markdown": true,
            }));

            if !credential.is_empty() {
                request = request.bearer_auth(credential);
            }

            Ok(request)
        }
    }
}

fn validate_provider_response(
    provider: SmsPushProvider,
    status: StatusCode,
    body: &str,
) -> Result<(), String> {
    if !status.is_success() {
        return Err(format!(
            "推送服务返回错误状态 {}{}",
            status,
            format_body_suffix(body),
        ));
    }

    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    let value = match serde_json::from_str::<Value>(trimmed) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };

    match provider {
        SmsPushProvider::Pushplus | SmsPushProvider::Bark => {
            if let Some(code) = value.get("code").and_then(Value::as_i64) {
                if code != 200 {
                    return Err(extract_provider_error("推送服务返回失败", &value));
                }
            }
        }
        SmsPushProvider::Serverchan | SmsPushProvider::Pushdeer => {
            if let Some(code) = value.get("code").and_then(Value::as_i64) {
                if code != 0 && code != 200 {
                    return Err(extract_provider_error("推送服务返回失败", &value));
                }
            }
        }
        SmsPushProvider::Ntfy => {}
    }

    Ok(())
}

fn extract_provider_error(prefix: &str, value: &Value) -> String {
    let message = value
        .get("msg")
        .or_else(|| value.get("message"))
        .or_else(|| value.get("error"))
        .and_then(Value::as_str)
        .unwrap_or("未知错误");

    format!("{}: {}", prefix, message)
}

fn render_sms_push_template(template: &str, message: &SmsMessage) -> String {
    template
        .replace("{{id}}", &message.id.to_string())
        .replace("{{phone_number}}", &message.phone_number)
        .replace("{{content}}", &message.content)
        .replace("{{direction}}", &message.direction)
        .replace("{{timestamp}}", &message.timestamp)
        .replace("{{status}}", &message.status)
        .replace("{{sender}}", &message.phone_number)
        .replace("{{message}}", &message.content)
        .replace("{{time}}", &message.timestamp)
}

fn resolve_endpoint(input: &str, default: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.trim_end_matches('/').to_string()
    }
}

fn resolve_base_url(input: &str, default: &str) -> String {
    resolve_endpoint(input, default)
}

fn preview_response(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut preview = trimmed.chars().take(120).collect::<String>();
    if trimmed.chars().count() > 120 {
        preview.push_str("...");
    }

    format!("body: {}", preview)
}

fn format_body_suffix(body: &str) -> String {
    let preview = preview_response(body);
    if preview.is_empty() {
        String::new()
    } else {
        format!(" ({})", preview)
    }
}
