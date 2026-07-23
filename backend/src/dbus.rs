/*
 * @Author: 1orz cloudorzi@gmail.com
 * @Date: 2025-12-10 09:19:05
 * @LastEditors: 1orz cloudorzi@gmail.com
 * @LastEditTime: 2025-12-13 12:46:02
 * @FilePath: /udx710-backend/backend/src/dbus.rs
 * @Description: 
 * 
 * Copyright (c) 2025 by 1orz, All Rights Reserved. 
 */
//! D-Bus 通信模块
//! 
//! 处理与 ofono D-Bus 服务的通信

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};
use zbus::{proxy, zvariant::OwnedValue, Connection, Proxy};

use crate::config::ConfigManager;
use crate::models::{
    AirplaneModeResponse, ApnContext, DeviceInfoResponse, NetworkInfoResponse, QosInfoResponse, RadioMode,
    RadioModeResponse, ServingCell, SimInfoResponse,
};
use crate::serial::with_serial;
use crate::state::FrontendRuntime;

/// ofono NetworkMonitor 代理接口
#[proxy(
    interface = "org.ofono.NetworkMonitor",
    default_service = "org.ofono",
    default_path = "/ril_0",
    assume_defaults = true
)]
pub trait NetworkMonitor {
    /// 获取服务小区信息
    fn get_serving_cell_information(
        &self,
    ) -> zbus::Result<HashMap<String, zbus::zvariant::OwnedValue>>;
}

/// ofono ConnectionContext 代理接口
#[proxy(
    interface = "org.ofono.ConnectionContext",
    default_service = "org.ofono",
    default_path = "/ril_0/context2",
    assume_defaults = true
)]
pub trait ConnectionContext {
    /// 获取连接上下文的所有属性
    fn get_properties(&self) -> zbus::Result<HashMap<String, zbus::zvariant::OwnedValue>>;
    
    /// 设置连接上下文的属性
    fn set_property(&self, name: &str, value: zbus::zvariant::Value<'_>) -> zbus::Result<()>;
}

/// ofono SimManager 代理接口
#[proxy(
    interface = "org.ofono.SimManager",
    default_service = "org.ofono",
    default_path = "/ril_0",
    assume_defaults = true
)]
pub trait SimManager {
    /// 获取SIM卡所有属性
    fn get_properties(&self) -> zbus::Result<HashMap<String, zbus::zvariant::OwnedValue>>;
}

/// ofono MessageManager 代理接口
#[proxy(
    interface = "org.ofono.MessageManager",
    default_service = "org.ofono",
    default_path = "/ril_0",
    assume_defaults = true
)]
pub trait MessageManager {
    /// 获取消息管理器所有属性
    fn get_properties(&self) -> zbus::Result<HashMap<String, zbus::zvariant::OwnedValue>>;
}

/// ofono NetworkRegistration 代理接口
#[proxy(
    interface = "org.ofono.NetworkRegistration",
    default_service = "org.ofono",
    default_path = "/ril_0",
    assume_defaults = true
)]
pub trait NetworkRegistration {
    /// 获取网络注册所有属性
    fn get_properties(&self) -> zbus::Result<HashMap<String, zbus::zvariant::OwnedValue>>;
}

/// ofono RadioSettings 代理接口
#[proxy(
    interface = "org.ofono.RadioSettings",
    default_service = "org.ofono",
    default_path = "/ril_0",
    assume_defaults = true
)]
pub trait RadioSettings {
    /// 获取无线设置所有属性
    fn get_properties(&self) -> zbus::Result<HashMap<String, zbus::zvariant::OwnedValue>>;
    
    /// 设置无线设置属性
    fn set_property(&self, name: &str, value: zbus::zvariant::Value<'_>) -> zbus::Result<()>;
}

/// ofono Modem 代理接口
#[proxy(
    interface = "org.ofono.Modem",
    default_service = "org.ofono",
    default_path = "/ril_0",
    assume_defaults = true
)]
pub trait Modem {
    /// 获取调制解调器所有属性
    fn get_properties(&self) -> zbus::Result<HashMap<String, zbus::zvariant::OwnedValue>>;
    
    /// 设置调制解调器属性
    fn set_property(&self, name: &str, value: zbus::zvariant::Value<'_>) -> zbus::Result<()>;
}

/// 串行 D-Bus/AT 操作的错误类型。
///
/// 包装底层 `zbus::Error`，并额外区分“操作超时”（oFona 卡住、长时间持有全局锁）。
/// 实现 `Display`，handler 中统一以 `format!("{}", e)` 渲染，无需感知具体类型。
#[derive(Debug)]
pub enum SerialOpError {
    /// 底层 D-Bus/AT 操作返回的错误。
    Inner(zbus::Error),
    /// 操作在 `SERIAL_OP_TIMEOUT` 内未完成（通常意味着 oFona 卡住）。
    Timeout,
}

impl std::fmt::Display for SerialOpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SerialOpError::Inner(e) => write!(f, "{}", e),
            SerialOpError::Timeout => write!(
                f,
                "D-Bus/AT operation timed out ({}s)",
                SERIAL_OP_TIMEOUT.as_secs()
            ),
        }
    }
}

impl std::error::Error for SerialOpError {}

impl From<zbus::Error> for SerialOpError {
    fn from(e: zbus::Error) -> Self {
        SerialOpError::Inner(e)
    }
}

/// 单次串行 D-Bus/AT 操作的超时阈值。
///
/// 超过该阈值未完成的操作将被中止并释放全局锁，防止一次卡住的 oFona 调用
/// 无限期阻塞后续所有射频/数据命令（含看门狗恢复）。修复 H3。
const SERIAL_OP_TIMEOUT: Duration = Duration::from_secs(15);

/// 带超时的串行执行：在全局锁内执行 `f`，若超过 `SERIAL_OP_TIMEOUT` 则中止。
///
/// 超时时 `with_serial` 的 future 被丢弃，其持有的全局锁 guard 随之释放，
/// 从而避免一次卡顿永久阻塞其它命令。修复 H3。
async fn with_serial_timed<T, F>(f: F) -> Result<T, SerialOpError>
where
    F: std::future::Future<Output = Result<T, zbus::Error>>,
{
    match tokio::time::timeout(SERIAL_OP_TIMEOUT, with_serial(f)).await {
        Ok(inner) => Ok(inner?),
        Err(_) => {
            warn!(
                timeout_secs = SERIAL_OP_TIMEOUT.as_secs(),
                "with_serial_timed: operation timed out, releasing global lock"
            );
            Err(SerialOpError::Timeout)
        }
    }
}

/// 通过 D-Bus 发送 AT 指令
///
/// # Arguments
/// * `conn` - D-Bus 连接
/// * `cmd` - AT 指令字符串
///
/// # Returns
/// AT 指令的响应结果
///
/// 已接入 `with_serial_timed`（15s 超时）：modem/oFona 卡住时不会无限期持有全局
/// `DBUS_LOCK` 阻塞其它射频/数据命令。超时映射回 `zbus::Error::Failure` 以保持
/// `zbus::Result` 返回类型（被大量 `?` 调用）。
#[tracing::instrument(skip(conn))]
pub async fn send_at_command(conn: &Connection, cmd: &str) -> zbus::Result<String> {
    debug!(command = %cmd, "发送 AT 指令");
    with_serial_timed(async {
        let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.Modem").await?;
        let result: String = proxy.call("SendAtcmd", &(cmd)).await?;
        debug!(command = %cmd, response = %result, "AT 指令响应");
        Ok(result)
    })
    .await
    .map_err(|e| match e {
        SerialOpError::Inner(zbe) => {
            error!(%zbe, command = %cmd, "AT 命令发送失败");
            zbe
        }
        SerialOpError::Timeout => {
            error!(command = %cmd, timeout_secs = SERIAL_OP_TIMEOUT.as_secs(), "AT 命令发送超时");
            zbus::Error::Failure(format!(
                "AT command '{}' timed out after {}s",
                cmd,
                SERIAL_OP_TIMEOUT.as_secs()
            ))
        }
    })
}

/// 获取服务小区信息
///
/// # Arguments
/// * `conn` - D-Bus 连接
///
/// # Returns
/// 服务小区信息结构
#[tracing::instrument(skip(conn))]
pub async fn get_serving_cell_info(conn: &Connection) -> zbus::Result<ServingCell> {
    debug!("查询服务小区信息");
    let result = with_serial(async {
        let proxy = NetworkMonitorProxy::new(conn).await?;
        let cell_info: HashMap<String, OwnedValue> = proxy.get_serving_cell_information().await?;

        let tech = cell_info
            .get("Technology")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_else(|| "unknown".to_string());

        let cell_id = parse_u32_from_keys(&cell_info, &["NCellId", "CellId", "NRCellID"]);
        let tac = parse_u32_from_keys(&cell_info, &["TrackingAreaCode"]);

        Ok(ServingCell { tech, cell_id, tac })
    }).await;
    if let Err(ref e) = result {
        error!(%e, "查询服务小区信息失败");
    }
    result
}

