//! `fayger doctor` 冷启动复现自检与零个人身份信息（Zero-PII）匿名验证回执生成器。
//!
//! 执行 6 项毫秒级本地确定性自检：
//! 1. `binary_and_platform`：校验版本、平台架构及 `$FAYGER_HOME/bin/fayger` 安装态（绝不回显真实 `$HOME` 或用户名）；
//! 2. `state_dir_rw`：校验状态目录可创建、属主读写正常（统一脱敏展示为 `$FAYGER_HOME/state`）；
//! 3. `runtime_conformance`：内存运行 `Runtime_Interface` 黑盒一致性套件 + `Faying` 拨杆（`ROGUE` 拒绝 / `FAYING` 放行）自检；
//! 4. `cap_negative_gate`：内存运行生产级 `CAP 1.0` 等级不足负向门禁校验（确定性拦截并返回 `E_GRADE_INSUFFICIENT`）；
//! 5. `audit_chain_integrity`：校验现有或临时 3 节点 SHA-256 哈希审计链完整性及防篡改检出能力；
//! 6. `companion_endpoints`：200ms 非阻塞探测本机回环 `127.0.0.1:7979` (`faygerd`)、`:7980` (`ego`)、`:8546` (`gmchain`) 在线状态（纯信息展示，绝不因可选伴随服务未启动而标红失败）。

use crate::ego_package::{encode_hex, hex_sha256};
use crate::engine::{FayGerEngine, FayingState};
use crate::live_ifay::{verify_audit_chain, AuditChainBuilder, HashChainedAuditEntry};
use crate::soul::SoulKind;
use crate::wallet::unix_now_secs;
use ed25519_dalek::{Signer, SigningKey};
use fayger_cap::{
    canonical_json, evaluate_production_command, AccessMode, CapErrorCode, CapGrant,
    ConsequenceIrreversibility, Criticality, DomainCategory, FayingGrade, GrantMinGrade,
    MandateBindingModel, ProductionDisposition, ProductionEvaluationRequest,
    ProductionMandateEvidence, ProductionOperationDescriptor, ProductionResourceDescriptor,
    ProductionSafeStateDescriptor, RealExecutionGateConfig, SignedDriverManifest, TrustAnchorStore,
};
use fayger_conformance::{run_conformance_suite, ConformanceVerdict, ReferenceRuntimeHarness};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fmt::Write as _;
use std::fs;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// `fayger doctor` JSON 报告 schema 版本。
pub const DOCTOR_SCHEMA_VERSION: &str = "fayger-doctor/1.0";

static DOCTOR_PROBE_SEQ: AtomicU64 = AtomicU64::new(0);

/// 单项自检门禁结果（对外严格零 PII）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorCheckItem {
    /// 检查项稳定标识。
    pub id: String,
    /// 是否属于必过核心门禁（前 5 项为 `true`，伴随端口探测为 `false`）。
    pub core_gate: bool,
    /// 状态：`"PASS"` / `"FAIL"` / `"INFO"`。
    pub status: String,
    /// 脱敏摘要说明（绝不含真实用户名、`$HOME` 展开路径、主机名或密钥）。
    pub summary: String,
}

/// 本机可选伴随服务回环端口在线快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionEndpointStatus {
    /// `127.0.0.1:7979` (`faygerd`) 状态：`"online"` 或 `"not_running"`。
    pub faygerd: String,
    /// `127.0.0.1:7980` (`ego`) 状态：`"online"` 或 `"not_running"`。
    pub ego: String,
    /// `127.0.0.1:8546` (`gmchain`) 状态：`"online"` 或 `"not_running"`。
    pub gmchain: String,
}

