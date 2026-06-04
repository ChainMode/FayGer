//! Fayger 加载链：Verify(Digest) 与 Negotiate(Version)。
//!
//! 这个 crate 在 M0 的 [`fayger_buf`]（格式 + Parser + Serializer）之上补齐蓝图加载链
//! 中 M0 之后的两个阶段：
//!
//! - [`mod@digest`]：SHA-256 与 CRC-32 基元（M1 任务 5）。
//! - [`mod@verify`]：[`verify_digest`] —— manifest_digest / 每段 section digest / crc32
//!   的完整性校验（M1 任务 6）。
//! - [`mod@seal`]：[`seal()`] —— 对内存对象计算并填入全部摘要，产出可通过校验的制品
//!   （M1 任务 7）。
//! - [`mod@version`]：[`LoadPolicy`] 与 [`negotiate_version`] —— schema 范围 / 弃用 /
//!   runtime 最低版本协商（M1 任务 8）。
//! - [`mod@load`]：[`load()`] —— `parse → verify_digest → negotiate_version` 的加载链编排
//!   （M1 任务 9）；以及 [`load_signed()`] —— 含 Verify(Signature) 的加载链（M4）。
//!
//! M4 追加签名校验阶段与安全横切组件：
//!
//! - [`mod@sig`]：[`verify_signature`] —— Verify(Signature) 决策与 ed25519 验证基元（M4）。
//! - [`mod@truststore`]：[`TrustStore`] / [`TrustedRoot`] —— 可信根与强制签名开关（M4）。
//! - [`mod@sign`]：[`seal_and_sign`] —— 生产侧"封装+签名"，产出同时通过 digest 与签名的制品（M4）。
//!
//! M5 追加多源存储抽象与延迟加载：
//!
//! - [`mod@source`]：[`BuFSource`] / [`MemorySource`] —— 随机访问存储后端抽象（M5）。
//! - [`mod@strategy`]：[`LoadStrategy`] / [`LoadedArtifact`] / [`SectionLoader`] —— Eager/Lazy 策略与按需读取（M5）。
//! - [`mod@source_load`]：[`load_from_source`] —— 从 BuF_Source 按策略加载（M5）。
//!
//! M6 追加 profile 驱动的部分加载：
//!
//! - [`mod@select`]：[`LoadProfile`] / [`select_sections`] —— Section 选择谓词与决策（M6）。
//! - [`mod@profile_load`]：[`load_with_profile`] / [`load_with_profile_signed`] /
//!   [`PartialArtifact`] —— profile 驱动的部分加载（M6）。
//!
//! 详见各里程碑 design.md（M1 / M4 / M5 / M6）。

#![doc(html_root_url = "https://docs.rs/fayger-loader/0.0.0")]

pub mod digest;
pub mod load;
pub mod profile_load;
pub mod seal;
pub mod select;
pub mod sig;
pub mod sign;
pub mod source;
pub mod source_load;
pub mod strategy;
pub mod truststore;
pub mod verify;
pub mod version;

pub use digest::{crc32, sha256};
pub use load::{load, load_signed};
pub use profile_load::{load_with_profile, load_with_profile_signed, PartialArtifact};
pub use seal::seal;
pub use select::{select_sections, LoadProfile, SelectionResult, SkipReason, SkipRecord};
pub use sig::{ed25519_verify, verify_signature, VerifyOutcome};
pub use sign::{ed25519_public_key, ed25519_sign, seal_and_sign};
pub use source::{BuFSource, MemorySource, SourceError};
pub use source_load::load_from_source;
pub use strategy::{LoadStrategy, LoadedArtifact, SectionLoader};
pub use truststore::{TrustStore, TrustedRoot};
pub use verify::verify_digest;
pub use version::{negotiate_version, LoadPolicy};
