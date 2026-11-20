//! 一键构建并封装可运行的 `iFay` 数字灵魂运行包与 `coFay` 公共岗位席位包（`.buf`）。
//!
//! 对应 `fayger soul create --kind ifay|cofay --name <name> --out <path.buf>` 与控制台一键创建向导：
//! - 对 `iFay`：在 `FayID` 保险箱派生 `ifay_...`，生成六维 `iFay Profile`（身份、活跃 `Ego` 引用、
//!   `Thought`、`Skill`、`Hardware`、三级权限与对等主体 `Primary Replicas`）；
//! - 对 `coFay`：在 `FayID` 保险箱派生 `cofay_...`，以公开 [`RoleCharter`] `1.0` 替代私人 `Ego`；
//! - 两者统一将声明能力写入 `BuFManifest.capabilities`（用于运行期 `BuF ∩ HostPool ∩ Policy` 交集裁剪），
//!   封装并密封为标准 `.buf` 二进制镜像，同时兼容 `fayger inspect`、`fayger run` 与 `faygerd` 多租户挂载。

use crate::ego_package::hex_sha256;
use crate::error::CliError;
use crate::live_ifay::{TaskProgram, TaskStep, TASK_IR_SCHEMA_VERSION};
use crate::wallet::{derive_cofay, derive_ifay, prefixed_base32_id, RoleCharter};
use fayger_buf::format::{Header, Trailer, FORMAT_VERSION_M0};
use fayger_buf::manifest::types::{Digest, ProfileConstraints, SectionKind, SectionVisibility};
use fayger_buf::manifest::{BuFManifest, EntryPoint, SectionDescriptor, SemVer};
use fayger_buf::object::{BuFObject, Section};
use fayger_faymanifest::{
    ControlMode, DriverRef, EgoConfig, FayManifest, ModuleRef, PermissionsConfig, ProtocolRef,
};
use fayger_loader::seal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// 数字灵魂运行包元数据 schema 版本。
pub const SOUL_PACKAGE_SCHEMA_VERSION: &str = "fayger-soul-package/1.0";

/// 默认写入 `.buf` 清单的请求能力集。
pub const DEFAULT_SOUL_REQUESTED_CAPABILITIES: [&str; 5] = [
    "fs.read",
    "fs.write",
    "process.exec",
    "net.http",
    "media.camera",
];

/// `Fay` 灵魂类别：`iFay`（个人数字灵魂）或 `coFay`（公共岗位席位）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoulKind {
    /// 锚定 `Human Prime` 的个人数字灵魂。
    Ifay,
    /// 基于公开 `RoleCharter` 运作的组织/社区公共席位。
    Cofay,
}

impl SoulKind {
    /// 从 CLI 字符串解析 `ifay` 或 `cofay`。
    pub fn parse(raw: &str) -> Result<Self, CliError> {
        match raw.trim() {
            "ifay" => Ok(Self::Ifay),
            "cofay" => Ok(Self::Cofay),
            other => Err(CliError::Usage(format!(
                "invalid --kind '{other}' (expected 'ifay' or 'cofay')"
            ))),
        }
    }

    /// 稳定字符串标签。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ifay => "ifay",
            Self::Cofay => "cofay",
        }
    }
}

/// `iFay` 六维个人资产与记忆画像（`Profile` vs `Ego` 分离架构）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IfaySixDimProfile {
    /// 维度 1：身份锚点与隐身状态。
    pub identity_summary: String,
    /// 维度 2：活跃 `Ego` 可切分神经核引用。
    pub active_ego_ref: String,
    /// 维度 3：`Thought` 个人语境与记忆摘要条目。
    pub thought_entries: Vec<String>,
    /// 维度 4：`Skill` 已挂载技能列表。
    pub skill_refs: Vec<String>,
    /// 维度 5：`Hardware` 默认宿主驱动声明。
    pub hardware_drivers: Vec<String>,
    /// 维度 6：三级权限分层（固有 / 持久 / 会话动态确认）。
    pub three_tier_permissions: ThreeTierPermissions,
    /// 对等完整存储主体列表（Primary Replicas）。
    pub primary_replicas: Vec<String>,
}

/// `iFay Profile` 第六维：三级权限模型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreeTierPermissions {
    /// 一级固有能力。
    pub inherent: Vec<String>,
    /// 二级持久策略能力。
    pub persistent: Vec<String>,
    /// 三级高敏需 `Human Prime` 实时确认能力。
    pub human_confirm_required: Vec<String>,
}

