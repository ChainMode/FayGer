//! 组件 / 包注册表抽象（草案）。
//!
//! 依赖解析需要一个「注册表」回答：某模块 / 协议 / 命名空间包是否存在、有哪些可用版本。
//!
//! **本波 stub 说明（重要）**：真正的注册表后端（在线索引、签名、版本发布流水线、
//! `@drone-drivers/...`·`@ego-models/...` 的治理）是 **H2** 的设计范围。H1 草案只定义
//! [`Registry`] trait 与两个实现：
//!
//! - [`StubRegistry`]：占位实现，所有查询返回 [`ResolutionOutcome::Stub`]。装配器据此把对应
//!   依赖明确标注为「stubbed（注册表未接入）」，**不**误报为"缺失"。
//! - [`InMemoryRegistry`]：内存实现，供测试 / 本地实验显式登记已知项与可用版本，使版本约束
//!   解析可被端到端验证。

use fayger_buf::manifest::SemVer;
use std::collections::BTreeMap;

/// 注册表对一次查询的回答。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionOutcome {
    /// 注册表已知该项，附可用版本列表（空列表表示已知但无版本信息）。
    Known(Vec<SemVer>),
    /// 注册表确定不存在该项。
    Unknown,
    /// 注册表未实现 / 未接入该类查询（H1 占位；真实后端见 H2）。
    Stub,
}

/// 组件与包的解析后端。
pub trait Registry {
    /// 查询模块 id。
    fn lookup_module(&self, id: &str) -> ResolutionOutcome;
    /// 查询协议 id。
    fn lookup_protocol(&self, id: &str) -> ResolutionOutcome;
    /// 查询命名空间包标识（`@scope/name`）。
    fn lookup_package(&self, identifier: &str) -> ResolutionOutcome;
    /// 该注册表是否为占位实现（未接入真实后端）。默认 `false`；
    /// [`StubRegistry`] 覆盖为 `true`，用于在装配报告中如实标注。
    fn is_stub(&self) -> bool {
        false
    }
}

/// H1 占位注册表：尚无注册表后端（设计见 H2）。所有查询返回 [`ResolutionOutcome::Stub`]。
#[derive(Debug, Clone, Copy, Default)]
pub struct StubRegistry;

impl Registry for StubRegistry {
    fn lookup_module(&self, _id: &str) -> ResolutionOutcome {
        ResolutionOutcome::Stub
    }
    fn lookup_protocol(&self, _id: &str) -> ResolutionOutcome {
        ResolutionOutcome::Stub
    }
    fn lookup_package(&self, _identifier: &str) -> ResolutionOutcome {
        ResolutionOutcome::Stub
    }
    fn is_stub(&self) -> bool {
        true
    }
}

/// 内存注册表（测试 / 本地实验）：显式登记已知模块 / 协议 / 包及其可用版本。
#[derive(Debug, Clone, Default)]
pub struct InMemoryRegistry {
    modules: BTreeMap<String, Vec<SemVer>>,
    protocols: BTreeMap<String, Vec<SemVer>>,
    packages: BTreeMap<String, Vec<SemVer>>,
}

impl InMemoryRegistry {
    /// 空注册表。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记一个模块及其可用版本（链式）。
    #[must_use]
    pub fn with_module(mut self, id: &str, versions: &[&str]) -> Self {
        self.modules.insert(id.to_owned(), parse_versions(versions));
        self
    }

    /// 登记一个协议及其可用版本（链式）。
    #[must_use]
    pub fn with_protocol(mut self, id: &str, versions: &[&str]) -> Self {
        self.protocols
            .insert(id.to_owned(), parse_versions(versions));
        self
    }

    /// 登记一个命名空间包及其可用版本（链式）。
    #[must_use]
    pub fn with_package(mut self, identifier: &str, versions: &[&str]) -> Self {
        self.packages
            .insert(identifier.to_owned(), parse_versions(versions));
        self
    }
}

fn parse_versions(versions: &[&str]) -> Vec<SemVer> {
    versions
        .iter()
        .filter_map(|v| SemVer::parse(v).ok())
        .collect()
}

fn outcome(map: &BTreeMap<String, Vec<SemVer>>, key: &str) -> ResolutionOutcome {
    map.get(key).map_or(ResolutionOutcome::Unknown, |vs| {
        ResolutionOutcome::Known(vs.clone())
    })
}

impl Registry for InMemoryRegistry {
    fn lookup_module(&self, id: &str) -> ResolutionOutcome {
        outcome(&self.modules, id)
    }
    fn lookup_protocol(&self, id: &str) -> ResolutionOutcome {
        outcome(&self.protocols, id)
    }
    fn lookup_package(&self, identifier: &str) -> ResolutionOutcome {
        outcome(&self.packages, identifier)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{InMemoryRegistry, Registry, ResolutionOutcome, StubRegistry};
    use fayger_buf::manifest::SemVer;

    #[test]
    fn stub_registry_returns_stub_for_everything() {
        let r = StubRegistry;
        assert_eq!(r.lookup_module("sensor"), ResolutionOutcome::Stub);
        assert_eq!(r.lookup_protocol("cap_protocol"), ResolutionOutcome::Stub);
        assert_eq!(
            r.lookup_package("@drone-drivers/dji-fc"),
            ResolutionOutcome::Stub
        );
    }

    #[test]
    fn in_memory_known_and_unknown() {
        let r = InMemoryRegistry::new()
            .with_module("sensor", &["1.0.0", "1.2.0"])
            .with_package("@drone-drivers/dji-fc", &["3.0.0"]);
        assert_eq!(
            r.lookup_module("sensor"),
            ResolutionOutcome::Known(vec![
                SemVer::parse("1.0.0").expect("v"),
                SemVer::parse("1.2.0").expect("v"),
            ])
        );
        assert_eq!(r.lookup_module("missing"), ResolutionOutcome::Unknown);
        assert!(matches!(
            r.lookup_package("@drone-drivers/dji-fc"),
            ResolutionOutcome::Known(_)
        ));
    }
}
