/*
 * @Author: 1orz cloudorzi@gmail.com
 * @Date: 2025-12-09 17:34:01
 * @LastEditors: 1orz cloudorzi@gmail.com
 * @LastEditTime: 2025-12-13 12:45:58
 * @FilePath: /udx710-backend/backend/src/config.rs
 * @Description: 
 * 
 * Copyright (c) 2025 by 1orz, All Rights Reserved. 
 */
//! 配置管理模块
//!
//! 使用 JSON 文件存储用户配置，支持热更新

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tracing::{info, warn};

const DEFAULT_LOADER_SCRIPT: &str = r#"#!/bin/sh
/home/root/ttyd/start.sh &
/home/root/udx710 -p 80 &
"#;
const LOADER_SCRIPT_PATH: &str = "/home/root/loader.sh";
const INIT_SCRIPT_PATH: &str = "/home/root/init.sh";
const INIT_SCRIPT_LOADER_COMMAND: &str = "sh /home/root/init.sh &";

/// Webhook 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub enabled: bool,
    pub url: String,
    pub forward_sms: bool,
    pub forward_calls: bool,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub secret: String,  // 可选的签名密钥
    #[serde(default = "default_sms_template")]
    pub sms_template: String,  // 短信 payload 模板
    #[serde(default = "default_call_template")]
    pub call_template: String,  // 通话 payload 模板
}

/// 默认短信模板 (飞书机器人格式)
fn default_sms_template() -> String {
    r#"{
  "msg_type": "text",
  "content": {
    "text": "📱 短信通知\n发送方: {{phone_number}}\n内容: {{content}}\n时间: {{timestamp}}"
  }
}"#.to_string()
}

/// 默认通话模板 (飞书机器人格式)
fn default_call_template() -> String {
    r#"{
  "msg_type": "text",
  "content": {
    "text": "📞 来电通知\n号码: {{phone_number}}\n类型: {{direction}}\n时间: {{start_time}}\n时长: {{duration}}秒\n已接听: {{answered}}"
  }
}"#.to_string()
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: String::new(),
            forward_sms: true,
            forward_calls: true,
            headers: HashMap::new(),
            secret: String::new(),
            sms_template: default_sms_template(),
            call_template: default_call_template(),
        }
    }
}

/// 短信推送服务提供商
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SmsPushProvider {
    Pushplus,
    Serverchan,
    Pushdeer,
    Bark,
    Ntfy,
}

impl Default for SmsPushProvider {
    fn default() -> Self {
        Self::Pushplus
    }
}

/// 短信推送配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmsPushConfig {
    pub enabled: bool,
    #[serde(default)]
    pub provider: SmsPushProvider,
    #[serde(default)]
    pub credential: String,
    #[serde(default)]
    pub server_url: String,
    #[serde(default)]
    pub topic: String,
    #[serde(default = "default_sms_push_title_template")]
    pub title_template: String,
    #[serde(default = "default_sms_push_body_template")]
    pub body_template: String,
}

fn default_sms_push_title_template() -> String {
    "短信通知 · {{phone_number}}".to_string()
}

fn default_sms_push_body_template() -> String {
    "时间: {{timestamp}}\n号码: {{phone_number}}\n状态: {{status}}\n\n{{content}}".to_string()
}

impl Default for SmsPushConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: SmsPushProvider::Pushplus,
            credential: String::new(),
            server_url: String::new(),
            topic: String::new(),
            title_template: default_sms_push_title_template(),
            body_template: default_sms_push_body_template(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshConfig {
    #[serde(default = "default_refresh_interval_ms")]
    pub interval_ms: u64,
}

fn default_refresh_interval_ms() -> u64 {
    5_000
}

impl Default for RefreshConfig {
    fn default() -> Self {
        Self {
            interval_ms: default_refresh_interval_ms(),
        }
    }
}

impl RefreshConfig {
    pub fn sanitize(mut self) -> Self {
        self.interval_ms = sanitize_refresh_interval_ms(self.interval_ms);
        self
    }

    pub fn heartbeat_timeout_ms(&self) -> u64 {
        let base = self.interval_ms.max(1_000);
        if self.interval_ms == 0 {
            30_000
        } else {
            (base.saturating_mul(4)).clamp(15_000, 120_000)
        }
    }

    pub fn active_watchdog_interval_ms(&self) -> u64 {
        if self.interval_ms == 0 {
            15_000
        } else {
            self.interval_ms.max(5_000)
        }
    }

    pub fn idle_watchdog_interval_ms(&self) -> u64 {
        self.active_watchdog_interval_ms()
            .saturating_mul(6)
            .max(60_000)
    }
}

fn sanitize_refresh_interval_ms(interval_ms: u64) -> u64 {
    match interval_ms {
        0 => 0,
        1..=999 => 1_000,
        value => value.min(60_000),
    }
}

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub webhook: WebhookConfig,
    #[serde(default)]
    pub sms_push: SmsPushConfig,
    #[serde(default)]
    pub refresh: RefreshConfig,
}