/// 嵌入 `.buf` `soul_metadata` 段的结构化灵魂包描述。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoulPackageMetadata {
    /// 固定为 [`SOUL_PACKAGE_SCHEMA_VERSION`]。
    pub schema_version: String,
    /// 灵魂类别（`ifay` / `cofay`）。
    pub kind: SoulKind,
    /// 规范化 `Fay` 标识（`ifay_...` 或 `cofay_...`）。
    pub fay_id: String,
    /// 人类可读名称。
    pub display_name: String,
    /// 对应 Ed25519 验证公钥（十六进制）。
    pub public_key_hex: String,
    /// `BuF` 清单声明请求的能力集合（参与 `BuF ∩ HostPool ∩ Policy` 计算）。
    pub requested_capabilities: Vec<String>,
    /// 个人 `iFay` 六维画像（仅 `kind == Ifay` 时存在）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ifay_profile: Option<IfaySixDimProfile>,
    /// 公共 `coFay` 岗位章程（仅 `kind == Cofay` 时存在）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cofay_role_charter: Option<RoleCharter>,
}

/// 创建并密封一个完整可运行的 `.buf` `iFay` 或 `coFay` 镜像字节流。
#[allow(clippy::too_many_lines)]
pub fn create_soul_buf_bytes(
    state_dir: &Path,
    kind: SoulKind,
    name: &str,
    requested_caps_override: Option<&[String]>,
) -> Result<(Vec<u8>, SoulPackageMetadata), CliError> {
    let clean_name = name.trim();
    if clean_name.is_empty() {
        return Err(CliError::Usage(
            "soul create requires non-empty --name".to_owned(),
        ));
    }

    let caps_set: BTreeSet<String> = requested_caps_override.map_or_else(
        || {
            DEFAULT_SOUL_REQUESTED_CAPABILITIES
                .into_iter()
                .map(str::to_owned)
                .collect()
        },
        |list| {
            list.iter()
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect()
        },
    );
    let requested_capabilities: Vec<String> = caps_set.iter().cloned().collect();

    let metadata = match kind {
        SoulKind::Ifay => {
            let derived = derive_ifay(state_dir, clean_name)?;
            let profile = IfaySixDimProfile {
                identity_summary: format!(
                    "{} ({}) · stealth_unregistered",
                    clean_name, derived.ifay_id
                ),
                active_ego_ref: "@ego-models/personal-core-v1".to_owned(),
                thought_entries: vec![
                    format!("Owner preference: concise, high-signal engineering output for {clean_name}"),
                    "Redline: never expose plaintext Human ID or raw legacy API keys".to_owned(),
                ],
                skill_refs: vec![
                    "workspace.file_assistant".to_owned(),
                    "icp.ctx_composer".to_owned(),
                    "tp.cognitive_sync".to_owned(),
                ],
                hardware_drivers: vec!["@cap-drivers/fayger-desktop-host".to_owned()],
                three_tier_permissions: ThreeTierPermissions {
                    inherent: vec!["icp.reply".to_owned()],
                    persistent: vec!["fs.read".to_owned(), "fs.write".to_owned()],
                    human_confirm_required: vec![
                        "process.exec".to_owned(),
                        "media.camera".to_owned(),
                    ],
                },
                primary_replicas: vec!["local://fayger/primary-soul-replica".to_owned()],
            };
            SoulPackageMetadata {
                schema_version: SOUL_PACKAGE_SCHEMA_VERSION.to_owned(),
                kind: SoulKind::Ifay,
                fay_id: derived.ifay_id,
                display_name: clean_name.to_owned(),
                public_key_hex: derived.public_key_hex,
                requested_capabilities,
                ifay_profile: Some(profile),
                cofay_role_charter: None,
            }
        }
        SoulKind::Cofay => {
            let derived = derive_cofay(
                state_dir,
                clean_name,
                &format!("{clean_name} Public Role Seat"),
                "Execute chartered public workflows, CAP-gated workspace tasks, and TP coordination",
            )?;
            SoulPackageMetadata {
                schema_version: SOUL_PACKAGE_SCHEMA_VERSION.to_owned(),
                kind: SoulKind::Cofay,
                fay_id: derived.cofay_id,
                display_name: clean_name.to_owned(),
                public_key_hex: derived.public_key_hex,
                requested_capabilities,
                ifay_profile: None,
                cofay_role_charter: Some(derived.role_charter),
            }
        }
    };

    let fay_manifest = build_embedded_faymanifest(&metadata)?;
    let fay_manifest_bytes =
        serde_json::to_vec_pretty(&fay_manifest).map_err(|err| CliError::Io {
            path: "<fay_manifest>".to_owned(),
            detail: err.to_string(),
        })?;
    let metadata_bytes = serde_json::to_vec_pretty(&metadata).map_err(|err| CliError::Io {
        path: "<soul_metadata>".to_owned(),
        detail: err.to_string(),
    })?;

    let task_program = TaskProgram {
        schema_version: TASK_IR_SCHEMA_VERSION.to_owned(),
        task_id: format!("boot-{}", metadata.fay_id),
        required_cells: Vec::new(),
        steps: vec![TaskStep::CryptoSha256 {
            utf8_payload: format!("soul-boot:{}:{}", metadata.kind.as_str(), metadata.fay_id),
        }],
    };
    let code_bytes = serde_json::to_vec(&task_program).map_err(|err| CliError::Io {
        path: "<task_ir>".to_owned(),
        detail: err.to_string(),
    })?;

    let raw_sections = [
        ("code", SectionKind::Code, code_bytes),
        ("soul_metadata", SectionKind::Data, metadata_bytes),
        ("fay_manifest", SectionKind::Data, fay_manifest_bytes),
    ];

    let descriptors: Vec<SectionDescriptor> = raw_sections
        .iter()
        .map(|(id, sec_kind, payload)| SectionDescriptor {
            id: (*id).to_owned(),
            kind: sec_kind.clone(),
            digest: Digest::placeholder(),
            length: payload.len() as u64,
            visibility: SectionVisibility::Required,
            profile_constraints: ProfileConstraints::default(),
        })
        .collect();
    let sections: Vec<Section> = raw_sections
        .into_iter()
        .map(|(id, _, payload)| Section::new(id.to_owned(), payload))
        .collect();

    let v1 = SemVer::parse("1.0.0").unwrap_or(SemVer {
        major: 1,
        minor: 0,
        patch: 0,
        pre: None,
    });
    let buf_manifest = BuFManifest {
        schema_version: v1.clone(),
        runtime_interface_min: v1,
        deprecation_notice: None,
        entry: EntryPoint::new("code"),
        runtime: None,
        capabilities: caps_set,
        quotas: None,
        sections: descriptors,
        signature: None,
        extensions: BTreeMap::new(),
    };
    let mut obj = BuFObject {
        manifest: buf_manifest,
        sections,
        raw_header: Header {
            format_version: FORMAT_VERSION_M0,
            manifest_offset: 0,
            manifest_length: 0,
            section_index_offset: 0,
            section_index_length: 0,
        },
        raw_trailer: Trailer::placeholder(0),
    };
    seal(&mut obj)?;
    let bytes = fayger_buf::serialize(&obj)?;
    Ok((bytes, metadata))
}

