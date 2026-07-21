/*
 * @Author: 1orz cloudorzi@gmail.com
 * @Description: 日志导出/上报模块（方向A 远程上报 + 方向B 现场查看/导出）
 *
 * 设计要点（设备内存 ≤ 2MB 硬约束）：
 * - 日志**完全不落盘**，仅在内存维护一个**严格有界**环形缓冲。
 * - 缓冲受 `buffer_capacity`（条数）与 `BYTE_CAP`（1 MiB 字节硬顶）双重夹逼。
 * - 远程 shipper 拥有独立有界 outbox（≤ 500 条），离线时丢旧 + 计数，永不无限堆积。
 * - broadcast / SSE 观看者仅持有 Arc 指针（共享同一份 LogEntry 分配），占用可忽略。
 * - 写入路径（tracing Layer::on_event）为同步非阻塞：先做 O(1) 级别判断，再短暂加锁。
 */

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Notify};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

use crate::config::{ConfigManager, LogExportConfig};

/// 环形缓冲的字节硬顶（条数之外的兜底，保证内存预算）。1 MiB。
const BYTE_CAP: usize = 1_048_576;
/// 远程 shipper 的 outbox 容量（离线积压上限，超出丢旧）。
const OUTBOX_CAP: usize = 500;
/// broadcast 容量（SSE 实时推流；慢消费者 lag 而不阻塞生产端）。
const BROADCAST_CAP: usize = 256;
/// 每个 LogEntry 的固定开销估算（struct + Arc + 字符串 heap 元数据）。
const ENTRY_OVERHEAD: usize = 96;

/// 级别字符串 → u8（error=1 … trace=5）；无法识别返回 None。
fn level_str_u8(s: &str) -> Option<u8> {
    match s.to_ascii_lowercase().as_str() {
        "error" => Some(1),
        "warn" => Some(2),
        "info" => Some(3),
        "debug" => Some(4),
        "trace" => Some(5),
        _ => None,
    }
}

/// `tracing::Level` → u8（同上序）。
fn level_u8(l: &Level) -> u8 {
    match *l {
        Level::ERROR => 1,
        Level::WARN => 2,
        Level::INFO => 3,
        Level::DEBUG => 4,
        Level::TRACE => 5,
    }
}

/// 单条日志记录（存储 / 广播 / 远程上报 / 导出共用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// ISO8601 UTC 时间戳
    pub ts: String,
    /// 级别（ERROR/WARN/INFO/DEBUG/TRACE）
    pub level: String,
    /// 目标（模块路径）
    pub target: String,
    /// 渲染后的消息文本
    pub message: String,
    /// 结构化字段（"k=v k2=v2"）
    pub fields: String,
}

impl LogEntry {
    /// 近似字节大小（用于字节硬顶核算）。
    pub fn approx_bytes(&self) -> usize {
        self.ts.len()
            + self.level.len()
            + self.target.len()
            + self.message.len()
            + self.fields.len()
            + ENTRY_OVERHEAD
    }

    /// 文本视图单行：`ts LEVEL target: message  fields`
    pub fn to_text_line(&self) -> String {
        if self.fields.is_empty() {
            format!("{} {} {}: {}", self.ts, self.level, self.target, self.message)
        } else {
            format!(
                "{} {} {}: {}  {}",
                self.ts, self.level, self.target, self.message, self.fields
            )
        }
    }
}

/// 采集 tracing 事件字段用的访问器。
#[derive(Default)]
struct EventVisitor {
    message: String,
    fields: String,
}

impl Visit for EventVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            // message 字段的 Debug 实现直接委托 Display，无引号
            self.message = format!("{:?}", value);
        } else {
            if !self.fields.is_empty() {
                self.fields.push(' ');
            }
            self.fields.push_str(field.name());
            self.fields.push('=');
            self.fields.push_str(&format!("{:?}", value));
        }
    }
}

/// 有界环形缓冲 + 实时广播 + 远程 outbox。
///
/// 注：release 配置为 `panic = "abort"`，`catch_unwind` 在此无效。
/// 因此所有 mutex 取锁均用 `unwrap_or_else(|e| e.into_inner())`——
/// 即便锁被毒化也恢复出 guard，绝不 panic，避免"一条日志搞崩整个进程"。
pub struct LogBuffer {
    /// 历史/导出用环形缓冲（条数 + 字节双上限）
    entries: Mutex<VecDeque<Arc<LogEntry>>>,
    cap_entries: AtomicUsize,
    current_bytes: AtomicUsize,

    /// 实时推流广播（SSE）
    tx: broadcast::Sender<Arc<LogEntry>>,

    /// 远程 shipper 的待发队列（独立有界，离线积压）
    outbox: Mutex<VecDeque<Arc<LogEntry>>>,
    /// 是否正向上报（push 时据此决定是否入 outbox，避免无谓 Arc clone）
    shipping_enabled: AtomicBool,