/// 配置管理器
pub struct ConfigManager {
    config: Arc<RwLock<AppConfig>>,
    config_path: PathBuf,
}

impl ConfigManager {
    /// 创建新的配置管理器
    pub fn new(config_path: PathBuf) -> Self {
        let config = if config_path.exists() {
            match fs::read_to_string(&config_path) {
                Ok(content) => {
                    match serde_json::from_str::<AppConfig>(&content) {
                        Ok(cfg) => AppConfig {
                            refresh: cfg.refresh.sanitize(),
                            ..cfg
                        },
                        Err(e) => {
                            warn!(error = %e, "Failed to parse config file, using defaults");
                            AppConfig::default()
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Failed to read config file, using defaults");
                    AppConfig::default()
                }
            }
        } else {
            info!("No config file found, using defaults");
            AppConfig::default()
        };

        let manager = Self {
            config: Arc::new(RwLock::new(config)),
            config_path,
        };
        
        // 保存默认配置（如果文件不存在）
        if !manager.config_path.exists() {
            let _ = manager.save();
        }
        
        manager
    }
    
    /// 获取当前配置
    #[allow(dead_code)]
    pub fn get(&self) -> AppConfig {
        self.config.read().unwrap().clone()
    }
    
    /// 获取 Webhook 配置
    pub fn get_webhook(&self) -> WebhookConfig {
        self.config.read().unwrap().webhook.clone()
    }
    
    /// 更新 Webhook 配置
    pub fn set_webhook(&self, webhook: WebhookConfig) -> Result<(), String> {
        {
            let mut config = self.config.write().unwrap();
            config.webhook = webhook;
        }
        self.save()
    }

    pub fn get_sms_push(&self) -> SmsPushConfig {
        self.config.read().unwrap().sms_push.clone()
    }

    pub fn set_sms_push(&self, sms_push: SmsPushConfig) -> Result<(), String> {
        {
            let mut config = self.config.write().unwrap();
            config.sms_push = sms_push;
        }
        self.save()
    }

    pub fn get_refresh(&self) -> RefreshConfig {
        self.config.read().unwrap().refresh.clone().sanitize()
    }

    pub fn set_refresh(&self, refresh: RefreshConfig) -> Result<(), String> {
        {
            let mut config = self.config.write().unwrap();
            config.refresh = refresh.sanitize();
        }
        self.save()
    }

    #[allow(dead_code)]
    pub fn set(&self, config: AppConfig) -> Result<(), String> {
        {
            let mut current = self.config.write().unwrap();
            *current = AppConfig {
                refresh: config.refresh.sanitize(),
                ..config
            };
        }
        self.save()
    }
    
    /// 保存配置到文件
    pub fn save(&self) -> Result<(), String> {
        let config = self.config.read().unwrap();
        let content = serde_json::to_string_pretty(&*config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        
        // 确保目录存在
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }
        
        fs::write(&self.config_path, content)
            .map_err(|e| format!("Failed to write config file: {}", e))?;
        
        Ok(())
    }
    
    /// 重新加载配置
    #[allow(dead_code)]
    pub fn reload(&self) -> Result<(), String> {
        if !self.config_path.exists() {
            return Err("Config file does not exist".to_string());
        }
        
        let content = fs::read_to_string(&self.config_path)
            .map_err(|e| format!("Failed to read config file: {}", e))?;
        
        let new_config: AppConfig = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse config file: {}", e))?;
        
        {
            let mut config = self.config.write().unwrap();
            *config = AppConfig {
                refresh: new_config.refresh.sanitize(),
                ..new_config
            };
        }
        
        Ok(())
    }
}

/// 获取默认配置文件路径
pub fn get_persistent_root_dir() -> PathBuf {
    let device_root = PathBuf::from("/data");
    if device_root.exists() {
        return device_root;
    }

    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn get_default_config_path() -> PathBuf {
    get_persistent_root_dir().join("config.json")
}

fn normalize_newlines(content: &str) -> String {
    content.replace("\r\n", "\n")
}

fn is_ota_hook_line(line: &str) -> bool {
    let trimmed = line.trim();

    if trimmed.is_empty() || trimmed.starts_with('#') {
        return false;
    }

    trimmed == "sh /home/root/ota.sh &"
        || trimmed == "/home/root/ota.sh"
        || trimmed == "/home/root/ota.sh &"
        || trimmed.starts_with("sh /home/root/ota.sh")
}

fn is_init_hook_line(line: &str) -> bool {
    let trimmed = line.trim();

    if trimmed.is_empty() || trimmed.starts_with('#') {
        return false;
    }

    trimmed == INIT_SCRIPT_LOADER_COMMAND
        || trimmed == INIT_SCRIPT_PATH
        || trimmed == format!("{} &", INIT_SCRIPT_PATH)
        || trimmed.starts_with(&format!("sh {}", INIT_SCRIPT_PATH))
}

fn loader_contains_ota_command(content: &str) -> bool {
    content.lines().any(is_ota_hook_line)
}

fn loader_contains_init_command(content: &str) -> bool {
    content.lines().any(is_init_hook_line)
}

fn remove_ota_command_from_loader(content: &str) -> String {
    let normalized = normalize_newlines(content);
    let mut filtered_lines: Vec<&str> = normalized
        .lines()
        .filter(|line| !is_ota_hook_line(line))
        .collect();

    while filtered_lines.last().is_some_and(|line| line.trim().is_empty()) {
        filtered_lines.pop();
    }

    if filtered_lines.is_empty() {
        return String::new();
    }

    format!("{}\n", filtered_lines.join("\n"))
}

fn append_init_command_to_loader(content: &str) -> String {
    let normalized = normalize_newlines(content);

    if loader_contains_init_command(&normalized) {
        return format!("{}\n", normalized.trim_end_matches('\n'));
    }

    let base = if normalized.trim().is_empty() {
        DEFAULT_LOADER_SCRIPT.trim_end_matches('\n').to_string()
    } else {
        normalized.trim_end_matches('\n').to_string()
    };

    format!("{}\n{}\n", base, INIT_SCRIPT_LOADER_COMMAND)
}

fn loader_uses_ab_bootstrap(content: &str) -> bool {
    content.contains("UDX710 OTA bootstrap")
        || content.contains("OTA_STATE_FILE=\"/home/root/ota/state.env\"")
}

fn loader_is_plain_legacy_bootstrap(content: &str) -> bool {
    let script_lines: Vec<&str> = content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with('#') || *line == "#!/bin/sh")
        .collect();

    if script_lines.len() < 3 {
        return false;
    }

    if script_lines[0] != "#!/bin/sh" {
        return false;
    }

    if script_lines[1] != "/home/root/ttyd/start.sh &"
        || script_lines[2] != "/home/root/udx710 -p 80 &"
    {
        return false;
    }

    script_lines[3..]
        .iter()
        .all(|line| *line == INIT_SCRIPT_LOADER_COMMAND)
}

fn set_executable_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path)
            .map_err(|e| format!("Failed to read metadata for {}: {}", path.display(), e))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
            .map_err(|e| format!("Failed to set permissions for {}: {}", path.display(), e))?;
    }

    Ok(())
}

