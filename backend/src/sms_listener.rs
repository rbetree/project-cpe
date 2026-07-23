/*
 * @Author: 1orz cloudorzi@gmail.com
 * @Date: 2025-12-07 07:33:11
 * @LastEditors: 1orz cloudorzi@gmail.com
 * @LastEditTime: 2025-12-13 12:46:16
 * @FilePath: /udx710-backend/backend/src/sms_listener.rs
 * @Description:
 *
 * Copyright (c) 2025 by 1orz, All Rights Reserved.
 */
//! SMS Listener Module
//!
//! Listens for incoming SMS via D-Bus signals and stores them in the database.
//!
//! Copyright (c) 2025 1orz
//! https://github.com/1orz/project-cpe

use crate::db::{CallRecord, Database, SmsMessage};
use crate::sms_push::SmsPushSender;
use crate::webhook::WebhookSender;
use futures_util::StreamExt;
use std::sync::Arc;
use tracing::{error, info, warn};
use zbus::zvariant::OwnedValue;
use zbus::{Connection, MessageStream, Proxy};

/// PDU decode result
#[allow(dead_code)]
pub struct PduDecodeResult {
    pub sender: String,
    pub content: String,
    pub is_multipart: bool,
    pub reference: u8,
    pub total_parts: u8,
    pub part_number: u8,
}

/// PDU decoder (improved version)
/// Supports concatenated SMS (with UDH) and regular SMS
#[allow(dead_code)]
pub fn decode_pdu_simple(pdu_hex: &str) -> Option<(String, String)> {
    let result = decode_pdu_full(pdu_hex)?;
    Some((result.sender, result.content))
}

/// Full PDU decode (includes multipart SMS info)
pub fn decode_pdu_full(pdu_hex: &str) -> Option<PduDecodeResult> {
    let pdu = pdu_hex.trim().to_uppercase();
    if pdu.len() < 20 {
        return None;
    }

    // 1. Skip SMSC (SMS center)
    let smsc_len = u8::from_str_radix(&pdu[0..2], 16).ok()? as usize;
    let mut pos = 2 + smsc_len * 2;

    if pos + 2 > pdu.len() {
        return None;
    }

    // 2. PDU type (first byte)
    let pdu_type = u8::from_str_radix(&pdu[pos..pos + 2], 16).ok()?;
    let has_udh = (pdu_type & 0x40) != 0; // Check UDHI bit
    pos += 2;

    if pos + 2 > pdu.len() {
        return None;
    }

    // 3. Sender address length
    let sender_len = u8::from_str_radix(&pdu[pos..pos + 2], 16).ok()? as usize;
    pos += 2;

    // 4. Sender type
    if pos + 2 > pdu.len() {
        return None;
    }
    pos += 2;

    // 5. Sender number (BCD encoded)
    let sender_digits_len = if sender_len % 2 == 0 {
        sender_len
    } else {
        sender_len + 1
    };
    if pos + sender_digits_len > pdu.len() {
        return None;
    }

    let sender_hex = &pdu[pos..pos + sender_digits_len];
    let sender = decode_bcd_number(sender_hex);
    pos += sender_digits_len;

    // 6. PID (1 byte)
    if pos + 2 > pdu.len() {
        return None;
    }
    pos += 2;

    // 7. DCS (1 byte) - Data Coding Scheme
    if pos + 2 > pdu.len() {
        return None;
    }
    let dcs = u8::from_str_radix(&pdu[pos..pos + 2], 16).ok()?;
    pos += 2;

    // 8. Timestamp (7 bytes = 14 hex chars)
    if pos + 14 > pdu.len() {
        return None;
    }
    pos += 14;

    // 9. User data length
    if pos + 2 > pdu.len() {
        return None;
    }
    let _ud_len = u8::from_str_radix(&pdu[pos..pos + 2], 16).ok()? as usize;
    pos += 2;

    // 10. Process user data
    let mut is_multipart = false;
    let mut reference: u8 = 0;
    let mut total_parts: u8 = 1;
    let mut part_number: u8 = 1;

    // If UDH (User Data Header) exists, parse it first
    if has_udh && pos + 2 <= pdu.len() {
        let udh_len = u8::from_str_radix(&pdu[pos..pos + 2], 16).ok()? as usize;
        pos += 2;

        // Parse UDH content
        if udh_len >= 5 && pos + udh_len * 2 <= pdu.len() {
            let udh_data = &pdu[pos..pos + udh_len * 2];

            // Check if concatenated SMS (IEI = 0x00)
            if udh_data.len() >= 10 && &udh_data[0..2] == "00" && &udh_data[2..4] == "03" {
                is_multipart = true;
                reference = u8::from_str_radix(&udh_data[4..6], 16).ok()?;
                total_parts = u8::from_str_radix(&udh_data[6..8], 16).ok()?;
                part_number = u8::from_str_radix(&udh_data[8..10], 16).ok()?;
            }
        }

        // Skip entire UDH
        pos += udh_len * 2;
    }

    // 11. Decode message content
    if pos >= pdu.len() {
        return None;
    }

    let user_data = &pdu[pos..];

    // Determine encoding based on DCS
    let content = if (dcs & 0x08) != 0 || dcs == 0x08 {
        // UCS2 encoding (UTF-16BE)
        decode_ucs2(user_data).ok()?
    } else if dcs == 0x00 || (dcs & 0xF0) == 0x00 {
        // GSM 7-bit encoding (simplified: try as ASCII)
        decode_ucs2(user_data).unwrap_or_else(|_| {
            // Fallback: treat as hex string
            user_data.to_string()
        })
    } else {
        // Other encodings, try UCS2
        decode_ucs2(user_data).ok()?
    };

    Some(PduDecodeResult {
        sender,
        content,
        is_multipart,
        reference,
        total_parts,
        part_number,
    })
}

