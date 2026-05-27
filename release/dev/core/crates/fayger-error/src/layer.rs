//! `Layer` 标签：标注错误源自 Fayger 的哪一层。
//!
//! 蓝图 §6.1 把所有跨层错误标注 `source_layer` 字段，并要求上层包装时不重写这个字段。
//! M0 阶段实际只产生 `Loader` 层错误，但为后续里程碑预留四个变体。

use std::fmt;

/// Fayger 三层架构 + Host 的层标签。
///
/// 标记为 `#[non_exhaustive]`，未来追加变体（例如更细粒度的子层）不破坏调用代码：
/// 调用方在 `match` 时必须写 `_ =>` 兜底。
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    /// 加载层：BuF 字节解析、版本协商、签名校验等。
    Loader,
    /// 运行层：Runtime_Interface、生命周期、资源监测等。M0 不使用。
    Runtime,
    /// 适配层：Universal_Instruction 与系统调用的双向翻译。M0 不使用。
    Adapter,
    /// 宿主层：错误最终源头是宿主侧（系统调用、SDK 等）。M0 不使用。
    Host,
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Loader => "loader",
            Self::Runtime => "runtime",
            Self::Adapter => "adapter",
            Self::Host => "host",
        };
        f.write_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::Layer;

    #[test]
    fn display_uses_lowercase_name() {
        assert_eq!(Layer::Loader.to_string(), "loader");
        assert_eq!(Layer::Runtime.to_string(), "runtime");
        assert_eq!(Layer::Adapter.to_string(), "adapter");
        assert_eq!(Layer::Host.to_string(), "host");
    }

    #[test]
    fn equality_and_copy_semantics() {
        let a = Layer::Loader;
        let b = a; // Copy
        assert_eq!(a, b);
        assert_ne!(Layer::Loader, Layer::Adapter);
    }
}