/// `fayger doctor` 与 `GET /api/v1/doctor` 完整自检报告及匿名复现回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorReport {
    /// 报告 schema 版本（`"fayger-doctor/1.0"`）。
    pub schema_version: String,
    /// 带 `v` 前缀的版本字符串（如 `"v0.3.1"`）。
    pub fayger_version: String,
    /// 操作系统标识（`std::env::consts::OS`）。
    pub os: String,
    /// 处理器架构标识（`std::env::consts::ARCH`）。
    pub arch: String,
    /// UTC 日期（`YYYY-MM-DD`）。
    pub utc_date: String,
    /// 5 项核心安全与运行门禁是否全部通过。
    pub all_core_passed: bool,
    /// 通过的核心门禁数。
    pub passed_core_count: usize,
    /// 核心门禁总数（固定为 `5`）。
    pub total_core_count: usize,
    /// 6 项自检明细。
    pub checks: Vec<DoctorCheckItem>,
    /// 本机回环伴随服务探测结果。
    pub companions: CompanionEndpointStatus,
    /// 基于 `(version, os, arch, check_statuses, utc_date)` 计算的确定性回执摘要（`sha256:<16-hex>`）。
    pub receipt_digest: String,
    /// 可直接复制粘贴至 GitHub Issue / Discussion 的零 PII Markdown 匿名复现回执。
    pub markdown_receipt: String,
}

impl DoctorReport {
    /// 渲染终端人类可读自检报告（含 6 项明细与可直接复制的 Markdown 匿名复现回执）。
    #[must_use]
    pub fn render_human(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "FayGer Doctor ({} · {}/{}) — Core Gates: {}/{} {}",
            self.fayger_version,
            self.os,
            self.arch,
            self.passed_core_count,
            self.total_core_count,
            if self.all_core_passed { "PASS" } else { "FAIL" }
        );
        for item in &self.checks {
            let badge = match item.status.as_str() {
                "PASS" => "[PASS]",
                "INFO" => "[INFO]",
                _ => "[FAIL]",
            };
            let _ = writeln!(out, "  {badge} {:<23} {}", item.id, item.summary);
        }
        let _ = writeln!(
            out,
            "\nCopy-pasteable Anonymous Reproduction Receipt (Zero-PII):\n"
        );
        out.push_str(&self.markdown_receipt);
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out
    }
}

/// 执行完整的本地确定性自检并生成 [`DoctorReport`]。
///
/// 当由 `faygerd` 守护进程内部处理 `GET /api/v1/doctor` 时传入 `daemon_self_online = true`。
#[must_use]
pub fn run_doctor(state_dir: &Path, daemon_self_online: bool) -> DoctorReport {
    let version_tag = format!("v{}", crate::FAYGER_VERSION);
    let os = std::env::consts::OS.to_owned();
    let arch = std::env::consts::ARCH.to_owned();
    let utc_date = format_utc_date_yyyy_mm_dd(unix_now_secs());

    let c1_bin = check_binary_and_platform(&version_tag, &os, &arch);
    let c2_state = check_state_dir_rw(state_dir);
    let c3_rt = check_runtime_conformance_and_faying();
    let c4_cap = check_cap_negative_gate();
    let c5_audit = check_audit_chain_integrity(state_dir);
    let (c6_comp, companions) = check_companion_endpoints(daemon_self_online);

    let binary_st = c1_bin.status.clone();
    let state_st = c2_state.status.clone();
    let faying_st = c3_rt.status.clone();
    let cap_st = c4_cap.status.clone();
    let audit_st = c5_audit.status.clone();

    let checks = vec![c1_bin, c2_state, c3_rt, c4_cap, c5_audit, c6_comp];
    let passed_core_count = checks
        .iter()
        .filter(|c| c.core_gate && c.status == "PASS")
        .count();
    let total_core_count = checks.iter().filter(|c| c.core_gate).count();
    let all_core_passed = passed_core_count == total_core_count;

    let digest_input = format!(
        "{DOCTOR_SCHEMA_VERSION}|{version_tag}|{os}|{arch}|state_rw={state_st}|faying_switch={faying_st}|cap_deny_gate={cap_st}|audit_sha256={audit_st}|binary={binary_st}|{utc_date}"
    );
    let full_sha = hex_sha256(digest_input.as_bytes());
    let receipt_digest = format!("sha256:{}", &full_sha[..16]);

    let core_verdict = if all_core_passed { "PASS" } else { "FAIL" };
    let raw_markdown = format!(
        "### FayGer Cold-Start Verification Receipt (Zero-PII)\n\
- Version: {version_tag} ({os}/{arch})\n\
- Date (UTC): {utc_date}\n\
- Core Gates: {passed_core_count}/{total_core_count} {core_verdict} (state_rw={state_st}, faying_switch={faying_st}, cap_deny_gate={cap_st}, audit_sha256={audit_st}, binary={binary_st})\n\
- Local Companions: faygerd={}, ego={}, gmchain={}\n\
- Receipt Digest: {receipt_digest}",
        companions.faygerd, companions.ego, companions.gmchain
    );
    let markdown_receipt = scrub_env_pii(&raw_markdown);

    DoctorReport {
        schema_version: DOCTOR_SCHEMA_VERSION.to_owned(),
        fayger_version: version_tag,
        os,
        arch,
        utc_date,
        all_core_passed,
        passed_core_count,
        total_core_count,
        checks,
        companions,
        receipt_digest,
        markdown_receipt,
    }
}