/// 从任意已校验加载的 [`BuFObject`] 中提取或合成 [`SoulPackageMetadata`]。
#[must_use]
pub fn extract_soul_from_buf(obj: &BuFObject, fallback_name: &str) -> SoulPackageMetadata {
    if let Some(sec) = obj.sections.iter().find(|s| s.id == "soul_metadata") {
        if let Ok(mut parsed) = serde_json::from_slice::<SoulPackageMetadata>(&sec.payload) {
            // 始终以 BuFManifest.capabilities 为真实权威来源同步声明能力。
            if !obj.manifest.capabilities.is_empty() {
                parsed.requested_capabilities = obj.manifest.capabilities.iter().cloned().collect();
            }
            return parsed;
        }
    }

    let seed = format!(
        "generic-buf:{}:{}",
        fallback_name,
        obj.manifest.entry.as_str()
    );
    let fay_id = prefixed_base32_id("ifay_", seed.as_bytes());
    let pk_placeholder = hex_sha256(fay_id.as_bytes());
    let requested_capabilities: Vec<String> = if obj.manifest.capabilities.is_empty() {
        vec!["fs.read".to_owned()]
    } else {
        obj.manifest.capabilities.iter().cloned().collect()
    };

    SoulPackageMetadata {
        schema_version: SOUL_PACKAGE_SCHEMA_VERSION.to_owned(),
        kind: SoulKind::Ifay,
        fay_id,
        display_name: if fallback_name.trim().is_empty() {
            "generic-buf-fay".to_owned()
        } else {
            fallback_name.trim().to_owned()
        },
        public_key_hex: pk_placeholder,
        requested_capabilities,
        ifay_profile: None,
        cofay_role_charter: None,
    }
}