pub fn ensure_loader_hooks_init() -> Result<(), String> {
    let loader_path = PathBuf::from(LOADER_SCRIPT_PATH);
    let current_content = if loader_path.exists() {
        fs::read_to_string(&loader_path)
            .map_err(|e| format!("Failed to read loader.sh: {}", e))?
    } else {
        String::new()
    };

    let stripped_content = remove_ota_command_from_loader(&current_content);
    let missing_backend_command = !stripped_content
        .lines()
        .any(|line| line.trim() == "/home/root/udx710 -p 80 &");

    let base_content = if loader_uses_ab_bootstrap(&current_content)
        || loader_contains_ota_command(&current_content)
        || missing_backend_command
    {
        DEFAULT_LOADER_SCRIPT.to_string()
    } else if current_content.trim().is_empty()
        || loader_is_plain_legacy_bootstrap(&current_content)
    {
        DEFAULT_LOADER_SCRIPT.to_string()
    } else {
        stripped_content
    };

    let updated_content = append_init_command_to_loader(&base_content);

    if let Some(parent) = loader_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create loader.sh directory: {}", e))?;
    }

    fs::write(&loader_path, updated_content)
        .map_err(|e| format!("Failed to write loader.sh: {}", e))?;
    set_executable_permissions(&loader_path)?;

    let _ = fs::remove_file("/home/root/ota.sh");

    Ok(())
}