fn check_binary_and_platform(version_tag: &str, os: &str, arch: &str) -> DoctorCheckItem {
    let bin_name = if os == "windows" {
        "fayger.exe"
    } else {
        "fayger"
    };
    let installed_in_fayger_home = default_user_root_dir()
        .is_some_and(|root| root.join(".fayger").join("bin").join(bin_name).is_file());
    let install_note = if installed_in_fayger_home {
        "$FAYGER_HOME/bin/fayger=installed"
    } else {
        "$FAYGER_HOME/bin/fayger=portable_or_cargo_bin"
    };
    DoctorCheckItem {
        id: "binary_and_platform".to_owned(),
        core_gate: true,
        status: "PASS".to_owned(),
        summary: scrub_env_pii(&format!("{version_tag} ({os}/{arch}) · {install_note}")),
    }
}

fn check_state_dir_rw(state_dir: &Path) -> DoctorCheckItem {
    let create_res = fs::create_dir_all(state_dir);
    if let Err(err) = create_res {
        return DoctorCheckItem {
            id: "state_dir_rw".to_owned(),
            core_gate: true,
            status: "FAIL".to_owned(),
            summary: scrub_env_pii(&format!(
                "$FAYGER_HOME/state directory creation failed ({})",
                err.kind()
            )),
        };
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(state_dir, fs::Permissions::from_mode(0o700));
    }

    let seq = DOCTOR_PROBE_SEQ.fetch_add(1, Ordering::Relaxed);
    let probe_file = state_dir.join(format!(".doctor_rw_probe_{}_{seq}", std::process::id()));
    let write_res = fs::write(&probe_file, b"fayger-doctor-rw-ok");
    if let Err(err) = write_res {
        return DoctorCheckItem {
            id: "state_dir_rw".to_owned(),
            core_gate: true,
            status: "FAIL".to_owned(),
            summary: scrub_env_pii(&format!(
                "$FAYGER_HOME/state write probe failed ({})",
                err.kind()
            )),
        };
    }

    #[cfg(unix)]
    let mode_desc = {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&probe_file, fs::Permissions::from_mode(0o600));
        let dir_mode = fs::metadata(state_dir).map_or(0o700, |m| m.permissions().mode() & 0o777);
        format!("owner-only mode {dir_mode:04o}")
    };
    #[cfg(not(unix))]
    let mode_desc = "user-profile rw verified".to_owned();

    let read_ok = fs::read(&probe_file).is_ok_and(|b| b == b"fayger-doctor-rw-ok");
    let _ = fs::remove_file(&probe_file);

    if read_ok {
        DoctorCheckItem {
            id: "state_dir_rw".to_owned(),
            core_gate: true,
            status: "PASS".to_owned(),
            summary: scrub_env_pii(&format!("$FAYGER_HOME/state writable ({mode_desc})")),
        }
    } else {
        DoctorCheckItem {
            id: "state_dir_rw".to_owned(),
            core_gate: true,
            status: "FAIL".to_owned(),
            summary: "$FAYGER_HOME/state readback verification failed".to_owned(),
        }
    }
}

