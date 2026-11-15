//! 宿主日志 / 追踪通道转发原语（Wave 2 · `add-host-log-channel-integration`）。
//!
//! 本模块提供与具体平台解耦的结构化宿主日志承载记录 [`HostLogRecord`]、非阻塞写出契约
//! [`HostLogSink`] 以及有界缓冲与故障隔离转发器 [`BoundedHostLogForwarder`]。
//!
//! 分层原则：
//! - [`crate::EventBus`] 默认不挂载任何宿主通道，维持「无显式订阅者且未挂载宿主日志通道时零外部副作用」；
//! - 具体平台通道名称与等级标签映射由适配层（`fayger-adapter`）的适配器描述符自描述并注入；
//! - 通道写入失败或队列溢出均被隔离在转发器内部计数，绝不 panic、不阻塞、不改变运行实例状态，
//!   也不影响其他订阅者接收事件。

use crate::bus::{EventBus, SubscriptionId};
use crate::event::{EventCategory, EventLevel, EventPayload, FaygerEvent, PhaseStatus};
use crate::filter::EventFilter;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::Write;
use std::rc::Rc;

/// 结构化宿主日志 JSON Lines 记录的 schema 标识。
pub const HOST_LOG_SCHEMA_VERSION: &str = "fayger-host-log/1.0";

/// 等级映射函数签名：把 [`EventLevel`] 映射为目标宿主通道的原生等级字符串。
pub type SeverityMapper = fn(EventLevel) -> &'static str;

/// 单条可序列化为 UTF-8 JSON Lines 的结构化宿主日志记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostLogRecord {
    /// 记录格式版本（固定为 [`HOST_LOG_SCHEMA_VERSION`]）。
    pub schema_version: String,
    /// 产生该映射的适配器平台标识（如 `"native-desktop"`）。
    pub platform_id: String,
    /// 宿主通道类别标识（如 `"os-logger"` / `"systemd-journal"`）。
    pub channel_kind: String,
    /// 映射后的宿主原生等级标签。
    pub host_severity: String,
    /// 事件总线分配的单调逻辑时钟时间戳。
    pub timestamp: u64,
    /// Fayger 事件类别稳定标识。
    pub category: String,
    /// Fayger 事件等级稳定标识。
    pub level: String,
    /// 关联运行实例标识（可选）。
    pub instance_id: Option<u64>,
    /// 结构化事件负载。
    pub payload: Value,
}

impl HostLogRecord {
    /// 从一条 [`FaygerEvent`] 与适配层提供的通道元数据构造结构化记录。
    #[must_use]
    pub fn from_event(
        event: &FaygerEvent,
        platform_id: &str,
        channel_kind: &str,
        host_severity: &str,
    ) -> Self {
        Self {
            schema_version: HOST_LOG_SCHEMA_VERSION.to_owned(),
            platform_id: platform_id.to_owned(),
            channel_kind: channel_kind.to_owned(),
            host_severity: host_severity.to_owned(),
            timestamp: event.timestamp,
            category: category_str(event.category).to_owned(),
            level: level_str(event.level).to_owned(),
            instance_id: event.instance_id,
            payload: payload_to_value(&event.payload),
        }
    }

    /// 序列化为单行确定性 JSON 文本（不含尾随换行）。
    #[must_use]
    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            format!(
                "{{\"schema_version\":\"{HOST_LOG_SCHEMA_VERSION}\",\"timestamp\":{}}}",
                self.timestamp
            )
        })
    }
}

const fn category_str(category: EventCategory) -> &'static str {
    match category {
        EventCategory::Lifecycle => "lifecycle",
        EventCategory::Quota => "quota",
        EventCategory::Loader => "loader",
        EventCategory::Adapter => "adapter",
        EventCategory::Debug => "debug",
    }
}

const fn level_str(level: EventLevel) -> &'static str {
    match level {
        EventLevel::Debug => "debug",
        EventLevel::Info => "info",
        EventLevel::Warn => "warn",
        EventLevel::Error => "error",
    }
}

const fn phase_status_str(status: PhaseStatus) -> &'static str {
    match status {
        PhaseStatus::Enter => "enter",
        PhaseStatus::Complete => "complete",
        PhaseStatus::Fail => "fail",
    }
}

