//! Runtime_Implementation 注册表：注册完备性校验与路由。
//!
//! - [`RuntimeRegistry::register`]：校验候选 [`RuntimeDescriptor`] 是否覆盖
//!   [`RuntimeOperation::required`] 全部必需操作；缺失则 `RT_IMPL_MISSING_METHODS`。
//! - [`RuntimeRegistry::resolve`]：按 BuFManifest 声明的选择策略与版本兼容性路由到一个
//!   已注册实现；确定性（按注册顺序取首个兼容者）。

use crate::interface::{RuntimeDescriptor, RuntimeOperation};
use fayger_buf::manifest::types::SelectionStrategy;
use fayger_buf::manifest::BuFManifest;
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};

/// 注册标识：已注册实现的注册顺序索引。提供确定性引用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RegistrationId(pub usize);

/// 运行层实现注册表。
#[derive(Debug, Default)]
pub struct RuntimeRegistry {
    entries: Vec<RuntimeDescriptor>,
}

impl RuntimeRegistry {
    /// 构造空注册表。
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// 注册一个实现。校验注册完备性：`supported_operations ⊇ REQUIRED_OPERATIONS`。
    ///
    /// # Errors
    ///
    /// 缺少任一必需操作时返回 `RT_IMPL_MISSING_METHODS`，`context.missing_methods` 为
    /// `REQUIRED_OPERATIONS \ supported_operations`（按 `RuntimeOperation` 排序的有序列表）。
    pub fn register(&mut self, desc: RuntimeDescriptor) -> Result<RegistrationId, FaygerError> {
        let required = RuntimeOperation::required();
        let missing: Vec<RuntimeOperation> = required
            .difference(&desc.supported_operations)
            .copied()
            .collect();
        if !missing.is_empty() {
            let missing_list = missing
                .iter()
                .map(|op| ContextValue::string(format!("{op:?}")))
                .collect();
            return Err(FaygerError::new(
                ErrorCode::RtImplMissingMethods,
                Layer::Runtime,
                "runtime implementation is missing required operations",
            )
            .with_context("impl_id", ContextValue::string(desc.impl_id))
            .with_context("missing_methods", ContextValue::List(missing_list)));
        }
        let id = RegistrationId(self.entries.len());
        self.entries.push(desc);
        Ok(id)
    }

    /// 返回所有已注册实现的描述符（按注册顺序）。
    #[must_use]
    pub fn list(&self) -> &[RuntimeDescriptor] {
        &self.entries
    }

    /// 按 `manifest.runtime` 声明的策略路由到一个已注册实现。
    ///
    /// 兼容判定：`desc.rt_version >= manifest.runtime_interface_min`。确定性：候选集合
    /// 按注册顺序遍历取首个。
    ///
    /// # Errors
    ///
    /// 无满足策略与兼容性的实现时返回 `RT_IMPL_NOT_FOUND`。
    pub fn resolve(&self, manifest: &BuFManifest) -> Result<RegistrationId, FaygerError> {
        // 解析策略与 preferred_impl（manifest.runtime 缺省视为 Any）。
        let (strategy, preferred) = manifest
            .runtime
            .as_ref()
            .map_or((SelectionStrategy::Any, None), |r| {
                (r.selection_strategy, r.preferred_impl.as_deref())
            });

        let compatible =
            |desc: &RuntimeDescriptor| desc.rt_version >= manifest.runtime_interface_min;

        // 找 preferred（若声明）：已注册 且 兼容。
        let preferred_hit = preferred.and_then(|pid| {
            self.entries
                .iter()
                .position(|d| d.impl_id == pid && compatible(d))
                .map(RegistrationId)
        });

        match strategy {
            SelectionStrategy::Strict => {
                preferred_hit.ok_or_else(|| Self::not_found(strategy, preferred, manifest))
            }
            SelectionStrategy::PreferThenAny => preferred_hit
                .or_else(|| self.first_compatible(&compatible))
                .ok_or_else(|| Self::not_found(strategy, preferred, manifest)),
            SelectionStrategy::Any => self
                .first_compatible(&compatible)
                .ok_or_else(|| Self::not_found(strategy, preferred, manifest)),
        }
    }

    /// 按注册顺序返回首个兼容实现。
    fn first_compatible(
        &self,
        compatible: &impl Fn(&RuntimeDescriptor) -> bool,
    ) -> Option<RegistrationId> {
        self.entries.iter().position(compatible).map(RegistrationId)
    }

