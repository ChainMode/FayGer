//! `Layer` 标签：标注错误源自 Fayger 的哪一层。
//!
//! 蓝图 §6.1 把所有跨层错误标注 `source_layer` 字段，并要求上层包装时不重写这个字段。
//! M0 阶段实际只产生 `Loader` 层错误，但为后续里程碑预留更多变体。
//!
//! `Assembler` 变体由 H1-PROTO-02（FayManifest 装配草案）追加：FayManifest 的解析、
//! schema 校验与依赖解析发生在「装配」阶段，逻辑上先于 Loader 的字节级加载。

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
    /// 装配层：FayManifest 的解析、schema 校验与依赖解析（H1-PROTO-02 草案）。
    ///
    /// 装配在逻辑上先于 Loader 的字节级加载：开发者声明的 FayManifest 经此层解析出
    /// 组件/协议/驱动依赖与解析阶段（Resolved Stage），再交由后续加载/运行。
    Assembler,
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Loader => "loader",
            Self::Runtime => "runtime",
            Self::Adapter => "adapter",
            Self::Host => "host",
            Self::Assembler => "assembler",
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
        assert_eq!(Layer::Assembler.to_string(), "assembler");
    }

    #[test]
    fn equality_and_copy_semantics() {
        let a = Layer::Loader;
        let b = a; // Copy
        assert_eq!(a, b);
        assert_ne!(Layer::Loader, Layer::Adapter);
    }
}