/// Decode BCD encoded phone number
fn decode_bcd_number(hex: &str) -> String {
    let mut result = String::new();
    for i in (0..hex.len()).step_by(2) {
        if i + 1 < hex.len() {
            let second = &hex[i + 1..i + 2];
            let first = &hex[i..i + 1];

            if second != "F" && second != "f" {
                result.push_str(second);
            }
            if first != "F" && first != "f" {
                result.push_str(first);
            }
        }
    }
    result
}

/// Decode UCS2 (UTF-16BE) encoding
fn decode_ucs2(hex: &str) -> Result<String, String> {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect();

    // UTF-16BE decode
    let utf16_values: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
        .collect();

    String::from_utf16(&utf16_values).map_err(|e| format!("UTF-16 decode error: {}", e))
}

/// Start SMS listener with webhook and SMS push support
pub async fn start_sms_listener(
    conn: Connection,
    db: Arc<Database>,
    webhook: Arc<WebhookSender>,
    sms_push: Arc<SmsPushSender>,
) -> zbus::Result<()> {
    // Subscribe to D-Bus signals via proxy
    let dbus_proxy = Proxy::new(
        &conn,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .await?;

    // Only listen to IncomingMessage signal (ofono auto-assembles long SMS)
    // Note: MessagePDU is not monitored to avoid duplicate SMS notifications
    let rule = "type='signal',sender='org.ofono',interface='org.ofono.MessageManager',member='IncomingMessage'";
    dbus_proxy.call::<_, _, ()>("AddMatch", &(rule,)).await?;

    // Create message stream
    let mut stream = MessageStream::from(&conn);

    // Listen for signals
    loop {
        let msg = match stream.next().await {
            Some(Ok(msg)) => msg,
            Some(Err(_)) => {
                // 瞬时错误：短暂 sleep 避免 busy-spin 占满 CPU
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                continue;
            }
            None => {
                // 流结束（D-Bus 连接断开）→ 返回，由外层 run_*_with_reconnect 重连
                return Ok(());
            }
        };

        // Check if it's a signal message
        if let Some(member) = msg.header().member() {
            if member.as_str() == "IncomingMessage" {
                // Parse IncomingMessage format (text format)
                if let Ok((content, props)) = msg
                    .body()
                    .deserialize::<(String, std::collections::HashMap<String, OwnedValue>)>()
                {
                    // Extract sender from properties
                    let sender = props
                        .get("Sender")
                        .and_then(|v| v.downcast_ref::<zbus::zvariant::Str>().ok())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "Unknown".to_string());

                    info!(from = %sender, len = content.len(), "收到短信");

                    // Store to database
                    match db.insert_sms("incoming", &sender, &content, "received", None) {
                        Ok(id) => {
                            // Forward to webhook / SMS push
                            let sms = SmsMessage {
                                id,
                                direction: "incoming".to_string(),
                                phone_number: sender,
                                content,
                                timestamp: chrono::Utc::now().to_rfc3339(),
                                status: "received".to_string(),
                                pdu: None,
                            };
                            let webhook_clone = Arc::clone(&webhook);
                            let sms_push_clone = Arc::clone(&sms_push);
                            tokio::spawn(async move {
                                if let Err(e) = webhook_clone.forward_sms(&sms).await {
                                    error!(%e, "短信 Webhook 转发失败");
                                }
                                if let Err(e) = sms_push_clone.forward_sms(&sms).await {
                                    error!(%e, "短信 Push 转发失败");
                                }
                            });
                        }
                        Err(e) => {
                            error!(%e, "短信存储失败");
                        }
                    }
                }
            }
        }
    }
}