    fn not_found(
        strategy: SelectionStrategy,
        preferred: Option<&str>,
        manifest: &BuFManifest,
    ) -> FaygerError {
        let mut err = FaygerError::new(
            ErrorCode::RtImplNotFound,
            Layer::Runtime,
            "no registered runtime implementation satisfies the manifest",
        )
        .with_context("strategy", ContextValue::string(format!("{strategy:?}")))
        .with_context(
            "runtime_interface_min",
            ContextValue::string(manifest.runtime_interface_min.to_string()),
        );
        if let Some(pid) = preferred {
            err = err.with_context("preferred_impl", ContextValue::string(pid.to_owned()));
        }
        err
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{RegistrationId, RuntimeRegistry};
    use crate::interface::{RuntimeDescriptor, RuntimeOperation};
    use fayger_buf::manifest::types::{RuntimeRequirement, SelectionStrategy};
    use fayger_buf::manifest::{BuFManifest, EntryPoint, SemVer};
    use fayger_error::ErrorCode;
    use std::collections::{BTreeMap, BTreeSet};

    fn v(s: &str) -> SemVer {
        SemVer::parse(s).expect("parse")
    }

    fn desc(id: &str, ver: &str) -> RuntimeDescriptor {
        RuntimeDescriptor::complete(id, v(ver))
    }

    fn manifest(rt_min: &str, runtime: Option<RuntimeRequirement>) -> BuFManifest {
        BuFManifest {
            schema_version: v("1.0.0"),
            runtime_interface_min: v(rt_min),
            deprecation_notice: None,
            entry: EntryPoint::new("main"),
            runtime,
            capabilities: BTreeSet::new(),
            quotas: None,
            sections: vec![],
            signature: None,
            extensions: BTreeMap::new(),
        }
    }

    fn req(strategy: SelectionStrategy, preferred: Option<&str>) -> RuntimeRequirement {
        RuntimeRequirement {
            preferred_impl: preferred.map(String::from),
            selection_strategy: strategy,
        }
    }

    #[test]
    fn registers_complete_descriptor() {
        let mut r = RuntimeRegistry::new();
        let id = r.register(desc("ref", "1.0.0")).expect("register ok");
        assert_eq!(id, RegistrationId(0));
        assert_eq!(r.list().len(), 1);
    }

    #[test]
    fn rejects_missing_operations() {
        let mut r = RuntimeRegistry::new();
        let mut ops = RuntimeOperation::required();
        ops.remove(&RuntimeOperation::Emit);
        ops.remove(&RuntimeOperation::OnEvent);
        let d = RuntimeDescriptor {
            impl_id: "partial".into(),
            rt_version: v("1.0.0"),
            supported_operations: ops,
        };
        let err = r.register(d).expect_err("should fail");
        assert_eq!(err.code, ErrorCode::RtImplMissingMethods);
        // missing 应恰为 {Emit, OnEvent}
        let missing = err.context.get("missing_methods").expect("missing_methods");
        match missing {
            fayger_error::ContextValue::List(l) => {
                let names: BTreeSet<String> = l
                    .iter()
                    .filter_map(|v| match v {
                        fayger_error::ContextValue::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    names,
                    ["Emit", "OnEvent"].into_iter().map(String::from).collect()
                );
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn strict_returns_preferred_when_registered_and_compatible() {
        let mut r = RuntimeRegistry::new();
        r.register(desc("a", "1.0.0")).unwrap();
        let b = r.register(desc("b", "1.2.0")).unwrap();
        let m = manifest("1.0.0", Some(req(SelectionStrategy::Strict, Some("b"))));
        assert_eq!(r.resolve(&m).unwrap(), b);
    }

    #[test]
    fn strict_fails_when_preferred_missing() {
        let mut r = RuntimeRegistry::new();
        r.register(desc("a", "1.0.0")).unwrap();
        let m = manifest("1.0.0", Some(req(SelectionStrategy::Strict, Some("zzz"))));
        let err = r.resolve(&m).expect_err("should fail");
        assert_eq!(err.code, ErrorCode::RtImplNotFound);
    }

    #[test]
    fn strict_fails_when_preferred_incompatible() {
        let mut r = RuntimeRegistry::new();
        r.register(desc("a", "1.0.0")).unwrap();
        // 要求 rt_min 1.5.0，但 a 只提供 1.0.0 → 不兼容
        let m = manifest("1.5.0", Some(req(SelectionStrategy::Strict, Some("a"))));
        let err = r.resolve(&m).expect_err("should fail");
        assert_eq!(err.code, ErrorCode::RtImplNotFound);
    }

    #[test]
    fn prefer_then_any_falls_back_to_first_compatible() {
        let mut r = RuntimeRegistry::new();
        let a = r.register(desc("a", "1.0.0")).unwrap();
        r.register(desc("b", "1.0.0")).unwrap();
        // preferred "zzz" 不存在 → 回退到首个兼容者 a
        let m = manifest(
            "1.0.0",
            Some(req(SelectionStrategy::PreferThenAny, Some("zzz"))),
        );
        assert_eq!(r.resolve(&m).unwrap(), a);
    }

    #[test]
    fn any_returns_first_compatible_deterministically() {
        let mut r = RuntimeRegistry::new();
        r.register(desc("a", "0.9.0")).unwrap(); // 不兼容 rt_min 1.0.0
        let b = r.register(desc("b", "1.0.0")).unwrap();
        r.register(desc("c", "1.2.0")).unwrap();
        let m = manifest("1.0.0", Some(req(SelectionStrategy::Any, None)));
        assert_eq!(r.resolve(&m).unwrap(), b);
        // 确定性：多次相同
        assert_eq!(r.resolve(&m).unwrap(), b);
    }

    #[test]
    fn missing_runtime_requirement_defaults_to_any() {
        let mut r = RuntimeRegistry::new();
        let a = r.register(desc("a", "1.0.0")).unwrap();
        let m = manifest("1.0.0", None);
        assert_eq!(r.resolve(&m).unwrap(), a);
    }

    #[test]
    fn any_fails_when_no_compatible() {
        let mut r = RuntimeRegistry::new();
        r.register(desc("a", "0.9.0")).unwrap();
        let m = manifest("1.0.0", Some(req(SelectionStrategy::Any, None)));
        let err = r.resolve(&m).expect_err("should fail");
        assert_eq!(err.code, ErrorCode::RtImplNotFound);
    }
}
