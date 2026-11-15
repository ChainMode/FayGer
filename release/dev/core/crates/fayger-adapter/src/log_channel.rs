//! 适配器自描述宿主日志 / 追踪通道映射规则（Wave 2 · `add-host-log-channel-integration`）。
//!
//! 每类 [`crate::PlatformAdapter`] 在其 [`crate::AdapterDescriptor`] 中通过
//! [`HostLogChannelDescriptor`] 声明自身宿主环境对应的日志通道类别与等级映射规则。
//! 运行层与加载层对通道差异保持零感知。

use fayger_observability::{
    BoundedHostLogForwarder, EventBus, EventFilter, EventLevel, HostLogSink, SeverityMapper,
    SubscriptionId,
};

/// 宿主日志 / 追踪通道类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostLogChannelKind {
    /// 宿主未提供外部日志通道（事件仅留在进程内总线）。
    None,
    /// 原生桌面统一日志设施（macOS Unified Logging / Windows Event Log / Desktop OS Logger）。
    OsLogger,
    /// 服务器守护进程结构化日志通道（如 `systemd-journald`）。
    SystemdJournal,
    /// 浏览器沙箱控制台通道（`console.debug/info/warn/error`）。
    BrowserConsole,
    /// 宿主 App 注入的 SDK 遥测日志回调通道。
    InAppSdkChannel,
}

impl HostLogChannelKind {
    /// 返回稳定机器可读通道标识。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::OsLogger => "os-logger",
            Self::SystemdJournal => "systemd-journal",
            Self::BrowserConsole => "browser-console",
            Self::InAppSdkChannel => "inapp-sdk",
        }
    }

    /// 该类别是否表示存在可挂载的外部宿主通道。
    #[must_use]
    pub const fn is_available(self) -> bool {
        !matches!(self, Self::None)
    }

    /// 返回该宿主通道对应的 [`EventLevel`] → 宿主等级字符串映射函数（蓝图 §6.6）。
    ///
    /// 严格保持语义全序，不提升或降低事件等级。
    #[must_use]
    pub const fn severity_mapper(self) -> SeverityMapper {
        match self {
            Self::None | Self::InAppSdkChannel => map_inapp_severity,
            Self::OsLogger => map_os_logger_severity,
            Self::SystemdJournal => map_systemd_severity,
            Self::BrowserConsole => map_browser_console_severity,
        }
    }
}

const fn map_os_logger_severity(level: EventLevel) -> &'static str {
    match level {
        EventLevel::Debug => "debug",
        EventLevel::Info => "info",
        EventLevel::Warn => "default",
        EventLevel::Error => "error",
    }
}

const fn map_systemd_severity(level: EventLevel) -> &'static str {
    match level {
        EventLevel::Debug => "debug",
        EventLevel::Info => "info",
        EventLevel::Warn => "warning",
        EventLevel::Error => "err",
    }
}

const fn map_browser_console_severity(level: EventLevel) -> &'static str {
    match level {
        EventLevel::Debug => "console.debug",
        EventLevel::Info => "console.info",
        EventLevel::Warn => "console.warn",
        EventLevel::Error => "console.error",
    }
}

const fn map_inapp_severity(level: EventLevel) -> &'static str {
    match level {
        EventLevel::Debug => "debug",
        EventLevel::Info => "info",
        EventLevel::Warn => "warn",
        EventLevel::Error => "error",
    }
}

/// 适配器自描述的宿主日志通道描述符。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HostLogChannelDescriptor {
    /// 宿主通道类别。
    pub kind: HostLogChannelKind,
}

impl Default for HostLogChannelDescriptor {
    fn default() -> Self {
        Self::none()
    }
}

impl HostLogChannelDescriptor {
    /// 声明无宿主通道（退化为仅保留在事件总线内）。
    #[must_use]
    pub const fn none() -> Self {
        Self {
            kind: HostLogChannelKind::None,
        }
    }

    /// 用指定宿主通道类别构造描述符。
    #[must_use]
    pub const fn new(kind: HostLogChannelKind) -> Self {
        Self { kind }
    }

    /// 把给定事件等级映射为当前宿主通道的原生等级名。
    #[must_use]
    pub fn map_level(self, level: EventLevel) -> &'static str {
        (self.kind.severity_mapper())(level)
    }

    /// 若当前描述符声明了可用宿主通道，则在 `bus` 上注册一个有界、故障隔离的转发消费者；
    /// 若通道为 [`HostLogChannelKind::None`]，不注册任何订阅并返回 `None`（保持零外部副作用）。
    pub fn attach_to_bus(
        self,
        platform_id: &str,
        bus: &mut EventBus,
        sink: Box<dyn HostLogSink>,
        max_buffered_records: usize,
        auto_flush: bool,
    ) -> Option<(SubscriptionId, BoundedHostLogForwarder)> {
        if !self.kind.is_available() {
            return None;
        }
        let forwarder = BoundedHostLogForwarder::new(
            platform_id,
            self.kind.as_str(),
            self.kind.severity_mapper(),
            max_buffered_records,
            auto_flush,
            sink,
        );
        let sub_id = forwarder.attach(bus, EventFilter::any());
        Some((sub_id, forwarder))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{HostLogChannelDescriptor, HostLogChannelKind};
    use fayger_observability::{EventBus, EventDraft, EventLevel, MemoryHostLogSink};

    #[test]
    fn none_channel_registers_zero_subscribers_and_has_zero_side_effects() {
        let mut bus = EventBus::new();
        let desc = HostLogChannelDescriptor::none();
        let attached = desc.attach_to_bus(
            "custom",
            &mut bus,
            Box::new(MemoryHostLogSink::new()),
            16,
            true,
        );
        assert!(attached.is_none());
        assert_eq!(bus.subscription_count(), 0);
        bus.publish(EventDraft::lifecycle("Loaded", "Initialized", 1));
        assert_eq!(bus.clock(), 1);
    }

    #[test]
    fn builtin_channel_kinds_map_levels_without_changing_semantics() {
        let systemd = HostLogChannelDescriptor::new(HostLogChannelKind::SystemdJournal);
        assert_eq!(systemd.map_level(EventLevel::Debug), "debug");
        assert_eq!(systemd.map_level(EventLevel::Info), "info");
        assert_eq!(systemd.map_level(EventLevel::Warn), "warning");
        assert_eq!(systemd.map_level(EventLevel::Error), "err");

        let browser = HostLogChannelDescriptor::new(HostLogChannelKind::BrowserConsole);
        assert_eq!(browser.map_level(EventLevel::Info), "console.info");
        assert_eq!(browser.map_level(EventLevel::Error), "console.error");
    }
}
