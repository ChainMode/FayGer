//! FayManifest 声明式装配（H1-PROTO-02 · 草案）。
//!
//! FayManifest 是 iFay 的「package.json」：开发者在一份 JSON 里声明所需模块、协议、控制模式、
//! 驱动与 Ego 配置，FayGer 在装配阶段据此解析依赖、补充基础设施、判定解析阶段并报告缺口。
//!
//! 本 crate 把 FayManifest 从概念推到**可解析**：
//!
//! - [`model`]：FayManifest 的 Rust 单一事实源（serde 结构 + `deny_unknown_fields` 结构校验
//!   + [`model::FayManifest::validate`] 语义校验）。
//! - [`constraint`]：版本约束（`*` / 精确 / `^` / `~` / 比较符）解析与满足判定，
//!   基于 [`fayger_buf::manifest::SemVer`]。
//! - [`namespace`]：`@scope/name` 命名空间包标识（蓝图 `@drone-drivers/...`、`@ego-models/...`）。
//! - [`registry`]：[`Registry`] 抽象 + [`StubRegistry`]（H1 占位）/ [`InMemoryRegistry`]（测试）。
//! - [`resolve()`]：装配算法（自动补充基础设施 → 判定 Phase → 解析依赖 → 报告阶段缺口）。
//! - [`schema`]：FayManifest JSON Schema 草案的生成器（与 [`model`] 同源）。
//!
//! # 权威与派生边界
//!
//! FayManifest 的**叙述权威**在伞仓库 ifay（`ifay/docs/<lang>/blueprint/13-FayManifest`），
//! **机器可读 schema 权威**在 ifay 的 openspec 能力 `faymanifest`（变更
//! `add-faymanifest-schema-authority`）+ 发布产物 `ifay/schema/faymanifest/`。
//! 本 crate 的 [`schema`] 产物是**派生/校验副本**：由 [`model`] 的 Rust 结构生成，
//! **必须**与 ifay 权威保持一致（冲突时以 ifay 为准）。注册表后端（在线索引 /
//! 命名空间治理 / 版本发布）是 **H2** 范围，本波以 [`StubRegistry`] 占位并在报告中明确标注。
//!
//! # Example
//!
//! ```
//! use fayger_faymanifest::{assemble_str, StubRegistry};
//!
//! let json = r#"{
//!   "name": "x", "version": "1.0.0", "description": "d", "vendor": "v",
//!   "modules": [{"id":"sensor"}], "protocols": [{"id":"cap_protocol"}],
//!   "controlMode": "command"
//! }"#;
//! let report = assemble_str(json, &StubRegistry).expect("assemble");
//! assert_eq!(report.resolved_phase, 2); // sensor + cap_protocol 落在 Phase 2
//! assert!(report.auto_supplemented_protocols.iter().any(|p| p == "faying_protocol"));
//! ```

#![doc(html_root_url = "https://docs.rs/fayger-faymanifest/0.0.0")]

pub mod constraint;
pub mod model;
pub mod namespace;
pub mod registry;
pub mod resolve;
pub mod schema;

pub use constraint::VersionConstraint;
pub use model::{
    ControlMode, DriverRef, EgoConfig, FayManifest, ModuleRef, PermissionsConfig, ProtocolRef,
};
pub use namespace::{
    Namespace, PackageId, SCOPE_CAP_DRIVERS, SCOPE_DRONE_DRIVERS, SCOPE_EGO_MODELS, SCOPE_IFAY,
};
pub use registry::{
    sign_registry_index_value, InMemoryRegistry, PackageGovernance, Registry,
    RegistryIndexSignature, RegistrySourceMode, RegistryTrustAnchor, RegistryTrustStore,
    ResolutionOutcome, StaticIndexRegistry, StaticPackageEntry, StaticPackageRelease,
    StaticRegistryIndex, StubRegistry, REGISTRY_INDEX_SCHEMA_VERSION,
    REGISTRY_INDEX_SIGNING_DOMAIN,
};
pub use resolve::{
    resolve, AssemblyReport, ComponentOrigin, DependencyKind, DependencyResolution,
    DependencyStatus, PhaseGap,
};
pub use schema::{draft_schema_json, draft_schema_value, schema_path, SCHEMA_RELATIVE_PATH};

use fayger_error::FaygerError;

/// 从 JSON 字符串完成完整装配：解析 → 校验 → 解析依赖。
///
/// # Errors
/// 解析失败（[`fayger_error::ErrorCode::FmParseFail`]）或语义校验失败
/// （`Fm*` 系列错误码）时返回 [`FaygerError`]。依赖的「缺失 / 不可满足 / stub」不是错误，
/// 体现在返回的 [`AssemblyReport`] 的依赖状态中。
pub fn assemble_str(json: &str, registry: &dyn Registry) -> Result<AssemblyReport, FaygerError> {
    let manifest = FayManifest::from_json_str(json)?;
    assemble(&manifest, registry)
}

/// 校验并解析一个已解析的 [`FayManifest`]。
///
/// # Errors
/// 语义校验失败时返回 [`FaygerError`]（`Fm*` 错误码）。
pub fn assemble(
    manifest: &FayManifest,
    registry: &dyn Registry,
) -> Result<AssemblyReport, FaygerError> {
    manifest.validate()?;
    Ok(resolve::resolve(manifest, registry))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{assemble_str, StubRegistry};

    #[test]
    fn assemble_str_rejects_invalid_json() {
        let e = assemble_str("{ not json", &StubRegistry).expect_err("invalid");
        assert_eq!(e.code, fayger_error::ErrorCode::FmParseFail);
    }

    #[test]
    fn assemble_str_happy_path() {
        let json = r#"{
            "name": "x", "version": "1.0.0", "description": "d", "vendor": "v",
            "modules": [{"id":"sensor"}], "protocols": [{"id":"cap_protocol"}],
            "controlMode": "command"
        }"#;
        let report = assemble_str(json, &StubRegistry).expect("assemble");
        assert_eq!(report.resolved_phase, 2);
        assert!(report.registry_is_stub);
    }

    /// 单一事实源纪律：`schema/faymanifest.draft.schema.json` 是 [`crate::schema`] 生成器的
    /// 产物（非手工编辑）。本测试断言磁盘文件与生成器一致并且是合法 JSON。
    ///
    /// 默认**只读校验**；当生成器变更需要重新落盘时，设置环境变量 `FAYMANIFEST_REGEN=1`
    /// 运行一次 `cargo test` 即可重写该文件。
    #[test]
    fn schema_file_matches_generator() {
        let generated = format!("{}\n", crate::schema::draft_schema_json());
        let path = crate::schema::schema_path();
        if std::env::var_os("FAYMANIFEST_REGEN").is_some() {
            std::fs::write(&path, &generated).expect("regenerate schema file");
        }
        let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
        // 短消息，避免在失败时把整份 schema 打印出来。
        assert!(
            on_disk == generated,
            "schema file is out of date; regenerate with FAYMANIFEST_REGEN=1 cargo test -p fayger-faymanifest"
        );
        assert!(serde_json::from_str::<serde_json::Value>(&on_disk).is_ok_and(|v| v.is_object()));
    }
}