/// 查找第一个有效的 internet 类型 context 路径
///
/// 遍历所有 context，返回第一个类型为 internet 且配置了 APN 的 context 路径。
/// 如果没有配置 APN 的 context，则返回第一个 internet 类型的 context。
///
/// # Arguments
/// * `conn` - D-Bus 连接
///
/// # Returns
/// context 路径字符串
#[tracing::instrument(skip(conn))]
pub async fn find_internet_context(conn: &Connection) -> zbus::Result<String> {
    let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.ConnectionManager").await
        .map_err(|e| { error!(%e, "创建 ConnectionManager 代理失败"); e })?;
        let contexts: Vec<(zbus::zvariant::OwnedObjectPath, HashMap<String, OwnedValue>)> = 
            proxy.call("GetContexts", &()).await
            .map_err(|e| { error!(%e, "获取 context 列表失败"); e })?;
    
    let mut first_internet_context: Option<String> = None;
    
    for (path, props) in contexts {
        let context_type = props
            .get("Type")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_default();
        
        if context_type == "internet" {
            let apn = props
                .get("AccessPointName")
                .and_then(|v| String::try_from(v.clone()).ok())
                .unwrap_or_default();
            
            // 如果配置了 APN，优先返回这个 context
            if !apn.is_empty() {
                return Ok(path.to_string());
            }
            
            // 记录第一个 internet 类型的 context
            if first_internet_context.is_none() {
                first_internet_context = Some(path.to_string());
            }
        }
    }
    
    // 返回第一个 internet context，如果没有则返回默认值
    Ok(first_internet_context.unwrap_or_else(|| "/ril_0/context2".to_string()))
}

/// 获取所有 APN Context 列表
///
/// # Arguments
/// * `conn` - D-Bus 连接
///
/// # Returns
/// APN Context 列表
#[tracing::instrument(skip(conn))]
pub async fn get_all_apn_contexts(conn: &Connection) -> zbus::Result<Vec<ApnContext>> {
    let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.ConnectionManager").await
        .map_err(|e| { error!(%e, "创建 ConnectionManager 代理失败"); e })?;
    let contexts: Vec<(zbus::zvariant::OwnedObjectPath, HashMap<String, OwnedValue>)> = 
        proxy.call("GetContexts", &()).await
        .map_err(|e| { error!(%e, "获取所有 APN context 失败"); e })?;
    
    let mut result = Vec::new();
    
    for (path, props) in contexts {
        let context_type = props
            .get("Type")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_default();
        
        // 只返回 internet 类型的 context
        if context_type == "internet" {
            let apn_context = ApnContext {
                path: path.to_string(),
                name: props
                    .get("Name")
                    .and_then(|v| String::try_from(v.clone()).ok())
                    .unwrap_or_else(|| "Internet".to_string()),
                active: props
                    .get("Active")
                    .and_then(|v| bool::try_from(v.clone()).ok())
                    .unwrap_or(false),
                apn: props
                    .get("AccessPointName")
                    .and_then(|v| String::try_from(v.clone()).ok())
                    .unwrap_or_default(),
                protocol: props
                    .get("Protocol")
                    .and_then(|v| String::try_from(v.clone()).ok())
                    .unwrap_or_else(|| "ip".to_string()),
                username: props
                    .get("Username")
                    .and_then(|v| String::try_from(v.clone()).ok())
                    .unwrap_or_default(),
                password: props
                    .get("Password")
                    .and_then(|v| String::try_from(v.clone()).ok())
                    .unwrap_or_default(),
                auth_method: props
                    .get("AuthenticationMethod")
                    .and_then(|v| String::try_from(v.clone()).ok())
                    .unwrap_or_else(|| "chap".to_string()),
                context_type,
            };
            result.push(apn_context);
        }
    }
    
    Ok(result)
}

/// 设置 APN 属性
///
/// # Arguments
/// * `conn` - D-Bus 连接
/// * `context_path` - context 的 D-Bus 路径
/// * `property` - 属性名
/// * `value` - 属性值
///
/// # Returns
/// 操作结果
#[tracing::instrument(skip_all)]
pub async fn set_apn_property(
    conn: &Connection, 
    context_path: &str, 
    property: &str, 
    value: &str
) -> zbus::Result<()> {
    let result = with_serial(async {
        let proxy = ConnectionContextProxy::builder(conn)
            .path(context_path)?
            .build()
            .await?;
        
        proxy.set_property(property, zbus::zvariant::Value::Str(value.into())).await?;
        Ok(())
    }).await;
    if let Err(ref e) = result {
        error!(%e, property = %property, "设置 APN 属性失败");
    }
    result
}