    /// 实时采集级别（viewer / remote 中较宽者；事件级别 > 此值则整条丢弃）
    capture_level: AtomicU8,

    /// 唤醒 shipper（有新日志入 outbox 时）
    notify: Notify,

    /// 统计：缓冲溢出丢弃数
    dropped_overflow: AtomicU64,
    /// 统计：远程上报积压丢弃数
    dropped_remote: AtomicU64,
}

impl LogBuffer {
    pub fn new(cap_entries: usize) -> Arc<Self> {
        let (tx, _rx) = broadcast::channel(BROADCAST_CAP);
        Arc::new(Self {
            entries: Mutex::new(VecDeque::with_capacity(cap_entries.min(2048))),
            cap_entries: AtomicUsize::new(cap_entries),
            current_bytes: AtomicUsize::new(0),
            tx,
            outbox: Mutex::new(VecDeque::with_capacity(OUTBOX_CAP)),
            shipping_enabled: AtomicBool::new(false),
            capture_level: AtomicU8::new(3), // 默认 info
            notify: Notify::new(),
            dropped_overflow: AtomicU64::new(0),
            dropped_remote: AtomicU64::new(0),
        })
    }

    /// 根据配置同步：采集级别、缓冲条数、是否上报。
    pub fn update_config(&self, cfg: &LogExportConfig) {
        // 采集级别 = viewer / remote 中较宽者（数字越大越宽）
        let mut level = 0u8; // 默认不采集
        if cfg.viewer_enabled {
            if let Some(l) = level_str_u8(&cfg.viewer_level) {
                level = level.max(l);
            }
        }
        if cfg.remote_enabled {
            if let Some(l) = level_str_u8(&cfg.remote_level) {
                level = level.max(l);
            }
        }
        self.capture_level.store(level, Ordering::Relaxed);
        self.cap_entries.store(cfg.buffer_capacity, Ordering::Relaxed);
        self.shipping_enabled.store(
            cfg.remote_enabled && !cfg.remote_url.is_empty(),
            Ordering::Relaxed,
        );
    }

    /// 推入一条日志（由 tracing Layer 同步调用，必须非阻塞）。
    /// 返回 false 表示因级别过滤被丢弃（供 Layer 早退用）。
    pub fn push(&self, entry: LogEntry) -> bool {
        let level_u8 = match level_str_u8(&entry.level) {
            Some(v) => v,
            None => return false,
        };
        if level_u8 > self.capture_level.load(Ordering::Relaxed) {
            return false;
        }

        let arc = Arc::new(entry);
        let bytes = arc.approx_bytes();

        // 1) 环形缓冲（双上限）
        {
            let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            entries.push_back(Arc::clone(&arc));
            let mut cur = self.current_bytes.load(Ordering::Relaxed);
            cur += bytes;
            let cap_n = self.cap_entries.load(Ordering::Relaxed);
            while entries.len() > cap_n || cur > BYTE_CAP {
                if let Some(old) = entries.pop_front() {
                    cur = cur.saturating_sub(old.approx_bytes());
                    self.dropped_overflow.fetch_add(1, Ordering::Relaxed);
                } else {
                    break;
                }
            }
            self.current_bytes.store(cur, Ordering::Relaxed);
        }

        // 2) 实时广播（lag 容忍，不阻塞）
        let _ = self.tx.send(Arc::clone(&arc));

        // 3) 远程 outbox（仅在启用上报时入队）
        if self.shipping_enabled.load(Ordering::Relaxed) {
            let mut outbox = self.outbox.lock().unwrap_or_else(|e| e.into_inner());
            outbox.push_back(Arc::clone(&arc));
            while outbox.len() > OUTBOX_CAP {
                if outbox.pop_front().is_some() {
                    self.dropped_remote.fetch_add(1, Ordering::Relaxed);
                } else {
                    break;
                }
            }
            drop(outbox);
            self.notify.notify_one();
        }

        true
    }

    /// 订阅实时推流（SSE 用）。
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<LogEntry>> {
        self.tx.subscribe()
    }