fn check_runtime_conformance_and_faying() -> DoctorCheckItem {
    let conf_report = run_conformance_suite(ReferenceRuntimeHarness::new);
    if conf_report.verdict != ConformanceVerdict::Conformant {
        return DoctorCheckItem {
            id: "runtime_conformance".to_owned(),
            core_gate: true,
            status: "FAIL".to_owned(),
            summary: "Runtime_Interface conformance suite reported non-conformant".to_owned(),
        };
    }

    let seq = DOCTOR_PROBE_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp_dir =
        std::env::temp_dir().join(format!("fayger-doctor-faying-{}-{seq}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp_dir);

    let faying_res = (|| -> Result<(), String> {
        let mut eng = FayGerEngine::open(&tmp_dir).map_err(|e| e.to_string())?;
        let soul = eng
            .create_and_load_soul(SoulKind::Ifay, "DoctorProbe", None)
            .map_err(|e| e.to_string())?;
        if soul.faying_state != FayingState::Rogue {
            return Err("initial soul state was not ROGUE".to_owned());
        }
        let rogue_turn = eng
            .chat_or_act(&soul.fay_id, "/write probe.txt blocked", None)
            .map_err(|e| e.to_string())?;
        if !rogue_turn.terminal_text.contains("DENY_ROGUE_FAY") {
            return Err("ROGUE state did not reject host write".to_owned());
        }
        let active = eng
            .activate_faying(&soul.fay_id, Some("G2"), Some(60), Some(30))
            .map_err(|e| e.to_string())?;
        if active.faying_state != FayingState::Faying {
            return Err("activate_faying did not enter FAYING state".to_owned());
        }
        let allow_turn = eng
            .chat_or_act(&soul.fay_id, "/write probe.txt allowed", None)
            .map_err(|e| e.to_string())?;
        if !allow_turn.terminal_text.contains("ALLOW_CONTROLLED") {
            return Err("FAYING state did not allow controlled sandbox write".to_owned());
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&tmp_dir);

    match faying_res {
        Ok(()) => DoctorCheckItem {
            id: "runtime_conformance".to_owned(),
            core_gate: true,
            status: "PASS".to_owned(),
            summary: format!(
                "Runtime_Interface {}/{} PASS · Faying OFF->DENY / ON->ALLOW verified",
                conf_report.passed_count, conf_report.total_count
            ),
        },
        Err(reason) => DoctorCheckItem {
            id: "runtime_conformance".to_owned(),
            core_gate: true,
            status: "FAIL".to_owned(),
            summary: scrub_env_pii(&format!("Faying lifecycle check failed: {reason}")),
        },
    }
}

#[allow(clippy::too_many_lines)]
fn check_cap_negative_gate() -> DoctorCheckItem {
    let fay_id = "ifay_7q2x5n3p4r6t2v6w7y3z5b7d6f";
    let signer_key_id = "doctor-cap-anchor-01";
    let signing_key = SigningKey::from_bytes(&[0x27; 32]);
    let pubkey_hex = encode_hex(&signing_key.verifying_key().to_bytes());

    let mut manifest = SignedDriverManifest {
        manifest_id: "drv-doctor-probe-1.0".to_owned(),
        driver_id: "@ifay-drivers/doctor-gate".to_owned(),
        version: "1.0.0".to_owned(),
        issued_at: "2026-11-25T00:00:00Z".to_owned(),
        signer_key_id: signer_key_id.to_owned(),
        resources: vec![ProductionResourceDescriptor {
            resource_id: "host_privileged_slot".to_owned(),
            safe_states: vec![ProductionSafeStateDescriptor {
                safe_state_id: "halt_privileged_slot".to_owned(),
                description: Some("Immediate safe halt on insufficient grade.".to_owned()),
            }],
            operations: vec![ProductionOperationDescriptor {
                operation_id: "elevated_reconfigure".to_owned(),
                mode: AccessMode::Write,
                criticality: Criticality::Sensitive,
                consequence_irreversibility: ConsequenceIrreversibility::BoundedCompensation,
                domain_category: Some(DomainCategory::OsResource),
            }],
        }],
        signature_ed25519: None,
    };

    let Ok(unsigned_val) = serde_json::to_value(&manifest) else {
        return DoctorCheckItem {
            id: "cap_negative_gate".to_owned(),
            core_gate: true,
            status: "FAIL".to_owned(),
            summary: "failed to serialize CAP driver manifest".to_owned(),
        };
    };
    let sig = signing_key.sign(canonical_json(&unsigned_val).as_bytes());
    manifest.signature_ed25519 = Some(encode_hex(&sig.to_bytes()));

    let mut anchors = TrustAnchorStore::new();
    if anchors
        .insert_ed25519_hex(signer_key_id.to_owned(), &pubkey_hex)
        .is_err()
    {
        return DoctorCheckItem {
            id: "cap_negative_gate".to_owned(),
            core_gate: true,
            status: "FAIL".to_owned(),
            summary: "failed to register CAP trust anchor".to_owned(),
        };
    }

    // Grant requires elevated G2+ (GrantMinGrade::G2), while active session holds lower G3 authority ->
    // CAP 1.0 production evaluator MUST deterministically DENY with E_GRADE_INSUFFICIENT (2002).
    let req = ProductionEvaluationRequest {
        protocol_version: "1.0".to_owned(),
        request_id: "doctor::cap::negative_grade_gate".to_owned(),
        fay_id: fay_id.to_owned(),
        resource_id: "host_privileged_slot".to_owned(),
        operation_id: "elevated_reconfigure".to_owned(),
        requested_mode: AccessMode::Write,
        params: json!({"probe": true}),
        domain_category: Some(DomainCategory::OsResource),
        caller_criticality_override: None,
        g0_override: false,
        now_epoch_ms: 1_795_564_800_000,
        manifest,
        grant: CapGrant {
            grant_id: "grant-doctor-neg-01".to_owned(),
            resource_id: "host_privileged_slot".to_owned(),
            allowed_modes: vec![AccessMode::Read, AccessMode::Write],
            min_grade: GrantMinGrade::G2,
            criticality_override: None,
            mandate_binding: ProductionMandateEvidence {
                mandate_id: "mandate-doctor-neg-01".to_owned(),
                fay_id: fay_id.to_owned(),
                resource_id: "host_privileged_slot".to_owned(),
                allowed_modes: vec![AccessMode::Read, AccessMode::Write],
                session_grade: FayingGrade::G3,
                binding_model: MandateBindingModel::ModelAPlusB,
                online_fresh: true,
                revoked: false,
                revocation_checked_at_ms: 1_795_564_795_000,
                max_revocation_staleness_ms: 60_000,
                expires_at_epoch_ms: 1_795_568_400_000,
                model_a_derived_proof_valid: true,
            },
        },
        human_confirmation: None,
    };

    let gate_cfg = RealExecutionGateConfig::default();
    let audit = evaluate_production_command(&req, &anchors, &gate_cfg);
    if audit.disposition == ProductionDisposition::Deny
        && audit.error_code == Some(CapErrorCode::GradeInsufficient)
    {
        DoctorCheckItem {
            id: "cap_negative_gate".to_owned(),
            core_gate: true,
            status: "PASS".to_owned(),
            summary: "Insufficient Faying grade deterministically rejected -> DENY (E_GRADE_INSUFFICIENT)"
                .to_owned(),
        }
    } else {
        DoctorCheckItem {
            id: "cap_negative_gate".to_owned(),
            core_gate: true,
            status: "FAIL".to_owned(),
            summary: format!(
                "unexpected CAP disposition={:?} code={:?}",
                audit.disposition, audit.error_code
            ),
        }
    }
}

fn check_audit_chain_integrity(state_dir: &Path) -> DoctorCheckItem {
    let state_file = state_dir.join("engine_state.json");
    if state_file.is_file() {
        if let Ok(raw) = fs::read_to_string(&state_file) {
            if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                if let Some(arr) = val.get("audit_entries") {
                    if let Ok(entries) =
                        serde_json::from_value::<Vec<HashChainedAuditEntry>>(arr.clone())
                    {
                        if !entries.is_empty() {
                            let valid = verify_audit_chain(&entries);
                            let mut tampered = entries.clone();
                            "tampered_stage".clone_into(&mut tampered[0].stage);
                            let tamper_caught = !verify_audit_chain(&tampered);
                            return if valid && tamper_caught {
                                DoctorCheckItem {
                                    id: "audit_chain_integrity".to_owned(),
                                    core_gate: true,
                                    status: "PASS".to_owned(),
                                    summary: format!(
                                        "Verified existing SHA-256 hash chain ({} entries) + tamper rejection",
                                        entries.len()
                                    ),
                                }
                            } else {
                                DoctorCheckItem {
                                    id: "audit_chain_integrity".to_owned(),
                                    core_gate: true,
                                    status: "FAIL".to_owned(),
                                    summary: "Existing engine_state.json SHA-256 audit chain failed verification"
                                        .to_owned(),
                                }
                            };
                        }
                    }
                }
            }
        }
    }

    let mut builder = AuditChainBuilder::new();
    let ok_build = builder
        .append("doctor.genesis", json!({"step": 1}))
        .and_then(|_| builder.append("doctor.faying_gate", json!({"step": 2})))
        .and_then(|_| builder.append("doctor.cap_gate", json!({"step": 3})))
        .is_ok();
    let entries = builder.into_entries();
    let valid = ok_build && entries.len() == 3 && verify_audit_chain(&entries);
    let mut tampered = entries;
    if let Some(first) = tampered.first_mut() {
        "corrupted".clone_into(&mut first.stage);
    }
    let tamper_caught = !verify_audit_chain(&tampered);

    if valid && tamper_caught {
        DoctorCheckItem {
            id: "audit_chain_integrity".to_owned(),
            core_gate: true,
            status: "PASS".to_owned(),
            summary: "Verified 3-entry ephemeral SHA-256 hash chain + tamper rejection".to_owned(),
        }
    } else {
        DoctorCheckItem {
            id: "audit_chain_integrity".to_owned(),
            core_gate: true,
            status: "FAIL".to_owned(),
            summary: "Ephemeral SHA-256 audit chain self-test failed".to_owned(),
        }
    }
}

fn check_companion_endpoints(
    daemon_self_online: bool,
) -> (DoctorCheckItem, CompanionEndpointStatus) {
    let faygerd = if daemon_self_online || probe_loopback_port(7979) {
        "online".to_owned()
    } else {
        "not_running".to_owned()
    };
    let ego = if probe_loopback_port(7980) {
        "online".to_owned()
    } else {
        "not_running".to_owned()
    };
    let gmchain = if probe_loopback_port(8546) {
        "online".to_owned()
    } else {
        "not_running".to_owned()
    };
    let summary = format!("faygerd={faygerd}, ego={ego}, gmchain={gmchain} (optional)");
    (
        DoctorCheckItem {
            id: "companion_endpoints".to_owned(),
            core_gate: false,
            status: "INFO".to_owned(),
            summary,
        },
        CompanionEndpointStatus {
            faygerd,
            ego,
            gmchain,
        },
    )
}

fn probe_loopback_port(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok()
}

fn default_user_root_dir() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return Some(PathBuf::from(home));
        }
    }
    if let Ok(up) = std::env::var("USERPROFILE") {
        if !up.trim().is_empty() {
            return Some(PathBuf::from(up));
        }
    }
    None
}