/// 批量设置 APN 属性
///
/// # Arguments
/// * `conn` - D-Bus 连接
/// * `context_path` - context 的 D-Bus 路径
/// * `apn` - APN 名称（可选）
/// * `protocol` - 协议（可选）
/// * `username` - 用户名（可选）
/// * `password` - 密码（可选）
/// * `auth_method` - 认证方式（可选）
///
/// # Returns
/// 操作结果
#[tracing::instrument(skip(conn))]
pub async fn set_apn_properties(
    conn: &Connection,
    context_path: &str,
    apn: Option<&str>,
    protocol: Option<&str>,
    username: Option<&str>,
    password: Option<&str>,
    auth_method: Option<&str>,
) -> zbus::Result<()> {
    // 先检查 context 是否激活，如果激活需要先关闭
    let proxy = ConnectionContextProxy::builder(conn)
        .path(context_path)?
        .build()
        .await?;
    
    let props = proxy.get_properties().await?;
    let was_active = props
        .get("Active")
        .and_then(|v| bool::try_from(v.clone()).ok())
        .unwrap_or(false);
    
    // 如果 context 是激活状态，先关闭它
    if was_active {
        with_serial(async {
            let proxy = ConnectionContextProxy::builder(conn)
                .path(context_path)?
                .build()
                .await?;
            proxy.set_property("Active", zbus::zvariant::Value::Bool(false)).await.map_err(|e| {
                error!(%e, context_path = %context_path, "停用 APN context 失败");
                e
            })?;
            Ok::<(), zbus::Error>(())
        }).await?;
        
        // 等待一下让状态稳定
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
    
    // 设置各个属性
    if let Some(apn_val) = apn {
        set_apn_property(conn, context_path, "AccessPointName", apn_val).await?;
    }
    
    if let Some(protocol_val) = protocol {
        set_apn_property(conn, context_path, "Protocol", protocol_val).await?;
    }
    
    if let Some(username_val) = username {
        set_apn_property(conn, context_path, "Username", username_val).await?;
    }
    
    if let Some(password_val) = password {
        set_apn_property(conn, context_path, "Password", password_val).await?;
    }
    
    if let Some(auth_method_val) = auth_method {
        set_apn_property(conn, context_path, "AuthenticationMethod", auth_method_val).await?;
    }
    
    // 如果之前是激活状态，重新激活
    if was_active {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        with_serial(async {
            let proxy = ConnectionContextProxy::builder(conn)
                .path(context_path)?
                .build()
                .await?;
            proxy.set_property("Active", zbus::zvariant::Value::Bool(true)).await.map_err(|e| {
                error!(%e, context_path = %context_path, "重新激活 APN context 失败");
                e
            })?;
            Ok::<(), zbus::Error>(())
        }).await?;
    }
    
    Ok(())
}

/// 设置数据连接状态
///
/// # Arguments
/// * `conn` - D-Bus 连接
/// * `active` - true 开启数据流量，false 关闭数据流量
///
/// # Returns
/// 操作结果
#[tracing::instrument(skip(conn))]
pub async fn set_data_connection(conn: &Connection, active: bool) -> Result<(), SerialOpError> {
    info!(active, "设置数据连接");
    let result = with_serial_timed(async {
        // 自动查找有效的 internet context
        let context_path = find_internet_context(conn).await?;

        let proxy = ConnectionContextProxy::builder(conn)
            .path(context_path)?
            .build()
            .await?;
        proxy.set_property("Active", zbus::zvariant::Value::Bool(active)).await?;
        Ok(())
    })
    .await;
    if let Err(ref e) = result {
        error!(%e, active, "设置数据连接失败");
    }
    result
}

/// 获取数据连接状态
///
/// # Arguments
/// * `conn` - D-Bus 连接
///
/// # Returns
/// 数据连接是否激活
#[tracing::instrument(skip(conn))]
pub async fn get_data_connection_status(conn: &Connection) -> zbus::Result<bool> {
    let result = (|| async {
        // 自动查找有效的 internet context
        let context_path = find_internet_context(conn).await?;
        
        let proxy = ConnectionContextProxy::builder(conn)
            .path(context_path)?
            .build()
            .await?;
        let properties = proxy.get_properties().await?;
        
        let active = properties
            .get("Active")
            .and_then(|v| bool::try_from(v.clone()).ok())
            .unwrap_or(false);
        
        Ok(active)
    })().await;
    if let Err(ref e) = result {
        error!(%e, "获取数据连接状态失败");
    }
    result
}

/// 获取漫游状态
///
/// # Arguments
/// * `conn` - D-Bus 连接
///
/// # Returns
/// (roaming_allowed, is_roaming) 元组
#[tracing::instrument(skip(conn))]
pub async fn get_roaming_status(conn: &Connection) -> zbus::Result<(bool, bool)> {
    // 获取 ConnectionManager 的 RoamingAllowed 属性
    let cm_proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.ConnectionManager").await?;
    let cm_props: std::collections::HashMap<String, OwnedValue> = cm_proxy.call("GetProperties", &()).await?;
    
    let roaming_allowed = cm_props
        .get("RoamingAllowed")
        .and_then(|v| bool::try_from(v.clone()).ok())
        .unwrap_or(false);
    
    // 获取 NetworkRegistration 的 Status 属性判断是否漫游
    let net_proxy = NetworkRegistrationProxy::new(conn).await?;
    let net_props = net_proxy.get_properties().await?;
    
    let status = net_props
        .get("Status")
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_else(|| "unknown".to_string());
    
    let is_roaming = status == "roaming";
    
    Ok((roaming_allowed, is_roaming))
}

/// 设置漫游开关
///
/// # Arguments
/// * `conn` - D-Bus 连接
/// * `allowed` - true 允许漫游数据，false 禁止漫游数据
///
/// # Returns
/// 操作结果
#[tracing::instrument(skip(conn))]
pub async fn set_roaming_allowed(conn: &Connection, allowed: bool) -> zbus::Result<()> {
    info!(allowed, "设置漫游开关");
    let result = with_serial(async {
        let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.ConnectionManager").await?;
        let value = zbus::zvariant::Value::Bool(allowed);
        proxy.call::<_, _, ()>("SetProperty", &("RoamingAllowed", value)).await?;
        Ok(())
    }).await;
    if let Err(ref e) = result {
        error!(%e, allowed, "设置漫游开关失败");
    }
    result
}

/// 初始化数据连接（程序启动时调用）
///
/// 读取网络注册状态（Status 字符串），失败返回 "unknown"。
/// 仅读，不持串行锁（与既有读路径一致）。
async fn read_registration_status(conn: &Connection) -> String {
    match NetworkRegistrationProxy::new(conn).await {
        Ok(net_proxy) => match net_proxy.get_properties().await {
            Ok(props) => props
                .get("Status")
                .and_then(|v| String::try_from(v.clone()).ok())
                .unwrap_or_else(|| "unknown".to_string()),
            Err(_) => "unknown".to_string(),
        },
        Err(_) => "unknown".to_string(),
    }
}

/// 确保调制解调器处于软件上电状态（Powered=true）。
///
/// 修复 H1：后端原本从不写 Powered，完全假设 oFona 维持上电。本函数幂等——
/// 仅在 Powered=false 时尝试上电，并在切换后让出全局锁、短暂等待 oFona 注册子接口。
///
/// 安全性：**只动 Powered，不碰 Online**。飞行模式下 Powered 也保持 true，
/// 故本函数与用户飞行模式意图不冲突；Online/射频的干预仅由 watchdog 的升级恢复路径
/// （`try_escalate`）在"已注网但数据起不来"时处理。
#[tracing::instrument(skip(conn))]
pub async fn ensure_modem_powered(conn: &Connection) -> Result<(), SerialOpError> {
    let was_off: bool = with_serial_timed::<bool, _>(async {
        let proxy = ModemProxy::new(conn).await?;
        let props = proxy.get_properties().await?;
        let powered = props
            .get("Powered")
            .and_then(|v| bool::try_from(v.clone()).ok())
            .unwrap_or(false);
        if powered {
            return Ok(false);
        }
        info!("Modem Powered=false, powering on (ensure oFona power precondition)");
        proxy
            .set_property("Powered", zbus::zvariant::Value::Bool(true))
            .await?;
        Ok(true)
    })
    .await
    .map_err(|e| {
        error!(%e, "Modem 上电（Powered）失败");
        e
    })?;

    if was_off {
        // 让出全局锁后等待 oFona 注册子接口，避免持锁 sleep。
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Ok(())
}

/// 检查当前数据连接状态，如果未激活则尝试自动激活。
/// 这个函数会在后台静默执行，不会阻塞服务启动。
///
/// 修复 H1/H2：
/// - 先 `ensure_modem_powered` 保证 oFona 上电前置条件（Powered=true）。
/// - 轮询等待网络注册（退避 2/3/5/8/12s，上限 ~30s），而非单次快照未注网即放弃。
///
/// # Arguments
/// * `conn` - D-Bus 连接
///
/// # Returns
/// 初始化结果消息
#[tracing::instrument(skip(conn))]
pub async fn init_data_connection(conn: &Connection) -> String {
    // 0. 确保调制解调器已上电（Powered=true）—— oFona 上电时序的前置条件。
    //    幂等，且不影响 Online/飞行模式。失败不阻断：仍尝试下游流程（Powered 可能由 oFona 维持）。
    if let Err(e) = ensure_modem_powered(conn).await {
        error!(%e, "init_data_connection: ensure_modem_powered 失败");
    }

    // 1. 轮询等待网络注册（修复 H2：原为单次快照，未注网即 bail 且不重试）
    const REG_BACKOFF_SECS: [u64; 5] = [2, 3, 5, 8, 12];
    let mut waited = 0u64;
    let mut status = read_registration_status(conn).await;
    for &step in &REG_BACKOFF_SECS {
        if status == "registered" || status == "roaming" {
            break;
        }
        info!(status = %status, waited_secs = waited, "init: waiting for network registration");
        tokio::time::sleep(Duration::from_secs(step)).await;
        waited += step;
        status = read_registration_status(conn).await;
    }

    if status != "registered" && status != "roaming" {
        return format!(
            "Network not registered after ~{}s (status: {}), delegating to watchdog",
            waited, status
        );
    }

    // 2. 自动查找有效的 internet context
    let context_path = match find_internet_context(conn).await {
        Ok(path) => path,
        Err(e) => {
            return format!("Failed to find internet context: {}", e);
        }
    };

    // 3. 获取 context 的属性
    let proxy = match ConnectionContextProxy::builder(conn)
        .path(context_path.as_str())
        .and_then(|b| Ok(b))
    {
        Ok(builder) => match builder.build().await {
            Ok(p) => p,
            Err(e) => return format!("Failed to create context proxy: {}", e),
        },
        Err(e) => return format!("Failed to build context path: {}", e),
    };

    let props = match proxy.get_properties().await {
        Ok(p) => p,
        Err(e) => return format!("Failed to get context properties: {}", e),
    };

    // 4. 检查是否已激活
    let active = props
        .get("Active")
        .and_then(|v| bool::try_from(v.clone()).ok())
        .unwrap_or(false);

    if active {
        return format!("Data connection already active ({})", context_path);
    }

    // 5. 检查 APN 是否配置
    let apn = props
        .get("AccessPointName")
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_default();

    if apn.is_empty() {
        return format!("APN not configured on {}, skipping auto-connect", context_path);
    }

    // 6. 尝试激活数据连接
    match set_data_connection(conn, true).await {
        Ok(_) => format!("Data connection activated on {} (APN: {})", context_path, apn),
        Err(e) => format!("Failed to activate data connection: {}", e),
    }
}

/// 根据 MCC/MNC 获取推荐的 APN 配置
///
/// # Arguments
/// * `mcc` - 移动国家代码
/// * `mnc` - 移动网络代码
///
/// # Returns
/// (apn, protocol) 元组，如果未找到则返回 None
fn get_recommended_apn(mcc: &str, mnc: &str) -> Option<(&'static str, &'static str)> {
    match (mcc, mnc) {
        // 中国移动 (46000, 46002, 46007, 46008)
        ("460", "00") | ("460", "02") | ("460", "07") | ("460", "08") => Some(("cmnet", "dual")),
        // 中国联通 (46001, 46006, 46009)
        ("460", "01") | ("460", "06") | ("460", "09") => Some(("3gnet", "dual")),
        // 中国电信 (46003, 46005, 46011)
        ("460", "03") | ("460", "05") | ("460", "11") => Some(("ctnet", "dual")),
        // 中国广电 (46015)
        ("460", "15") => Some(("cbnet", "dual")),
        _ => None,
    }
}

/// 自动配置 APN（根据 SIM 卡运营商）
///
/// 根据 SIM 卡的 MCC/MNC 自动查找并设置推荐的 APN 配置
///
/// # Arguments
/// * `conn` - D-Bus 连接
/// * `context_path` - 要配置的 context 路径
///
/// # Returns
/// 配置结果消息
async fn auto_configure_apn(conn: &Connection, context_path: &str) -> Result<String, String> {
    // 1. 获取网络注册信息中的 MCC/MNC
    let net_proxy = NetworkRegistrationProxy::new(conn)
        .await
        .map_err(|e| format!("Failed to create network proxy: {}", e))?;
    
    let props = net_proxy
        .get_properties()
        .await
        .map_err(|e| format!("Failed to get network properties: {}", e))?;
    
    let mcc = props
        .get("MobileCountryCode")
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_default();
    
    let mnc = props
        .get("MobileNetworkCode")
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_default();
    
    if mcc.is_empty() || mnc.is_empty() {
        return Err("MCC/MNC not available".to_string());
    }
    
    // 2. 查找推荐 APN
    let (apn, protocol) = get_recommended_apn(&mcc, &mnc)
        .ok_or_else(|| format!("No recommended APN for MCC={} MNC={}", mcc, mnc))?;
    
    // 3. 设置 APN 和协议
    set_apn_property(conn, context_path, "AccessPointName", apn)
        .await
        .map_err(|e| format!("Failed to set APN: {}", e))?;
    
    set_apn_property(conn, context_path, "Protocol", protocol)
        .await
        .map_err(|e| format!("Failed to set protocol: {}", e))?;
    
    Ok(format!("Auto-configured APN: {} ({})", apn, protocol))
}

/// 升级恢复的连续失败阈值（已注网但 context 连续未激活的 tick 数）。
const RECOVERY_L1_THRESHOLD: u32 = 4;
/// L1 升级（cycle Online）的最短冷却，避免频繁重置射频导致 SIM 锁/耗电。
const RECOVERY_L1_COOLDOWN: Duration = Duration::from_secs(90);

/// 看门狗的恢复状态机。修复 H4：原 watchdog 只会重置 ConnectionContext.Active，
/// 无任何升级恢复。这里以“连续失败计数 + 冷却”驱动 L0 重激活 → L1 cycle Online。
#[derive(Default)]
struct RecoveryState {
    /// 连续“已注网但 context 未激活”的次数。
    consecutive_failures: u32,
    /// 累计执行过的升级次数。
    escalation_count: u32,
    /// 上次执行升级恢复的时刻。
    last_escalation: Option<std::time::Instant>,
}

impl RecoveryState {
    fn reset(&mut self) {
        self.consecutive_failures = 0;
    }

    fn record_failure(&mut self) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
    }

    fn can_escalate(&self, now: std::time::Instant, cooldown: Duration) -> bool {
        match self.last_escalation {
            None => true,
            Some(t) => now.duration_since(t) >= cooldown,
        }
    }

    fn next_escalation_in(&self, now: std::time::Instant, cooldown: Duration) -> Duration {
        match self.last_escalation {
            None => Duration::ZERO,
            Some(t) => {
                let elapsed = now.duration_since(t);
                if elapsed >= cooldown {
                    Duration::ZERO
                } else {
                    cooldown - elapsed
                }
            }
        }
    }
}

/// 设置 Modem.Online（射频上电），走带超时的串行锁。
async fn set_modem_online(conn: &Connection, online: bool) -> Result<(), SerialOpError> {
    info!(online, "设置 Modem Online");
    let result = with_serial_timed(async {
        let proxy = ModemProxy::new(conn).await?;
        proxy
            .set_property("Online", zbus::zvariant::Value::Bool(online))
            .await?;
        Ok(())
    })
    .await;
    if let Err(ref e) = result {
        error!(%e, online, "设置 Modem Online 失败");
    }
    result
}

/// 周期性翻转 Modem.Online（false→等待→true）以强制射频重连。
/// 先确保 Powered=true（覆盖运行时 Powered 掉电，H1 重症场景），再 cycle Online。
/// 两次 set 之间让出全局锁，避免持锁 sleep。
async fn cycle_online(conn: &Connection) -> Result<(), SerialOpError> {
    info!("开始射频重连（cycle Online）");
    let result = (|| async {
        ensure_modem_powered(conn).await?;
        set_modem_online(conn, false).await?;
        tokio::time::sleep(Duration::from_secs(2)).await;
        set_modem_online(conn, true).await?;
        Ok(())
    })().await;
    if let Err(ref e) = result {
        error!(%e, "射频重连（cycle Online）失败");
    }
    result
}

/// 升级恢复：连续失败达阈值且冷却已过时，cycle Online 强制射频重连。
///
/// 修复 H4。仅在调用方保证“已注网”时触发——飞行模式不会注网，故不会与用户
/// 飞行模式冲突。不做自动 reboot；Powered cycle 留作后续（避免 SIM 锁风险）。
async fn try_escalate(conn: &Connection, recovery: &mut RecoveryState) -> Option<String> {
    if recovery.consecutive_failures < RECOVERY_L1_THRESHOLD {
        return None;
    }
    let now = std::time::Instant::now();
    if !recovery.can_escalate(now, RECOVERY_L1_COOLDOWN) {
        return Some(format!(
            "escalation pending cooldown ({}s left)",
            recovery
                .next_escalation_in(now, RECOVERY_L1_COOLDOWN)
                .as_secs()
        ));
    }
    recovery.last_escalation = Some(now);
    recovery.escalation_count = recovery.escalation_count.saturating_add(1);
    match cycle_online(conn).await {
        Ok(_) => {
            recovery.consecutive_failures = 0;
            Some(format!(
                "escalated: cycled Modem.Online (#{})",
                recovery.escalation_count
            ))
        }
        Err(e) => Some(format!("escalation (Online cycle) failed: {}", e)),
    }
}

/// 检查并恢复数据连接（带升级恢复）。
///
/// 被 watchdog 调用：检查数据连接状态，按 L0 重激活 → L1 cycle Online 的梯度恢复。
/// 修复 H4：原实现每 tick 只重置一次 ConnectionContext.Active，从不升级。
///
/// # Arguments
/// * `conn` - D-Bus 连接
/// * `recovery` - 看门狗持有的恢复状态机
///
/// # Returns
/// 当前状态描述字符串
async fn check_and_restore_data_connection(conn: &Connection, recovery: &mut RecoveryState) -> String {
    // 1. 检查网络注册状态
    let net_status = read_registration_status(conn).await;
    if net_status == "unknown" {
        return "Network proxy unavailable".to_string();
    }

    // 网络未注册时不尝试激活，交由 watchdog 继续 tick（不累加失败计数）
    if net_status != "registered" && net_status != "roaming" {
        return format!("Waiting for network (status: {})", net_status);
    }

    // 2. 查找 internet context
    let context_path = match find_internet_context(conn).await {
        Ok(path) => path,
        Err(e) => return format!("No internet context: {}", e),
    };

    // 3. 获取 context 属性
    let proxy = match ConnectionContextProxy::builder(conn)
        .path(context_path.as_str())
        .and_then(|b| Ok(b))
    {
        Ok(builder) => match builder.build().await {
            Ok(p) => p,
            Err(e) => return format!("Context proxy error: {}", e),
        },
        Err(e) => return format!("Context path error: {}", e),
    };

    let props = match proxy.get_properties().await {
        Ok(p) => p,
        Err(e) => return format!("Get properties error: {}", e),
    };

    let apn = props
        .get("AccessPointName")
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_default();

    let active = props
        .get("Active")
        .and_then(|v| bool::try_from(v.clone()).ok())
        .unwrap_or(false);

    // 4. 已激活且 APN 已配置 => 恢复成功，清零计数
    if active && !apn.is_empty() {
        recovery.reset();
        return format!("Connected (APN: {})", apn);
    }

    // 5. APN 为空 => 自动配置后激活
    if apn.is_empty() {
        match auto_configure_apn(conn, &context_path).await {
            Ok(msg) => match set_data_connection(conn, true).await {
                Ok(_) => {
                    recovery.reset();
                    return format!("{}, connection activated", msg);
                }
                Err(e) => {
                    recovery.record_failure();
                    return format!(
                        "{}, but activation failed: {} (failures={})",
                        msg, e, recovery.consecutive_failures
                    );
                }
            },
            Err(e) => return format!("APN not configured: {}", e),
        }
    }

    // 6. 未激活 => 累计失败、尝试激活；反复未激活则升级恢复（修复 H4）
    recovery.record_failure();
    let activate_outcome = match set_data_connection(conn, true).await {
        Ok(_) => format!("Connection restore attempted (APN: {})", apn),
        Err(e) => format!(
            "Activation failed: {} (failures={})",
            e, recovery.consecutive_failures
        ),
    };

    // 连续未激活达阈值且冷却已过 => cycle Online 强制 RF 重连
    if let Some(msg) = try_escalate(conn, recovery).await {
        return format!("{}; {}", activate_outcome, msg);
    }

    activate_outcome
}

/// 数据连接 Watchdog - 后台轮询监控并自动恢复
///
/// 持续监控数据连接状态，在断开时自动尝试恢复。
/// 支持自动识别运营商并配置 APN。
///
/// # Arguments
/// * `conn` - D-Bus 连接
/// * `interval_secs` - 检查间隔（秒）
#[tracing::instrument(skip(conn, config_manager, frontend_runtime))]
pub async fn data_connection_watchdog(
    conn: Arc<Connection>,
    config_manager: Arc<ConfigManager>,
    frontend_runtime: Arc<FrontendRuntime>,
) {
    use crate::iptables::{flush_iptables, get_iptables_rule_count};
    
    let mut last_data_log = String::new();
    let mut last_iptables_action = false; // 上次是否清空了 iptables
    let mut recovery = RecoveryState::default(); // 升级恢复状态机（修复 H4）
    
    loop {
        let refresh = config_manager.get_refresh();
        let heartbeat_timeout = Duration::from_millis(refresh.heartbeat_timeout_ms());
        let interval = if frontend_runtime.is_recent(heartbeat_timeout) {
            Duration::from_millis(refresh.active_watchdog_interval_ms())
        } else {
            Duration::from_millis(refresh.idle_watchdog_interval_ms())
        };

        tokio::time::sleep(interval).await;
        
        // 1. 检查并清空 iptables 规则
        match get_iptables_rule_count().await {
            Ok(count) => {
                if count.has_rules() {
                    // 有规则，执行清空
                    if let Err(e) = flush_iptables().await {
                        warn!(error = %e, "Watchdog: iptables flush failed");
                    } else {
                        if !last_iptables_action {
                            // 只在首次清空时打印日志
                            info!(
                                total = count.total(),
                                ipv4 = count.ipv4_rules,
                                ipv6 = count.ipv6_rules,
                                "Watchdog: iptables flushed"
                            );
                        }
                        last_iptables_action = true;
                    }
                } else {
                    // 无规则，重置标志
                    last_iptables_action = false;
                }
            }
            Err(e) => {
                warn!(error = %e, "Watchdog: iptables check failed");
            }
        }
        
        // 2. 检查并恢复数据连接（带升级恢复）
        let result = check_and_restore_data_connection(&conn, &mut recovery).await;
        
        // 只在状态变化时打印日志，避免刷屏
        if result != last_data_log {
            info!(status = %result, "Watchdog: data connection");
            last_data_log = result;
        }
    }
}

/// 获取 SIM 卡信息（整合所有 SIM 相关信息）
///
/// # Arguments
/// * `conn` - D-Bus 连接
///
/// # Returns
/// SIM 卡信息结构（整合 SimManager + MessageManager）
#[tracing::instrument(skip(conn))]
pub async fn get_sim_info_data(conn: &Connection) -> zbus::Result<SimInfoResponse> {
    let sim_proxy = SimManagerProxy::new(conn).await?;
    let msg_proxy = MessageManagerProxy::new(conn).await?;
    
    let sim_props = sim_proxy.get_properties().await?;
    let msg_props = msg_proxy.get_properties().await?;

    // 基本状态
    let present = sim_props
        .get("Present")
        .and_then(|v| bool::try_from(v.clone()).ok())
        .unwrap_or(false);

    // ICCID
    let iccid = sim_props
        .get("CardIdentifier")
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_default();

    // IMSI
    let imsi = sim_props
        .get("SubscriberIdentity")
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_default();

    // 手机号码列表
    let phone_numbers: Vec<String> = sim_props
        .get("SubscriberNumbers")
        .and_then(|v| <Vec<String>>::try_from(v.clone()).ok())
        .unwrap_or_default();

    // 短信中心
    let sms_center = msg_props
        .get("ServiceCenterAddress")
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_default();

    // MCC/MNC
    let mcc = sim_props
        .get("MobileCountryCode")
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_default();

    let mnc = sim_props
        .get("MobileNetworkCode")
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_default();

    // PIN 状态
    let pin_required = sim_props
        .get("PinRequired")
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_else(|| "none".to_string());

    // 首选语言
    let preferred_languages: Vec<String> = sim_props
        .get("PreferredLanguages")
        .and_then(|v| <Vec<String>>::try_from(v.clone()).ok())
        .unwrap_or_default();

    Ok(SimInfoResponse {
        present,
        iccid,
        imsi,
        phone_numbers,
        sms_center,
        mcc,
        mnc,
        pin_required,
        preferred_languages,
    })
}

/// 获取网络信息
///
/// # Arguments
/// * `conn` - D-Bus 连接
///
/// # Returns
/// 网络信息结构
#[tracing::instrument(skip(conn))]
pub async fn get_network_info_data(conn: &Connection) -> zbus::Result<NetworkInfoResponse> {
    let net_proxy = NetworkRegistrationProxy::new(conn).await?;
    let radio_proxy = RadioSettingsProxy::new(conn).await?;
    
    let net_props = net_proxy.get_properties().await?;
    let radio_props = radio_proxy.get_properties().await?;

    let operator_name = net_props
        .get("Name")
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_default();

    let registration_status = net_props
        .get("Status")
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_else(|| "unknown".to_string());

    let technology_preference = radio_props
        .get("TechnologyPreference")
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_default();

    let signal_strength = net_props
        .get("Strength")
        .and_then(|v| u8::try_from(v.clone()).ok())
        .unwrap_or(0);

    let mcc = net_props
        .get("MobileCountryCode")
        .and_then(|v| String::try_from(v.clone()).ok());

    let mnc = net_props
        .get("MobileNetworkCode")
        .and_then(|v| String::try_from(v.clone()).ok());

    Ok(NetworkInfoResponse {
        operator_name,
        registration_status,
        technology_preference,
        signal_strength,
        mcc,
        mnc,
    })
}

/// 获取设备信息（来自 D-Bus Modem 接口）
///
/// # Arguments
/// * `conn` - D-Bus 连接
///
/// # Returns
/// 设备信息结构
#[tracing::instrument(skip(conn))]
pub async fn get_device_info_data(conn: &Connection) -> zbus::Result<DeviceInfoResponse> {
    let proxy = ModemProxy::new(conn).await?;
    let props = proxy.get_properties().await?;

    let imei = props
        .get("Serial")
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_default();

    let manufacturer = props
        .get("Manufacturer")
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_default();

    let model = props
        .get("Model")
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_default();

    let revision = props
        .get("Revision")
        .and_then(|v| String::try_from(v.clone()).ok());

    let online = props
        .get("Online")
        .and_then(|v| bool::try_from(v.clone()).ok())
        .unwrap_or(false);

    let powered = props
        .get("Powered")
        .and_then(|v| bool::try_from(v.clone()).ok())
        .unwrap_or(false);

    Ok(DeviceInfoResponse {
        imei,
        manufacturer,
        model,
        revision,
        online,
        powered,
    })
}

/// 获取QoS信息
///
/// # Arguments
/// * `conn` - D-Bus 连接
///
/// # Returns
/// QoS信息结构
#[tracing::instrument(skip(conn))]
pub async fn get_qos_info_data(conn: &Connection) -> zbus::Result<QosInfoResponse> {
    let response = send_at_command(conn, "AT+CGEQOSRDP").await?;
    
    // 解析 +CGEQOSRDP: <cid>,<QCI>,[<DL_GBR>,<UL_GBR>],[<DL_MBR>,<UL_MBR>],[<DL_AMBR>,<UL_AMBR>]
    let parsed = parse_qos_response(&response);
    
    Ok(parsed)
}

/// 解析QoS响应
///
/// 格式: +CGEQOSRDP: <cid>,<QCI>,[<DL_GBR>,<UL_GBR>],[<DL_MBR>,<UL_MBR>],[<DL_AMBR>,<UL_AMBR>]
/// 示例: +CGEQOSRDP: 11,5,0,0,0,0,30000,30000
fn parse_qos_response(response: &str) -> QosInfoResponse {
    // 查找 +CGEQOSRDP: 开头的行
    for line in response.lines() {
        let line = line.trim();
        if line.starts_with("+CGEQOSRDP:") {
            // 提取冒号后面的部分
            if let Some(data) = line.strip_prefix("+CGEQOSRDP:") {
                let parts: Vec<&str> = data.trim().split(',').collect();
                
                if parts.len() >= 8 {
                    // 解析各个字段
                    let qci = parts.get(1).and_then(|s| s.trim().parse::<u8>().ok()).unwrap_or(0);
                    let dl_gbr = parts.get(2).and_then(|s| s.trim().parse::<u32>().ok()).unwrap_or(0);
                    let ul_gbr = parts.get(3).and_then(|s| s.trim().parse::<u32>().ok()).unwrap_or(0);
                    let dl_mbr = parts.get(4).and_then(|s| s.trim().parse::<u32>().ok()).unwrap_or(0);
                    let ul_mbr = parts.get(5).and_then(|s| s.trim().parse::<u32>().ok()).unwrap_or(0);
                    let dl_ambr = parts.get(6).and_then(|s| s.trim().parse::<u32>().ok()).unwrap_or(0);
                    let ul_ambr = parts.get(7).and_then(|s| s.trim().parse::<u32>().ok()).unwrap_or(0);
                    
                    // 优先使用 GBR，如果为0则使用 MBR，如果还是0则使用 AMBR
                    let dl_speed = if dl_gbr > 0 { dl_gbr } else if dl_mbr > 0 { dl_mbr } else { dl_ambr };
                    let ul_speed = if ul_gbr > 0 { ul_gbr } else if ul_mbr > 0 { ul_mbr } else { ul_ambr };
                    
                    return QosInfoResponse {
                        qci,
                        dl_speed,
                        ul_speed,
                        raw_response: None, // 不返回原始响应，保持简洁
                    };
                }
            }
        }
    }
    
    // 如果解析失败，返回默认值
    QosInfoResponse {
        qci: 0,
        dl_speed: 0,
        ul_speed: 0,
        raw_response: Some(response.to_string()),
    }
}

/// 从多个可能的键中解析 u32 值
///
/// 不同的 udx710 设备可能使用不同的键名和值类型
fn parse_u32_from_keys(cell_info: &HashMap<String, OwnedValue>, keys: &[&str]) -> u32 {
    for key in keys {
        if let Some(value) = cell_info.get(*key) {
            // 尝试直接转换为 u32
            if let Ok(num) = u32::try_from(value) {
                return num;
            }
            // 尝试转换为字符串后再解析
            if let Ok(s) = String::try_from(value.clone()) {
                // 尝试十进制解析
                if let Ok(num) = s.parse::<u32>() {
                    return num;
                }
                // 尝试十六进制解析
                if let Ok(num) = u32::from_str_radix(&s, 16) {
                    return num;
                }
            }
        }
    }
    0
}

/// 设置飞行模式
///
/// # Arguments
/// * `conn` - D-Bus 连接
/// * `enabled` - true 开启飞行模式（关闭射频），false 关闭飞行模式（开启射频）
///
/// # Returns
/// 操作结果
///
/// # 说明
/// 飞行模式通过设置 Modem 的 Online 属性实现：
/// - Online = false: 关闭射频，进入飞行模式（但 Modem 保持上电）
/// - Online = true: 开启射频，退出飞行模式
#[tracing::instrument(skip(conn))]
pub async fn set_airplane_mode(conn: &Connection, enabled: bool) -> Result<(), SerialOpError> {
    // 飞行模式：设置 Online 为相反值
    // enabled=true 表示开启飞行模式，即 Online=false
    // 复用 set_modem_online（带超时的串行锁，修复 H3）
    info!(enabled, "设置飞行模式");
    let result = set_modem_online(conn, !enabled).await;
    if let Err(ref e) = result {
        error!(%e, enabled, "设置飞行模式失败");
    }
    result
}

/// 获取飞行模式状态
///
/// # Arguments
/// * `conn` - D-Bus 连接
///
/// # Returns
/// 飞行模式响应结构，包含飞行模式状态、Powered 和 Online 属性
///
/// # 说明
/// 飞行模式状态判断：
/// - enabled = !Online (Online=false 表示飞行模式已启用)
#[tracing::instrument(skip(conn))]
pub async fn get_airplane_mode(conn: &Connection) -> zbus::Result<AirplaneModeResponse> {
    let proxy = ModemProxy::new(conn).await?;
    let props = proxy.get_properties().await?;
    
    let powered = props
        .get("Powered")
        .and_then(|v| bool::try_from(v.clone()).ok())
        .unwrap_or(false);
    
    let online = props
        .get("Online")
        .and_then(|v| bool::try_from(v.clone()).ok())
        .unwrap_or(false);
    
    // 飞行模式状态：Online=false 表示飞行模式已启用
    let enabled = !online;
    
    Ok(AirplaneModeResponse {
        enabled,
        powered,
        online,
    })
}

/// 获取射频模式
///
/// # Arguments
/// * `conn` - D-Bus 连接
///
/// # Returns
/// 射频模式响应结构
///
/// # 说明
/// 通过 RadioSettings.GetProperties 获取 TechnologyPreference 属性
#[tracing::instrument(skip(conn))]
pub async fn get_radio_mode(conn: &Connection) -> zbus::Result<RadioModeResponse> {
    let result = with_serial(async {
        let proxy = RadioSettingsProxy::new(conn).await?;
        let props = proxy.get_properties().await?;
        
        let technology_preference = props
            .get("TechnologyPreference")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_else(|| "unknown".to_string());
        
        // 尝试映射为标准模式
        let mode = RadioMode::from_ofono_value(&technology_preference)
            .map(|m| match m {
                RadioMode::Auto => "auto",
                RadioMode::LteOnly => "lte",
                RadioMode::NrOnly => "nr",
            })
            .unwrap_or("unknown")
            .to_string();
        
        Ok(RadioModeResponse {
            mode,
            technology_preference,
        })
    }).await;
    if let Err(ref e) = result {
        error!(%e, "获取射频模式失败");
    }
    result
}

/// 设置射频模式
///
/// # Arguments
/// * `conn` - D-Bus 连接
/// * `mode` - 目标射频模式
///
/// # Returns
/// 操作结果
///
/// # 说明
/// 通过 RadioSettings.SetProperty 设置 TechnologyPreference 属性
#[tracing::instrument(skip(conn))]
pub async fn set_radio_mode(conn: &Connection, mode: RadioMode) -> zbus::Result<()> {
    info!(?mode, "设置射频模式");
    let result = with_serial(async {
        let proxy = RadioSettingsProxy::new(conn).await?;
        let ofono_value = mode.to_ofono_value();
        
        proxy
            .set_property(
                "TechnologyPreference",
                zbus::zvariant::Value::Str(ofono_value.into()),
            )
            .await?;
        
        Ok(())
    }).await;
    if let Err(ref e) = result {
        error!(%e, ?mode, "设置射频模式失败");
    }
    result
}

// ============ 电话相关 D-Bus 接口 ============

use crate::models::CallInfo;

/// ofono VoiceCallManager 代理接口
#[proxy(
    interface = "org.ofono.VoiceCallManager",
    default_service = "org.ofono",
    default_path = "/ril_0",
    assume_defaults = true
)]
pub trait VoiceCallManager {
    /// 获取所有通话
    fn get_calls(&self) -> zbus::Result<Vec<(zbus::zvariant::OwnedObjectPath, HashMap<String, OwnedValue>)>>;
    