    /// 快照当前环形缓冲（导出用；按时间正序，最旧在前）。
    pub fn snapshot(&self) -> Vec<Arc<LogEntry>> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect()
    }

    /// 清空环形缓冲与统计。
    pub fn clear(&self) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.clear();
        self.current_bytes.store(0, Ordering::Relaxed);
    }

    /// 从 outbox 头部取最多 n 条（shipper 用）。
    fn drain_outbox(&self, n: usize) -> Vec<Arc<LogEntry>> {
        let mut outbox = self.outbox.lock().unwrap_or_else(|e| e.into_inner());
        let take = n.min(outbox.len());
        outbox.drain(..take).collect()
    }

    /// 把发送失败的批次重新塞回 outbox 头部（保持顺序），溢出则丢最新。
    fn pushback_outbox(&self, batch: Vec<Arc<LogEntry>>) {
        let mut outbox = self.outbox.lock().unwrap_or_else(|e| e.into_inner());
        for entry in batch.into_iter().rev() {
            outbox.push_front(entry);
        }
        while outbox.len() > OUTBOX_CAP {
            if outbox.pop_back().is_some() {
                self.dropped_remote.fetch_add(1, Ordering::Relaxed);
            } else {
                break;
            }
        }
    }

    /// 清空 outbox（关闭上报时丢弃积压）。
    fn discard_outbox(&self) {
        let mut outbox = self.outbox.lock().unwrap_or_else(|e| e.into_inner());
        let dropped = outbox.len();
        outbox.clear();
        if dropped > 0 {
            self.dropped_remote
                .fetch_add(dropped as u64, Ordering::Relaxed);
        }
    }

    /// 丢弃计数快照（供 API/调试展示）。
    pub fn dropped_stats(&self) -> (u64, u64) {
        (
            self.dropped_overflow.load(Ordering::Relaxed),
            self.dropped_remote.load(Ordering::Relaxed),
        )
    }
}

/// 自定义 tracing Layer：把事件采集进 LogBuffer。
pub struct LogBufferLayer {
    buffer: Arc<LogBuffer>,
}

impl LogBufferLayer {
    pub fn new(buffer: Arc<LogBuffer>) -> Self {
        Self { buffer }
    }
}

impl<S> Layer<S> for LogBufferLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        // O(1) 级别早退（避免无谓格式化）
        let level = event.metadata().level();
        if level_u8(level) > self.buffer.capture_level.load(Ordering::Relaxed) {
            return;
        }

        let mut visitor = EventVisitor::default();
        event.record(&mut visitor);

        let entry = LogEntry {
            ts: Utc::now().to_rfc3339(),
            level: level.as_str().to_string(),
            target: event.metadata().target().to_string(),
            message: visitor.message,
            fields: visitor.fields,
        };
        self.buffer.push(entry);
    }
}

/// 远程上报 shipper：按 batch/flush 异步 POST 到外部端点。
pub struct LogShipper;

impl LogShipper {
    /// 启动后台 shipper 任务。返回 JoinHandle（主要用于不被取消地 detached 运行）。
    pub fn spawn(config_manager: Arc<ConfigManager>, buffer: Arc<LogBuffer>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let client = match Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
            {
                Ok(c) => c,
                Err(_) => return,
            };

            let mut consec_fail = 0u32;
            loop {
                let cfg = config_manager.get_log_export();
                let enabled = cfg.remote_enabled && !cfg.remote_url.is_empty();
                let flush = Duration::from_millis(cfg.flush_interval_ms.max(500));

                if !enabled {
                    buffer.discard_outbox();
                    // 关闭时退化为只等 flush，避免空转
                    tokio::time::sleep(flush).await;
                    continue;
                }

                // 等待：有新日志 OR flush 超时
                tokio::select! {
                    _ = buffer.notify.notified() => {}
                    _ = tokio::time::sleep(flush) => {}
                }

                let batch = buffer.drain_outbox(cfg.batch_size);
                if batch.is_empty() {
                    continue;
                }

                match ship_batch(&client, &cfg, &batch).await {
                    Ok(()) => {
                        consec_fail = 0;
                    }
                    Err(_) => {
                        consec_fail = consec_fail.saturating_add(1);
                        buffer.pushback_outbox(batch);
                        // 指数退避：0.5s,1s,2s,4s,8s,... 上限 30s
                        let backoff = Duration::from_millis(
                            (500u64).saturating_mul(1u64 << consec_fail.min(7).min(20))
                        ).min(Duration::from_secs(30));
                        tokio::time::sleep(backoff).await;
                    }
                }
            }
        })
    }
}