fn payload_to_value(payload: &EventPayload) -> Value {
    match payload {
        EventPayload::Lifecycle { from, to } => json!({
            "kind": "lifecycle",
            "from": from,
            "to": to,
        }),
        EventPayload::Quota {
            cpu_ms_per_sec,
            memory_bytes,
            io_bytes_per_sec,
            suspended,
            exceeded,
        } => json!({
            "kind": "quota",
            "cpu_ms_per_sec": cpu_ms_per_sec,
            "memory_bytes": memory_bytes,
            "io_bytes_per_sec": io_bytes_per_sec,
            "suspended": suspended,
            "exceeded": exceeded,
        }),
        EventPayload::Loader { phase, status } => json!({
            "kind": "loader",
            "phase": phase,
            "status": phase_status_str(*status),
        }),
        EventPayload::Adapter { detail } => json!({
            "kind": "adapter",
            "detail": detail,
        }),
        EventPayload::Debug { detail } => json!({
            "kind": "debug",
            "detail": detail,
        }),
    }
}

/// 宿主日志通道非阻塞写出契约。
///
/// 实现者不得 panic；当底层通道不可用或写入失败时应返回 `Err(reason)`，由转发器隔离并计数。
pub trait HostLogSink {
    /// 写出一条结构化宿主日志记录及其单行 JSON 表示。
    fn write_record(&mut self, record: &HostLogRecord, json_line: &str) -> Result<(), String>;
}

/// 内存宿主日志收集器（用于审计采集、嵌入式宿主测试与故障注入验证）。
#[derive(Debug, Default)]
pub struct MemoryHostLogSink {
    records: Vec<HostLogRecord>,
    lines: Vec<String>,
    fail_writes: bool,
}

impl MemoryHostLogSink {
    /// 构造空的内存日志收集器。
    #[must_use]
    pub const fn new() -> Self {
        Self {
            records: Vec::new(),
            lines: Vec::new(),
            fail_writes: false,
        }
    }

    /// 切换故障注入开关：开启后每次 `write_record` 返回 `Err`。
    pub const fn set_fail_writes(&mut self, fail: bool) {
        self.fail_writes = fail;
    }

    /// 已成功写出的结构化记录切片。
    #[must_use]
    pub fn records(&self) -> &[HostLogRecord] {
        &self.records
    }

    /// 已成功写出的 JSON Lines 切片。
    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }
}

impl HostLogSink for MemoryHostLogSink {
    fn write_record(&mut self, record: &HostLogRecord, json_line: &str) -> Result<(), String> {
        if self.fail_writes {
            return Err("injected host log channel write failure".to_owned());
        }
        self.records.push(record.clone());
        self.lines.push(json_line.to_owned());
        Ok(())
    }
}

/// 基于任意 [`Write`] 目标的 JSON Lines 宿主日志写出器。
pub struct WriterHostLogSink<W: Write> {
    writer: W,
}

impl<W: Write> WriterHostLogSink<W> {
    /// 包装一个 [`Write`] 目标。
    #[must_use]
    pub const fn new(writer: W) -> Self {
        Self { writer }
    }

    /// 取出底层写入器。
    #[must_use]
    pub fn into_inner(self) -> W {
        self.writer
    }
}

impl<W: Write> HostLogSink for WriterHostLogSink<W> {
    fn write_record(&mut self, _record: &HostLogRecord, json_line: &str) -> Result<(), String> {
        writeln!(self.writer, "{json_line}").map_err(|e| e.to_string())
    }
}

/// 宿主日志转发器运行统计（含溢出丢弃与写出失败计数）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostLogStats {
    /// 成功写出到底层 `HostLogSink` 的记录数。
    pub emitted_count: u64,
    /// 因有界缓冲队列已满而被溢出丢弃的记录数。
    pub dropped_overflow_count: u64,
    /// 底层 `HostLogSink::write_record` 返回失败并被隔离的次数。
    pub write_failure_count: u64,
    /// 当前尚在有界队列中等待 `flush` 的记录数。
    pub buffered_count: usize,
}

struct ForwarderInner {
    platform_id: String,
    channel_kind: String,
    severity_mapper: SeverityMapper,
    max_buffered_records: usize,
    auto_flush: bool,
    buffer: VecDeque<HostLogRecord>,
    sink: Box<dyn HostLogSink>,
    emitted_count: u64,
    dropped_overflow_count: u64,
    write_failure_count: u64,
}

impl ForwarderInner {
    fn ingest(&mut self, event: &FaygerEvent) {
        let host_severity = (self.severity_mapper)(event.level);
        let record =
            HostLogRecord::from_event(event, &self.platform_id, &self.channel_kind, host_severity);
        if self.auto_flush {
            self.write_one(&record);
            return;
        }
        if self.max_buffered_records == 0 || self.buffer.len() >= self.max_buffered_records {
            self.dropped_overflow_count += 1;
            return;
        }
        self.buffer.push_back(record);
    }