use chrono::Utc;
/// 活跃通话追踪
use std::collections::HashMap;
use std::sync::Mutex as StdMutex;

/// 通话追踪信息
struct ActiveCall {
    db_id: i64,
    phone_number: String,
    direction: String,
    start_time: chrono::DateTime<Utc>,
    answered: bool,
}

lazy_static::lazy_static! {
    static ref ACTIVE_CALLS: StdMutex<HashMap<String, ActiveCall>> = StdMutex::new(HashMap::new());
}

/// Start call status listener with call history recording and webhook support
pub async fn start_call_listener(
    conn: Connection,
    db: Arc<Database>,
    webhook: Arc<WebhookSender>,
) -> zbus::Result<()> {
    // Subscribe to D-Bus signals via proxy
    let dbus_proxy = Proxy::new(
        &conn,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .await?;

    // Add signal match rules - listen to VoiceCallManager signals
    let rule1 = "type='signal',sender='org.ofono',interface='org.ofono.VoiceCallManager'";
    dbus_proxy.call::<_, _, ()>("AddMatch", &(rule1,)).await?;

    // Also listen to VoiceCall property changes
    let rule2 = "type='signal',sender='org.ofono',interface='org.ofono.VoiceCall'";
    dbus_proxy.call::<_, _, ()>("AddMatch", &(rule2,)).await?;

    let mut stream = MessageStream::from(&conn);

    let mut sweep_ticker = tokio::time::interval(std::time::Duration::from_secs(300));
    loop {
        // 周期清扫 ACTIVE_CALLS（防漏 CallRemoved 导致内存泄漏）与流消息并行
        let msg = tokio::select! {
            biased;
            _ = sweep_ticker.tick() => {
                sweep_stale_active_calls();
                continue;
            }
            m = stream.next() => match m {
                Some(Ok(msg)) => msg,
                Some(Err(_)) => {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    continue;
                }
                None => return Ok(()),
            },
        };

        // Process call-related signals
        if let Some(member) = msg.header().member() {
            let member_str = member.as_str();

            match member_str {
                "CallAdded" => {
                    // Parse CallAdded signal: (object_path, properties)
                    if let Ok((path, props)) = msg.body().deserialize::<(
                        zbus::zvariant::ObjectPath,
                        std::collections::HashMap<String, OwnedValue>,
                    )>() {
                        let path_str = path.to_string();

                        // Extract phone number from LineIdentification property
                        let phone_number = props
                            .get("LineIdentification")
                            .and_then(|v| v.downcast_ref::<zbus::zvariant::Str>().ok())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "Unknown".to_string());

                        // Extract call state
                        let state = props
                            .get("State")
                            .and_then(|v| v.downcast_ref::<zbus::zvariant::Str>().ok())
                            .map(|s| s.to_string())
                            .unwrap_or_default();

                        // Determine direction based on state
                        let direction = if state == "incoming" || state == "alerting" {
                            "incoming"
                        } else {
                            "outgoing"
                        };

                        info!(caller = %phone_number, state = %state, "收到来电");

                        // Insert call record into database
                        let answered = state == "active";
                        if let Ok(db_id) = db.insert_call(direction, &phone_number, answered) {
                            let mut active_calls =
                                ACTIVE_CALLS.lock().unwrap_or_else(|e| e.into_inner());
                            active_calls.insert(
                                path_str,
                                ActiveCall {
                                    db_id,
                                    phone_number,
                                    direction: direction.to_string(),
                                    start_time: Utc::now(),
                                    answered,
                                },
                            );
                        }
                    }
                }
                "CallRemoved" => {
                    // Parse CallRemoved signal: object_path
                    if let Ok(path) = msg.body().deserialize::<zbus::zvariant::ObjectPath>() {
                        let path_str = path.to_string();

                        let mut active_calls =
                            ACTIVE_CALLS.lock().unwrap_or_else(|e| e.into_inner());
                        if let Some(call) = active_calls.remove(&path_str) {
                            info!("通话结束");
                            // Calculate duration
                            let duration = (Utc::now() - call.start_time).num_seconds();
                            let end_time = Utc::now().to_rfc3339();

                            // Determine final direction
                            let final_direction = if !call.answered && call.direction == "incoming"
                            {
                                // Missed call
                                let _ = db.mark_call_missed(call.db_id);
                                "missed".to_string()
                            } else {
                                let _ = db.update_call_end(call.db_id, duration, call.answered);
                                call.direction.clone()
                            };

                            // Forward to webhook
                            let call_record = CallRecord {
                                id: call.db_id,
                                direction: final_direction,
                                phone_number: call.phone_number,
                                duration,
                                start_time: call.start_time.to_rfc3339(),
                                end_time: Some(end_time),
                                answered: call.answered,
                            };
                            let webhook_clone = Arc::clone(&webhook);
                            tokio::spawn(async move {
                                let _ = webhook_clone.forward_call(&call_record).await;
                            });
                        }
                    }
                }
                "PropertyChanged" => {
                    // Handle VoiceCall property changes (e.g., state changes to "active")
                    if let Ok((name, value)) = msg.body().deserialize::<(String, OwnedValue)>() {
                        if name == "State" {
                            if let Some(state) = value.downcast_ref::<zbus::zvariant::Str>().ok() {
                                let state_str = state.to_string();

                                // Get call path from message
                                if let Some(path) = msg.header().path() {
                                    let path_str = path.to_string();

                                    // Update answered status if call becomes active
                                    if state_str == "active" {
                                        let mut active_calls =
                                            ACTIVE_CALLS.lock().unwrap_or_else(|e| e.into_inner());
                                        if let Some(call) = active_calls.get_mut(&path_str) {
                                            call.answered = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// 清扫 ACTIVE_CALLS 中超过 4 小时的陈旧条目（防漏 CallRemoved 的内存泄漏）
fn sweep_stale_active_calls() {
    let mut active = ACTIVE_CALLS.lock().unwrap_or_else(|e| e.into_inner());
    let now = Utc::now();
    let max_age = chrono::Duration::hours(4);
    active.retain(|_, c| (now - c.start_time) < max_age);
}

/// SMS 监听器：连接 + 监听，失败自动重连（指数退避，封顶 30s）
pub async fn run_sms_listener_with_reconnect(
    db: Arc<Database>,
    webhook: Arc<WebhookSender>,
    sms_push: Arc<SmsPushSender>,
) {
    let initial_delay = std::time::Duration::from_millis(500);
    let max_delay = std::time::Duration::from_secs(30);
    let mut connect_delay = initial_delay;
    let mut restart_delay = initial_delay;
    loop {
        let conn = match Connection::system().await {
            Ok(c) => {
                connect_delay = initial_delay;
                info!("SMS D-Bus 监听已启动");
                c
            }
            Err(e) => {
                warn!(error = %e, ?connect_delay, "SMS listener: D-Bus 不可用，重试");
                tokio::time::sleep(connect_delay).await;
                connect_delay = (connect_delay * 2).min(max_delay);
                continue;
            }
        };
        let started_at = std::time::Instant::now();
        match start_sms_listener(
            conn,
            Arc::clone(&db),
            Arc::clone(&webhook),
            Arc::clone(&sms_push),
        )
        .await
        {
            Ok(()) => warn!("SMS listener 流结束，重连"),
            Err(e) => warn!(error = %e, "SMS listener 错误，重连"),
        }
        if started_at.elapsed() >= std::time::Duration::from_secs(60) {
            restart_delay = initial_delay;
        }
        tokio::time::sleep(restart_delay).await;
        restart_delay = (restart_delay * 2).min(max_delay);
    }
}

/// 通话监听器：连接 + 监听，失败自动重连
pub async fn run_call_listener_with_reconnect(db: Arc<Database>, webhook: Arc<WebhookSender>) {
    let initial_delay = std::time::Duration::from_millis(500);
    let max_delay = std::time::Duration::from_secs(30);
    let mut connect_delay = initial_delay;
    let mut restart_delay = initial_delay;
    loop {
        let conn = match Connection::system().await {
            Ok(c) => {
                connect_delay = initial_delay;
                info!("通话 D-Bus 监听已启动");
                c
            }
            Err(e) => {
                warn!(error = %e, ?connect_delay, "Call listener: D-Bus 不可用，重试");
                tokio::time::sleep(connect_delay).await;
                connect_delay = (connect_delay * 2).min(max_delay);
                continue;
            }
        };
        let started_at = std::time::Instant::now();
        match start_call_listener(conn, Arc::clone(&db), Arc::clone(&webhook)).await {
            Ok(()) => warn!("Call listener 流结束，重连"),
            Err(e) => warn!(error = %e, "Call listener 错误，重连"),
        }
        if started_at.elapsed() >= std::time::Duration::from_secs(60) {
            restart_delay = initial_delay;
        }
        tokio::time::sleep(restart_delay).await;
        restart_delay = (restart_delay * 2).min(max_delay);
    }
}