pub fn get_init_script() -> Result<crate::models::InitScriptResponse, String> {
    let loader_content = if Path::new(LOADER_SCRIPT_PATH).exists() {
        fs::read_to_string(LOADER_SCRIPT_PATH)
            .map_err(|e| format!("Failed to read loader.sh: {}", e))?
    } else {
        DEFAULT_LOADER_SCRIPT.to_string()
    };

    let script = match fs::read_to_string(INIT_SCRIPT_PATH) {
        Ok(content) => normalize_newlines(&content),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("Failed to read init.sh: {}", e)),
    };

    Ok(crate::models::InitScriptResponse {
        script,
        init_path: INIT_SCRIPT_PATH.to_string(),
        loader_path: LOADER_SCRIPT_PATH.to_string(),
        loader_hooked: loader_contains_init_command(&loader_content),
    })
}

pub fn set_init_script(script: String) -> Result<crate::models::InitScriptResponse, String> {
    let init_path = PathBuf::from(INIT_SCRIPT_PATH);
    if let Some(parent) = init_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create init.sh directory: {}", e))?;
    }

    fs::write(&init_path, normalize_newlines(&script))
        .map_err(|e| format!("Failed to write init.sh: {}", e))?;
    set_executable_permissions(&init_path)?;

    ensure_loader_hooks_init()?;

    get_init_script()
}

#[cfg(test)]
mod tests {
    use super::{
        append_init_command_to_loader,
        loader_contains_init_command,
        loader_contains_ota_command,
        remove_ota_command_from_loader,
        INIT_SCRIPT_LOADER_COMMAND,
    };

    #[test]
    fn append_init_command_once_for_new_loader() {
        let loader = "#!/bin/sh\n/home/root/ttyd/start.sh &\n/home/root/udx710 -p 80 &\n";
        let updated = append_init_command_to_loader(loader);

        assert!(updated.contains(INIT_SCRIPT_LOADER_COMMAND));
        assert_eq!(updated.matches(INIT_SCRIPT_LOADER_COMMAND).count(), 1);
    }

    #[test]
    fn append_init_command_is_idempotent() {
        let loader = format!(
            "#!/bin/sh\n/home/root/ttyd/start.sh &\n/home/root/udx710 -p 80 &\n{}\n",
            INIT_SCRIPT_LOADER_COMMAND
        );
        let updated = append_init_command_to_loader(&loader);

        assert_eq!(updated.matches(INIT_SCRIPT_LOADER_COMMAND).count(), 1);
    }

    #[test]
    fn loader_detects_init_command() {
        let loader = format!("#!/bin/sh\n{}\n", INIT_SCRIPT_LOADER_COMMAND);
        assert!(loader_contains_init_command(&loader));
    }

    #[test]
    fn loader_ignores_commented_init_command() {
        let loader = format!("#!/bin/sh\n# {}\n", INIT_SCRIPT_LOADER_COMMAND);
        assert!(!loader_contains_init_command(&loader));
    }

    #[test]
    fn remove_ota_command_from_loader_strips_old_hook() {
        let loader = "#!/bin/sh\n/home/root/ttyd/start.sh &\nsh /home/root/ota.sh &\n/home/root/udx710 -p 80 &\n";
        let updated = remove_ota_command_from_loader(loader);

        assert!(!loader_contains_ota_command(&updated));
        assert!(updated.contains("/home/root/udx710 -p 80 &"));
    }
}