    fn flush(&mut self) -> usize {
        let mut drained = 0usize;
        while let Some(record) = self.buffer.pop_front() {
            self.write_one(&record);
            drained += 1;
        }
        drained
    }

    fn write_one(&mut self, record: &HostLogRecord) {
        let line = record.to_json_line();
        if self.sink.write_record(record, &line).is_ok() {
            self.emitted_count += 1;
        } else {
            self.write_failure_count += 1;
        }
    }

    fn stats(&self) -> HostLogStats {
        HostLogStats {
            emitted_count: self.emitted_count,
            dropped_overflow_count: self.dropped_overflow_count,
            write_failure_count: self.write_failure_count,
            buffered_count: self.buffer.len(),
        }
    }
}

/// 有界缓冲与故障隔离的宿主日志通道转发器句柄。
///
/// 通过 [`BoundedHostLogForwarder::attach`] 注册为 [`EventBus`] 的普通消费者：
/// - `auto_flush == true` 时在回调内非阻塞直写并隔离错误；
/// - `auto_flush == false` 时先写入容量为 `max_buffered_records` 的有界队列，超容丢弃并递增
///   `dropped_overflow_count`，由调用方在安全点调用 [`BoundedHostLogForwarder::flush`] 排空。
#[derive(Clone)]
pub struct BoundedHostLogForwarder {
    inner: Rc<RefCell<ForwarderInner>>,
}

impl std::fmt::Debug for BoundedHostLogForwarder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let stats = self.stats();
        f.debug_struct("BoundedHostLogForwarder")
            .field("stats", &stats)
            .finish()
    }
}

impl BoundedHostLogForwarder {
    /// 创建宿主日志转发器句柄（尚未注册到 `EventBus`）。
    #[must_use]
    pub fn new(
        platform_id: impl Into<String>,
        channel_kind: impl Into<String>,
        severity_mapper: SeverityMapper,
        max_buffered_records: usize,
        auto_flush: bool,
        sink: Box<dyn HostLogSink>,
    ) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ForwarderInner {
                platform_id: platform_id.into(),
                channel_kind: channel_kind.into(),
                severity_mapper,
                max_buffered_records,
                auto_flush,
                buffer: VecDeque::with_capacity(max_buffered_records.min(256)),
                sink,
                emitted_count: 0,
                dropped_overflow_count: 0,
                write_failure_count: 0,
            })),
        }
    }

    /// 把本转发器按给定过滤条件挂载到 [`EventBus`]，返回 [`SubscriptionId`]。
    pub fn attach(&self, bus: &mut EventBus, filter: EventFilter) -> SubscriptionId {
        let shared = Rc::clone(&self.inner);
        bus.subscribe(
            filter,
            Box::new(move |event| {
                if let Ok(mut guard) = shared.try_borrow_mut() {
                    guard.ingest(event);
                }
            }),
        )
    }

    /// 排空有界缓冲队列到底层 `HostLogSink`，返回本次尝试写出的记录数。
    pub fn flush(&self) -> usize {
        self.inner
            .try_borrow_mut()
            .map_or(0, |mut guard| guard.flush())
    }

    /// 查询当前转发器计量统计。
    #[must_use]
    pub fn stats(&self) -> HostLogStats {
        self.inner.try_borrow().map_or(
            HostLogStats {
                emitted_count: 0,
                dropped_overflow_count: 0,
                write_failure_count: 0,
                buffered_count: 0,
            },
            |guard| guard.stats(),
        )
    }
}