/// 纵深防御：清除一切可能从环境变量混入的 `$HOME`、用户名或主机名字符串。
fn scrub_env_pii(input: &str) -> String {
    let mut out = input.to_owned();
    for var_name in ["HOME", "USERPROFILE"] {
        if let Ok(val) = std::env::var(var_name) {
            let trimmed = val.trim().trim_end_matches(['/', '\\']);
            if trimmed.len() >= 3 && out.contains(trimmed) {
                out = out.replace(trimmed, "$HOME");
            }
        }
    }
    for var_name in ["USER", "LOGNAME", "USERNAME", "HOSTNAME", "COMPUTERNAME"] {
        if let Ok(val) = std::env::var(var_name) {
            let trimmed = val.trim();
            if trimmed.len() >= 3 && out.contains(trimmed) {
                out = out.replace(trimmed, "<redacted>");
            }
        }
    }
    out
}

/// 纯 Rust 无外部依赖将 Unix 秒转换为 `YYYY-MM-DD`（Howard Hinnant civil-from-days 算法）。
#[must_use]
#[allow(clippy::cast_possible_wrap)]
pub fn format_utc_date_yyyy_mm_dd(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    format!("{year:04}-{m:02}-{d:02}")
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{format_utc_date_yyyy_mm_dd, run_doctor, DOCTOR_SCHEMA_VERSION};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn utc_date_conversion_is_exact() {
        assert_eq!(format_utc_date_yyyy_mm_dd(0), "1970-01-01");
        assert_eq!(format_utc_date_yyyy_mm_dd(1_795_564_800), "2026-11-25");
    }

    #[test]
    fn doctor_passes_core_gates_and_emits_zero_pii_receipt() {
        let seq = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("fayger-doctor-unit-{}-{seq}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        let report = run_doctor(&dir, false);
        assert_eq!(report.schema_version, DOCTOR_SCHEMA_VERSION);
        assert!(report.all_core_passed);
        assert_eq!(report.passed_core_count, 5);
        assert_eq!(report.total_core_count, 5);
        assert_eq!(report.checks.len(), 6);
        assert!(report
            .markdown_receipt
            .contains("### FayGer Cold-Start Verification Receipt (Zero-PII)"));
        assert!(report.markdown_receipt.contains("Core Gates: 5/5 PASS"));
        assert!(report.receipt_digest.starts_with("sha256:"));
        assert_eq!(report.receipt_digest.len(), 7 + 16);

        let full_json = serde_json::to_string(&report).expect("serialize doctor report");
        if let Ok(home) = std::env::var("HOME") {
            let trimmed = home.trim();
            if trimmed.len() >= 3 {
                assert!(
                    !report.markdown_receipt.contains(trimmed),
                    "markdown_receipt must never leak $HOME"
                );
                assert!(
                    !full_json.contains(trimmed),
                    "doctor JSON must never leak $HOME"
                );
            }
        }
        if let Ok(user) = std::env::var("USER") {
            let trimmed = user.trim();
            if trimmed.len() >= 3 {
                assert!(
                    !report.markdown_receipt.contains(trimmed),
                    "markdown_receipt must never leak USER"
                );
                assert!(
                    !full_json.contains(trimmed),
                    "doctor JSON must never leak USER"
                );
            }
        }

        let _ = fs::remove_dir_all(&dir);
    }
}