fn build_embedded_faymanifest(meta: &SoulPackageMetadata) -> Result<FayManifest, CliError> {
    let ego = if meta.kind == SoulKind::Ifay {
        Some(EgoConfig {
            model_source: "@ego-models/personal-core-v1".to_owned(),
            scenario_tags: vec!["workspace".to_owned(), "daily_intuition".to_owned()],
            constraints: None,
        })
    } else {
        None
    };

    let manifest = FayManifest {
        name: sanitize_manifest_name(&meta.display_name),
        version: "1.0.0".to_owned(),
        description: format!(
            "FayGer runnable {} package ({})",
            meta.kind.as_str(),
            meta.fay_id
        ),
        vendor: "fayger-local-soul-forge".to_owned(),
        fay_id: Some(meta.fay_id.clone()),
        modules: vec![ModuleRef {
            id: "workspace_core".to_owned(),
            version: Some("^1.0.0".to_owned()),
            provider: None,
            config: None,
        }],
        protocols: vec![
            ProtocolRef {
                id: "cap_protocol".to_owned(),
                version: Some("^1.0.0".to_owned()),
                config: None,
            },
            ProtocolRef {
                id: "faying_protocol".to_owned(),
                version: Some("^1.0.0".to_owned()),
                config: None,
            },
            ProtocolRef {
                id: "icp_protocol".to_owned(),
                version: Some("^1.0.0".to_owned()),
                config: None,
            },
            ProtocolRef {
                id: "tp_protocol".to_owned(),
                version: Some("^1.0.0".to_owned()),
                config: None,
            },
        ],
        control_mode: match meta.kind {
            SoulKind::Ifay => ControlMode::Ego,
            SoulKind::Cofay => ControlMode::Command,
        },
        drivers: vec![DriverRef {
            name: "fayger_desktop_host".to_owned(),
            driver_type: "host".to_owned(),
            driver_package: "@cap-drivers/fayger-desktop-host".to_owned(),
            config: None,
        }],
        ego,
        permissions: Some(PermissionsConfig {
            inherent: vec!["icp.reply".to_owned()],
            persistent: meta.requested_capabilities.clone(),
            auth_methods: vec!["fayid_mandate".to_owned()],
        }),
    };
    manifest.validate()?;
    Ok(manifest)
}

fn sanitize_manifest_name(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() {
            out.push('-');
        }
    }
    if out.is_empty() {
        "fay-soul-package".to_owned()
    } else {
        out
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{create_soul_buf_bytes, extract_soul_from_buf, SoulKind};
    use crate::commands::{default_policy, run_object};
    use fayger_loader::load;
    use fayger_runtime::LifecycleState;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn creates_runnable_ifay_and_cofay_buf_packages() {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("fayger-soul-test-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");

        let (ifay_bytes, ifay_meta) =
            create_soul_buf_bytes(&dir, SoulKind::Ifay, "Alice Soul", None).expect("create ifay");
        assert!(ifay_meta.fay_id.starts_with("ifay_"));
        assert!(ifay_meta.ifay_profile.is_some());

        let loaded = load(&ifay_bytes, &default_policy()).expect("load buf");
        let extracted = extract_soul_from_buf(&loaded, "fallback");
        assert_eq!(extracted.fay_id, ifay_meta.fay_id);
        let run_rep = run_object(loaded).expect("run buf");
        assert_eq!(run_rep.final_state, LifecycleState::Terminated);

        let (cofay_bytes, cofay_meta) =
            create_soul_buf_bytes(&dir, SoulKind::Cofay, "Ops Chair", None).expect("create cofay");
        assert!(cofay_meta.fay_id.starts_with("cofay_"));
        assert!(cofay_meta.cofay_role_charter.is_some());
        let loaded_co = load(&cofay_bytes, &default_policy()).expect("load cofay");
        assert!(!loaded_co.manifest.capabilities.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }
}