    /// 拨打电话
    fn dial(&self, number: &str, hide_callerid: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
    
    /// 挂断所有通话
    fn hangup_all(&self) -> zbus::Result<()>;
}

/// ofono VoiceCall 代理接口（单个通话）
#[proxy(
    interface = "org.ofono.VoiceCall",
    default_service = "org.ofono",
    assume_defaults = true
)]
pub trait VoiceCall {
    /// 挂断此通话
    fn hangup(&self) -> zbus::Result<()>;
    
    /// 接听来电
    fn answer(&self) -> zbus::Result<()>;
    
    /// 获取通话属性
    fn get_properties(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
}

/// 获取当前活动的通话列表
#[tracing::instrument(skip(conn))]
pub async fn get_active_calls(conn: &Connection) -> zbus::Result<Vec<CallInfo>> {
    let result = with_serial(async {
        let proxy = VoiceCallManagerProxy::new(conn).await?;
        let calls = proxy.get_calls().await?;
        
        let mut result = Vec::new();
        for (path, props) in calls {
            let phone_number = props
                .get("LineIdentification")
                .and_then(|v| String::try_from(v.clone()).ok())
                .unwrap_or_else(|| "Unknown".to_string());
            
            let state = props
                .get("State")
                .and_then(|v| String::try_from(v.clone()).ok())
                .unwrap_or_else(|| "unknown".to_string());
            
            let start_time = props
                .get("StartTime")
                .and_then(|v| String::try_from(v.clone()).ok());
            
            // 判断方向：incoming 或 outgoing
            let direction = if state == "incoming" {
                "incoming".to_string()
            } else {
                "outgoing".to_string()
            };
            
            result.push(CallInfo {
                path: path.to_string(),
                phone_number,
                state,
                direction,
                start_time,
            });
        }
        
        Ok(result)
    }).await;
    if let Err(ref e) = result {
        error!(%e, "获取活动通话列表失败");
    }
    result
}

/// 拨打电话
#[tracing::instrument(skip(conn))]
pub async fn dial_call(conn: &Connection, phone_number: &str) -> zbus::Result<CallInfo> {
    info!(number = %phone_number, "拨打电话");
    let result = with_serial(async {
        let proxy = VoiceCallManagerProxy::new(conn).await?;
        let path = proxy.dial(phone_number, "default").await?;
        
        Ok(CallInfo {
            path: path.to_string(),
            phone_number: phone_number.to_string(),
            state: "dialing".to_string(),
            direction: "outgoing".to_string(),
            start_time: Some(chrono::Utc::now().to_rfc3339()),
        })
    }).await;
    match &result {
        Ok(_) => info!(number = %phone_number, "拨打电话成功"),
        Err(e) => error!(%e, number = %phone_number, "拨打电话失败"),
    }
    result
}

/// 挂断指定通话
#[tracing::instrument(skip(conn))]
pub async fn hangup_call(conn: &Connection, call_path: &str) -> zbus::Result<()> {
    info!(call_path = %call_path, "挂断通话");
    let result = with_serial(async {
        let proxy = VoiceCallProxy::builder(conn)
            .path(call_path)?
            .build()
            .await?;
        
        proxy.hangup().await
    }).await;
    if let Err(ref e) = result {
        error!(%e, call_path = %call_path, "挂断通话失败");
    }
    result
}

/// 挂断所有通话
#[tracing::instrument(skip(conn))]
pub async fn hangup_all_calls(conn: &Connection) -> zbus::Result<usize> {
    let result = with_serial(async {
        let proxy = VoiceCallManagerProxy::new(conn).await?;
        let calls = proxy.get_calls().await?;
        let count = calls.len();
        
        if count > 0 {
            proxy.hangup_all().await?;
        }
        
        Ok(count)
    }).await;
    match &result {
        Ok(count) => info!(count, "挂断所有通话成功"),
        Err(e) => error!(%e, "挂断所有通话失败"),
    }
    result
}

/// 接听来电
#[tracing::instrument(skip(conn))]
pub async fn answer_call(conn: &Connection, call_path: &str) -> zbus::Result<()> {
    info!(call_path = %call_path, "接听来电");
    let result = with_serial(async {
        let proxy = VoiceCallProxy::builder(conn)
            .path(call_path)?
            .build()
            .await?;
        
        proxy.answer().await
    }).await;
    match &result {
        Ok(_) => info!("接听来电成功"),
        Err(e) => error!(%e, call_path = %call_path, "接听来电失败"),
    }
    result
}

// ============ 短信相关 D-Bus 接口 ============

/// 发送短信
#[tracing::instrument(skip(conn, content))]
pub async fn send_sms(conn: &Connection, phone_number: &str, content: &str) -> zbus::Result<String> {
    info!(to = %phone_number, len = content.len(), "发送短信");
    let result = with_serial(async {
        let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.MessageManager").await?;
        let message_path: zbus::zvariant::OwnedObjectPath = proxy.call("SendMessage", &(phone_number, content)).await?;
        Ok(message_path.to_string())
    }).await;
    match &result {
        Ok(path) => info!(to = %phone_number, path = %path, "发送短信成功"),
        Err(e) => error!(%e, to = %phone_number, "发送短信失败"),
    }
    result
}

// ============ 新增功能接口 ============

use crate::models::{
    ImeisvResponse, SignalStrengthResponse, CallForwardingResponse, CallSettingsResponse,
    OperatorInfo, OperatorListResponse, NitzTimeResponse, ImsStatusResponse,
    CallVolumeResponse, VoicemailStatusResponse,
};

/// 获取 IMEISV（软件版本号）
#[tracing::instrument(skip(conn))]
pub async fn get_imeisv(conn: &Connection) -> zbus::Result<ImeisvResponse> {
    let result = with_serial(async {
        let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.Modem").await?;
        let result: HashMap<String, OwnedValue> = proxy.call("GetImeisv", &()).await?;
        
        let svn = result
            .get("SoftwareVersionNumber")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_else(|| "Unknown".to_string());
        
        Ok(ImeisvResponse {
            software_version_number: svn,
        })
    }).await;
    if let Err(ref e) = result {
        error!(%e, "获取 IMEISV 失败");
    }
    result
}

/// 获取信号强度详细信息
#[tracing::instrument(skip(conn))]
pub async fn get_signal_strength(conn: &Connection) -> zbus::Result<SignalStrengthResponse> {
    debug!("查询信号强度");
    let result = with_serial(async {
        let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.NetworkRegistration").await?;
        let result: HashMap<String, OwnedValue> = proxy.call("GetSignalStrength", &()).await?;
        
        let strength = result
            .get("Strength")
            .and_then(|v| i32::try_from(v.clone()).ok())
            .unwrap_or(0);
        
        Ok(SignalStrengthResponse { strength })
    }).await;
    match &result {
        Ok(resp) => debug!(strength = resp.strength, "信号强度查询结果"),
        Err(e) => error!(%e, "查询信号强度失败"),
    }
    result
}

/// 获取 NITZ 网络时间
#[tracing::instrument(skip(conn))]
pub async fn get_nitz_time(conn: &Connection) -> zbus::Result<NitzTimeResponse> {
    let result = with_serial(async {
        let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.Modem").await?;
        
        match proxy.call("GetNITZ", &()).await {
            Ok(time_string) => Ok(NitzTimeResponse {
                time_string,
                available: true,
            }),
            Err(e) => {
                error!(%e, "获取 NITZ 网络时间失败");
                Ok(NitzTimeResponse {
                    time_string: String::new(),
                    available: false,
                })
            },
        }
    }).await;
    if let Err(ref e) = result {
        error!(%e, "获取 NITZ 时间 D-Bus 调用失败");
    }
    result
}

/// 获取 IMS 状态
#[tracing::instrument(skip(conn))]
pub async fn get_ims_status(conn: &Connection) -> zbus::Result<ImsStatusResponse> {
    let result = with_serial(async {
        let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.IpMultimediaSystem").await?;
        let props: HashMap<String, OwnedValue> = proxy.call("GetProperties", &()).await?;
        
        let registered = props
            .get("Registered")
            .and_then(|v| bool::try_from(v.clone()).ok())
            .unwrap_or(false);
        
        let voice_capable = props
            .get("VoiceCapable")
            .and_then(|v| bool::try_from(v.clone()).ok())
            .unwrap_or(false);
        
        let sms_capable = props
            .get("SmsCapable")
            .and_then(|v| bool::try_from(v.clone()).ok())
            .unwrap_or(false);
        
        Ok(ImsStatusResponse {
            registered,
            voice_capable,
            sms_capable,
        })
    }).await;
    if let Err(ref e) = result {
        error!(%e, "获取 IMS 状态失败");
    }
    result
}

/// 获取通话音量
#[tracing::instrument(skip(conn))]
pub async fn get_call_volume(conn: &Connection) -> zbus::Result<CallVolumeResponse> {
    let result = with_serial(async {
        let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.CallVolume").await?;
        let props: HashMap<String, OwnedValue> = proxy.call("GetProperties", &()).await?;
        
        let speaker_volume = props
            .get("SpeakerVolume")
            .and_then(|v| u8::try_from(v.clone()).ok())
            .unwrap_or(0);
        
        let microphone_volume = props
            .get("MicrophoneVolume")
            .and_then(|v| u8::try_from(v.clone()).ok())
            .unwrap_or(0);
        
        let muted = props
            .get("Muted")
            .and_then(|v| bool::try_from(v.clone()).ok())
            .unwrap_or(false);
        
        Ok(CallVolumeResponse {
            speaker_volume,
            microphone_volume,
            muted,
        })
    }).await;
    if let Err(ref e) = result {
        error!(%e, "获取通话音量失败");
    }
    result
}

/// 设置通话音量
#[tracing::instrument(skip(conn))]
pub async fn set_call_volume(
    conn: &Connection,
    speaker: Option<u8>,
    microphone: Option<u8>,
    muted: Option<bool>,
) -> zbus::Result<()> {
    let result = with_serial(async {
        let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.CallVolume").await?;
        
        if let Some(vol) = speaker {
            let val = zbus::zvariant::Value::new(vol);
            proxy.call::<_, _, ()>("SetProperty", &("SpeakerVolume", val)).await?;
        }
        
        if let Some(vol) = microphone {
            let val = zbus::zvariant::Value::new(vol);
            proxy.call::<_, _, ()>("SetProperty", &("MicrophoneVolume", val)).await?;
        }
        
        if let Some(m) = muted {
            let val = zbus::zvariant::Value::new(m);
            proxy.call::<_, _, ()>("SetProperty", &("Muted", val)).await?;
        }
        
        Ok(())
    }).await;
    match &result {
        Ok(_) => info!("设置通话音量成功"),
        Err(e) => error!(%e, "设置通话音量失败"),
    }
    result
}

/// 获取语音留言状态
#[tracing::instrument(skip(conn))]
pub async fn get_voicemail_status(conn: &Connection) -> zbus::Result<VoicemailStatusResponse> {
    let result = with_serial(async {
        let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.MessageWaiting").await?;
        let props: HashMap<String, OwnedValue> = proxy.call("GetProperties", &()).await?;
        
        let waiting = props
            .get("VoicemailWaiting")
            .and_then(|v| bool::try_from(v.clone()).ok())
            .unwrap_or(false);
        
        let message_count = props
            .get("VoicemailMessageCount")
            .and_then(|v| u8::try_from(v.clone()).ok())
            .unwrap_or(0);
        
        let mailbox_number = props
            .get("VoicemailMailboxNumber")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_else(|| String::new());
        
        Ok(VoicemailStatusResponse {
            waiting,
            message_count,
            mailbox_number,
        })
    }).await;
    if let Err(ref e) = result {
        error!(%e, "获取语音留言状态失败");
    }
    result
}

/// 获取运营商列表（快速，仅返回当前）
#[tracing::instrument(skip(conn))]
pub async fn get_operators(conn: &Connection) -> zbus::Result<OperatorListResponse> {
    let result = with_serial(async {
        let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.NetworkRegistration").await?;
        let result: Vec<(zbus::zvariant::OwnedObjectPath, HashMap<String, OwnedValue>)> = 
            proxy.call("GetOperators", &()).await?;
        
        let mut operators = Vec::new();
        for (path, props) in result {
            operators.push(parse_operator_info(path.to_string(), props));
        }
        
        Ok(OperatorListResponse { operators })
    }).await;
    if let Err(ref e) = result {
        error!(%e, "获取运营商列表失败");
    }
    result
}

/// 扫描运营商（慢，返回所有可用）
#[tracing::instrument(skip(conn))]
pub async fn scan_operators(conn: &Connection) -> zbus::Result<OperatorListResponse> {
    info!("扫描运营商网络");
    let result = with_serial(async {
        let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.NetworkRegistration").await?;
        let result: Vec<(zbus::zvariant::OwnedObjectPath, HashMap<String, OwnedValue>)> = 
            proxy.call("Scan", &()).await?;
        
        let mut operators = Vec::new();
        for (path, props) in result {
            operators.push(parse_operator_info(path.to_string(), props));
        }
        
        Ok(OperatorListResponse { operators })
    }).await;
    match &result {
        Ok(resp) => info!(count = resp.operators.len(), "运营商扫描完成"),
        Err(e) => error!(%e, "扫描运营商失败"),
    }
    result
}

/// 解析运营商信息
fn parse_operator_info(path: String, props: HashMap<String, OwnedValue>) -> OperatorInfo {
    let name = props
        .get("Name")
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_else(|| "Unknown".to_string());
    
    let status = props
        .get("Status")
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_else(|| "unknown".to_string());
    
    let mcc = props
        .get("MobileCountryCode")
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_else(|| "".to_string());
    
    let mnc = props
        .get("MobileNetworkCode")
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_else(|| "".to_string());
    
    let technologies: Vec<String> = props
        .get("Technologies")
        .and_then(|v| {
            // 尝试将 Value 转换为数组
            if let Ok(arr) = <Vec<String>>::try_from(v.clone()) {
                Some(arr)
            } else {
                None
            }
        })
        .unwrap_or_else(Vec::new);
    
    OperatorInfo {
        path,
        name,
        status,
        mcc,
        mnc,
        technologies,
    }
}

/// 手动注册到指定运营商
#[tracing::instrument(skip(conn))]
pub async fn register_operator_manual(conn: &Connection, mccmnc: &str) -> zbus::Result<()> {
    info!(mccmnc = %mccmnc, "手动注册运营商");
    let result = with_serial(async {
        let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.NetworkRegistration").await?;
        proxy.call("RegisterManually", &(mccmnc, "")).await
    }).await;
    match &result {
        Ok(_) => info!(mccmnc = %mccmnc, "手动注册运营商成功"),
        Err(e) => error!(%e, mccmnc = %mccmnc, "手动注册运营商失败"),
    }
    result
}

/// 自动注册运营商
#[tracing::instrument(skip(conn))]
pub async fn register_operator_auto(conn: &Connection) -> zbus::Result<()> {
    info!("自动注册运营商");
    let result = with_serial(async {
        let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.NetworkRegistration").await?;
        proxy.call("Register", &()).await
    }).await;
    match &result {
        Ok(_) => info!("自动注册运营商成功"),
        Err(e) => error!(%e, "自动注册运营商失败"),
    }
    result
}

/// 获取呼叫转移设置
#[tracing::instrument(skip(conn))]
pub async fn get_call_forwarding(conn: &Connection) -> zbus::Result<CallForwardingResponse> {
    let result = with_serial(async {
        let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.CallForwarding").await?;
        let props: HashMap<String, OwnedValue> = proxy.call("GetProperties", &()).await?;
        
        let voice_unconditional = props
            .get("VoiceUnconditional")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_else(|| String::new());
        
        let voice_busy = props
            .get("VoiceBusy")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_else(|| String::new());
        
        let voice_no_reply = props
            .get("VoiceNoReply")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_else(|| String::new());
        
        let voice_no_reply_timeout = props
            .get("VoiceNoReplyTimeout")
            .and_then(|v| u16::try_from(v.clone()).ok())
            .unwrap_or(20);
        
        let voice_not_reachable = props
            .get("VoiceNotReachable")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_else(|| String::new());
        
        let forwarding_flag_on_sim = props
            .get("ForwardingFlagOnSim")
            .and_then(|v| bool::try_from(v.clone()).ok())
            .unwrap_or(false);
        
        Ok(CallForwardingResponse {
            voice_unconditional,
            voice_busy,
            voice_no_reply,
            voice_no_reply_timeout,
            voice_not_reachable,
            forwarding_flag_on_sim,
        })
    }).await;
    if let Err(ref e) = result {
        error!(%e, "获取呼叫转移设置失败");
    }
    result
}

/// 设置呼叫转移
#[tracing::instrument(skip_all)]  // skip_all: number 为电话号码 PII，避免入 span
pub async fn set_call_forwarding(
    conn: &Connection,
    forward_type: &str,
    number: &str,
    timeout: Option<u16>,
) -> zbus::Result<()> {
    info!(forward_type = %forward_type, number = %number, "设置呼叫转移");
    let result = with_serial(async {
        let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.CallForwarding").await?;
        
        let property_name = match forward_type {
            "unconditional" => "VoiceUnconditional",
            "busy" => "VoiceBusy",
            "noreply" => "VoiceNoReply",
            "notreachable" => "VoiceNotReachable",
            _ => return Err(zbus::Error::Failure("Invalid forward type".to_string())),
        };
        
        let number_value = zbus::zvariant::Value::new(number);
        proxy.call::<_, _, ()>("SetProperty", &(property_name, number_value)).await?;
        
        // 如果是 noreply 类型且提供了超时时间
        if forward_type == "noreply" && timeout.is_some() {
            let timeout_value = zbus::zvariant::Value::new(timeout.unwrap());
            proxy.call::<_, _, ()>("SetProperty", &("VoiceNoReplyTimeout", timeout_value)).await?;
        }
        
        Ok(())
    }).await;
    if let Err(ref e) = result {
        error!(%e, forward_type = %forward_type, "设置呼叫转移失败");
    }
    result
}

/// 获取通话设置
#[tracing::instrument(skip(conn))]
pub async fn get_call_settings(conn: &Connection) -> zbus::Result<CallSettingsResponse> {
    let result = with_serial(async {
        let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.CallSettings").await?;
        let props: HashMap<String, OwnedValue> = proxy.call("GetProperties", &()).await?;
        
        let calling_line_presentation = props
            .get("CallingLinePresentation")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_else(|| "unknown".to_string());
        
        let calling_name_presentation = props
            .get("CallingNamePresentation")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_else(|| "unknown".to_string());
        
        let connected_line_presentation = props
            .get("ConnectedLinePresentation")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_else(|| "unknown".to_string());
        
        let connected_line_restriction = props
            .get("ConnectedLineRestriction")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_else(|| "unknown".to_string());
        
        let called_line_presentation = props
            .get("CalledLinePresentation")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_else(|| "unknown".to_string());
        
        let calling_line_restriction = props
            .get("CallingLineRestriction")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_else(|| "unknown".to_string());
        
        let hide_caller_id = props
            .get("HideCallerId")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_else(|| "default".to_string());
        
        let voice_call_waiting = props
            .get("VoiceCallWaiting")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_else(|| "unknown".to_string());
        
        Ok(CallSettingsResponse {
            calling_line_presentation,
            calling_name_presentation,
            connected_line_presentation,
            connected_line_restriction,
            called_line_presentation,
            calling_line_restriction,
            hide_caller_id,
            voice_call_waiting,
        })
    }).await;
    if let Err(ref e) = result {
        error!(%e, "获取通话设置失败");
    }
    result
}

/// 设置通话设置
#[tracing::instrument(skip_all)]
pub async fn set_call_setting(conn: &Connection, property: &str, value: &str) -> zbus::Result<()> {
    info!(property = %property, value = %value, "设置通话设置");
    let result = with_serial(async {
        let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.CallSettings").await?;
        let value_variant = zbus::zvariant::Value::new(value);
        proxy.call("SetProperty", &(property, value_variant)).await
    }).await;
    if let Err(ref e) = result {
        error!(%e, property = %property, "设置通话设置失败");
    }
    result
}

// ============ SIM 卡槽功能 ============

use crate::models::SimSlotResponse;

/// 获取 SIM 卡槽信息
#[tracing::instrument(skip(conn))]
pub async fn get_sim_slot(conn: &Connection) -> zbus::Result<SimSlotResponse> {
    let result = with_serial(async {
        let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.Modem").await?;
        let response: String = proxy.call("SendAtcmd", &("AT+SPCONFIGSIMSLOT?")).await?;
        
        // 解析响应：+SPCONFIGSIMSLOT: 66051
        let raw_value = response
            .lines()
            .find(|line| line.contains("+SPCONFIGSIMSLOT:"))
            .and_then(|line| line.split(':').nth(1))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| String::new());
        
        // 根据值判断卡槽（这个需要根据实际设备的规则来解析）
        // 66051 可能表示卡槽 1，66306 可能表示卡槽 2
        // 您需要提供切换命令来确认规则
        let active_slot = if raw_value.contains("66051") { 1 } else if raw_value.contains("66306") { 2 } else { 0 };
        
        Ok(SimSlotResponse {
            active_slot,
            raw_value,
        })
    }).await;
    if let Err(ref e) = result {
        error!(%e, "获取 SIM 卡槽信息失败");
    }
    result
}

