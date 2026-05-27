//! Fayger `BuF` 制品格式、Parser 与 Serializer。
//!
//! 这个 crate 实现 BuF 制品的字节布局、解析与序列化。M0 阶段交付：
//!
//! - [`mod@format`] 模块：`Header` 与 `Trailer` 的固定字节布局（任务 3 完成）
//! - [`mod@canonical`] 模块：canonical CBOR 编解码层（任务 4 完成）
//! - `manifest` 模块：`BuFManifest` 与子类型（任务 6-7）
//! - `object` 模块：`BuFObject` 与一致性自检（任务 8）
//! - `parser`、`serializer` 模块：顶层 API（任务 9-10）
//!
//! 详见 `release/dev/core/MILESTONES/M0-buf-core/design.md` 中 `fayger-buf` crate 章节。

#![doc(html_root_url = "https://docs.rs/fayger-buf/0.0.0")]

pub mod canonical;
mod error;
pub mod format;