/// 发送一个批次（按 remote_level 过滤后 POST JSON 数组）。
/// 失败统一返回 `Box<dyn Error>`，调用方据此重试。
async fn ship_batch(
    client: &Client,
    cfg: &LogExportConfig,
    batch: &[Arc<LogEntry>],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let filter = level_str_u8(&cfg.remote_level).unwrap_or(3);
    let filtered: Vec<&LogEntry> = batch
        .iter()
        .filter(|e| level_str_u8(&e.level).unwrap_or(0) <= filter)
        .map(|e| e.as_ref())
        .collect();
    if filtered.is_empty() {
        return Ok(());
    }

    let body = serde_json::to_vec(&filtered)
        .map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(format!("serialize: {e}")))?;

    let mut req = client
        .post(cfg.remote_url.as_str())
        .header("Content-Type", "application/json")
        .body(body);
    if !cfg.remote_token.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", cfg.remote_token));
    }
    let resp = req.send().await?;
    resp.error_for_status()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LogExportConfig;

    fn entry(level: &str, msg: &str) -> LogEntry {
        LogEntry {
            ts: "2026-01-01T00:00:00+00:00".to_string(),
            level: level.to_string(),
            target: "test".to_string(),
            message: msg.to_string(),
            fields: String::new(),
        }
    }

    #[test]
    fn buffer_respects_entry_cap() {
        let buf = LogBuffer::new(5);
        buf.update_config(&LogExportConfig {
            viewer_enabled: true,
            viewer_level: "trace".into(),
            buffer_capacity: 5,
            ..Default::default()
        });
        for i in 0..20 {
            buf.push(entry("INFO", &format!("msg {i}")));
        }
        let snap = buf.snapshot();
        assert_eq!(snap.len(), 5, "应被条数上限夹到 5 条");
        assert_eq!(snap.last().unwrap().message, "msg 19", "应保留最新");
        let (overflow, _) = buf.dropped_stats();
        assert_eq!(overflow, 15, "应丢弃 15 条最旧");
    }

    #[test]
    fn buffer_respects_byte_cap() {
        let buf = LogBuffer::new(10000);
        buf.update_config(&LogExportConfig {
            viewer_enabled: true,
            viewer_level: "trace".into(),
            buffer_capacity: 10000,
            ..Default::default()
        });
        // 每条 ~200B，注入足够多以触达 1MiB 字节顶
        let big = "x".repeat(200);
        for _ in 0..20000 {
            buf.push(entry("INFO", &big));
        }
        let snap = buf.snapshot();
        let total: usize = snap.iter().map(|e| e.approx_bytes()).sum();
        assert!(total <= BYTE_CAP + 256, "字节总量不应超过硬顶 {}，实际 {}", BYTE_CAP, total);
    }

    #[test]
    fn level_filter_drops_below_capture_level() {
        let buf = LogBuffer::new(100);
        // 仅采集 warn 及以上
        buf.update_config(&LogExportConfig {
            viewer_enabled: true,
            viewer_level: "warn".into(),
            buffer_capacity: 100,
            ..Default::default()
        });
        buf.push(entry("DEBUG", "should drop"));
        buf.push(entry("INFO", "should drop"));
        buf.push(entry("WARN", "keep"));
        buf.push(entry("ERROR", "keep"));
        let snap = buf.snapshot();
        assert_eq!(snap.len(), 2, "应只保留 WARN/ERROR");
        assert!(snap.iter().all(|e| matches!(e.level.as_str(), "WARN" | "ERROR")));
    }

    #[test]
    fn dynamic_capacity_change_takes_effect() {
        let buf = LogBuffer::new(100);
        buf.update_config(&LogExportConfig {
            viewer_enabled: true,
            viewer_level: "trace".into(),
            buffer_capacity: 100,
            ..Default::default()
        });
        for i in 0..100 {
            buf.push(entry("INFO", &format!("a{i}")));
        }
        // 运行时收紧到 10
        buf.update_config(&LogExportConfig {
            viewer_enabled: true,
            viewer_level: "trace".into(),
            buffer_capacity: 10,
            ..Default::default()
        });
        // 再推一条触发收缩
        buf.push(entry("INFO", "trigger"));
        let snap = buf.snapshot();
        assert!(snap.len() <= 11, "收紧后条数应 ≤ 11，实际 {}", snap.len());
        assert_eq!(snap.last().unwrap().message, "trigger");
    }

    #[test]
    fn outbox_pushback_keeps_bounded() {
        let buf = LogBuffer::new(100);
        buf.update_config(&LogExportConfig {
            remote_enabled: true,
            remote_url: "http://example".into(),
            remote_level: "trace".into(),
            viewer_enabled: false,
            buffer_capacity: 100,
            ..Default::default()
        });
        let mut batch: Vec<Arc<LogEntry>> = (0..1000)
            .map(|i| Arc::new(entry("INFO", &format!("b{i}"))))
            .collect();
        // 直接塞回（模拟发送失败重入队）
        let half = batch.split_off(500);
        buf.pushback_outbox(batch);
        buf.pushback_outbox(half);
        // outbox 应被夹到 OUTBOX_CAP
        let drained = buf.drain_outbox(usize::MAX);
        assert!(drained.len() <= OUTBOX_CAP, "outbox 应 ≤ {}", OUTBOX_CAP);
    }

    #[test]
    fn approx_bytes_positive() {
        let e = entry("INFO", "hello world");
        assert!(e.approx_bytes() > ENTRY_OVERHEAD);
    }
}