/// 切换 SIM 卡槽
#[tracing::instrument(skip(conn))]
pub async fn switch_sim_slot(conn: &Connection, slot: u8) -> zbus::Result<String> {
    info!(slot, "切换 SIM 卡槽");
    let result = with_serial(async {
        let proxy = Proxy::new(conn, "org.ofono", "/ril_0", "org.ofono.Modem").await?;
        
        // 根据卡槽号生成 AT 命令
        // 注意：这个命令格式需要根据您的设备文档确认
        // 可能是 AT+SPCONFIGSIMSLOT=1 或 AT+SPCONFIGSIMSLOT=66051
        let value = match slot {
            1 => "66051",  // 卡槽 1 的值
            2 => "66306",  // 卡槽 2 的值（猜测，需要您确认）
            _ => return Err(zbus::Error::Failure("Invalid slot number, must be 1 or 2".to_string())),
        };
        
        let cmd = format!("AT+SPCONFIGSIMSLOT={}", value);
        let response: String = proxy.call("SendAtcmd", &(cmd.as_str())).await?;
        
        Ok(response)
    }).await;
    match &result {
        Ok(_) => info!(slot, "切换 SIM 卡槽成功"),
        Err(e) => error!(%e, slot, "切换 SIM 卡槽失败"),
    }
    result
}