/// 默认通用等级映射函数。
#[must_use]
pub const fn default_severity_mapper(level: EventLevel) -> &'static str {
    level_str(level)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{
        default_severity_mapper, BoundedHostLogForwarder, HostLogSink, WriterHostLogSink,
        HOST_LOG_SCHEMA_VERSION,
    };
    use crate::bus::EventBus;
    use crate::event::{EventDraft, EventLevel, PhaseStatus};
    use crate::filter::EventFilter;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Default)]
    struct SharedBufferSink {
        lines: Rc<RefCell<Vec<String>>>,
        fail: Rc<RefCell<bool>>,
    }

    impl HostLogSink for SharedBufferSink {
        fn write_record(
            &mut self,
            _record: &super::HostLogRecord,
            json_line: &str,
        ) -> Result<(), String> {
            if *self.fail.borrow() {
                return Err("sink unavailable".to_owned());
            }
            self.lines.borrow_mut().push(json_line.to_owned());
            Ok(())
        }
    }

    #[test]
    fn auto_flush_emits_structured_jsonl_and_respects_debug_gate() {
        let mut bus = EventBus::new();
        let sink = SharedBufferSink::default();
        let forwarder = BoundedHostLogForwarder::new(
            "native-desktop",
            "os-logger",
            default_severity_mapper,
            16,
            true,
            Box::new(sink.clone()),
        );
        forwarder.attach(&mut bus, EventFilter::any());

        // Debug 默认关闭：不进入宿主通道。
        assert!(bus
            .publish(EventDraft::debug("secret-debug", Some(1)))
            .is_none());
        bus.publish(EventDraft::lifecycle("Loaded", "Initialized", 1));
        bus.publish(EventDraft::loader(
            "verify_digest",
            PhaseStatus::Complete,
            Some(1),
        ));
        bus.publish(EventDraft::quota(
            1,
            10,
            2048,
            0,
            true,
            Some("Memory".to_owned()),
        ));
        bus.publish(EventDraft::adapter(
            "denied capability",
            EventLevel::Error,
            Some(1),
        ));

        let stats = forwarder.stats();
        assert_eq!(stats.emitted_count, 4);
        assert_eq!(stats.dropped_overflow_count, 0);
        assert_eq!(stats.write_failure_count, 0);

        let lines = sink.lines.borrow();
        assert_eq!(lines.len(), 4);
        let first: serde_json::Value = serde_json::from_str(&lines[0]).expect("valid jsonl");
        assert_eq!(first["schema_version"], HOST_LOG_SCHEMA_VERSION);
        assert_eq!(first["platform_id"], "native-desktop");
        assert_eq!(first["channel_kind"], "os-logger");
        assert_eq!(first["host_severity"], "info");
        assert_eq!(first["category"], "lifecycle");
        assert_eq!(first["timestamp"], 0);
    }

    #[test]
    fn bounded_queue_drops_overflow_and_counts_accurately() {
        let mut bus = EventBus::new();
        let sink = SharedBufferSink::default();
        let forwarder = BoundedHostLogForwarder::new(
            "server",
            "systemd-journal",
            default_severity_mapper,
            2,
            false,
            Box::new(sink.clone()),
        );
        forwarder.attach(&mut bus, EventFilter::any());

        bus.publish(EventDraft::lifecycle("Loaded", "Initialized", 1));
        bus.publish(EventDraft::lifecycle("Initialized", "Running", 1));
        bus.publish(EventDraft::lifecycle("Running", "Terminated", 1));

        let before = forwarder.stats();
        assert_eq!(before.buffered_count, 2);
        assert_eq!(before.dropped_overflow_count, 1);
        assert_eq!(before.emitted_count, 0);

        let drained = forwarder.flush();
        assert_eq!(drained, 2);
        let after = forwarder.stats();
        assert_eq!(after.buffered_count, 0);
        assert_eq!(after.emitted_count, 2);
        assert_eq!(after.dropped_overflow_count, 1);
        assert_eq!(sink.lines.borrow().len(), 2);
    }

    #[test]
    fn sink_write_failure_is_isolated_from_other_subscribers() {
        let mut bus = EventBus::new();
        let sink = SharedBufferSink::default();
        *sink.fail.borrow_mut() = true;

        let forwarder = BoundedHostLogForwarder::new(
            "browser",
            "browser-console",
            default_severity_mapper,
            8,
            true,
            Box::new(sink),
        );
        forwarder.attach(&mut bus, EventFilter::any());

        let other_seen = Rc::new(RefCell::new(0usize));
        let other_clone = Rc::clone(&other_seen);
        bus.subscribe(
            EventFilter::any(),
            Box::new(move |_| *other_clone.borrow_mut() += 1),
        );

        bus.publish(EventDraft::lifecycle("Loaded", "Initialized", 9));
        assert_eq!(*other_seen.borrow(), 1);
        assert_eq!(forwarder.stats().write_failure_count, 1);
        assert_eq!(forwarder.stats().emitted_count, 0);
    }

    #[test]
    fn writer_sink_outputs_newline_delimited_json() {
        let mut bus = EventBus::new();
        let event = bus
            .publish(EventDraft::lifecycle("Loaded", "Initialized", 3))
            .expect("event");
        let record = super::HostLogRecord::from_event(&event, "inapp", "inapp-sdk", "info");
        let line = record.to_json_line();
        let mut sink = WriterHostLogSink::new(Vec::<u8>::new());
        sink.write_record(&record, &line).expect("write");
        let text = String::from_utf8(sink.into_inner()).expect("utf8");
        assert!(text.ends_with('\n'));
        assert!(text.contains("\"channel_kind\":\"inapp-sdk\""));
    }
}
