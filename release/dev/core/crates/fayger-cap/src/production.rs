//! Production CAP 1.0 `D-1..D-7` evaluation, Ed25519 driver manifest trust verification,
//! canonical FayID checking, and default-closed controlled real execution gating.

use crate::{AccessMode, AuthorizationSource, Criticality};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};

/// Schema identifier for machine-readable production CAP evaluation records.
pub const PRODUCTION_EVALUATION_SCHEMA_VERSION: &str = "fayger-cap-production-evaluation/1.0";

/// Faying-Protocol session grade (`G0` Human Prime override through `G4` minimal delegation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FayingGrade {
    /// Human Prime immediate takeover (`G0`).
    G0,
    /// Highest autonomous / co-present authority (`G1`).
    G1,
    /// Standard supervised authority (`G2`).
    G2,
    /// Restricted routine/sensitive authority (`G3`).
    G3,
    /// Minimal read/execute authority (`G4`).
    G4,
}

/// Minimum Faying grade required by a `CapGrant`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GrantMinGrade {
    /// Requires `G1`.
    G1,
    /// Requires `G1` or `G2`.
    G2,
    /// Requires `G1`, `G2`, or `G3`.
    G3,
    /// Allows `G1`..=`G4`.
    G4,
}

impl GrantMinGrade {
    const fn strength(self) -> u8 {
        match self {
            Self::G1 => 4,
            Self::G2 => 3,
            Self::G3 => 2,
            Self::G4 => 1,
        }
    }
}

impl FayingGrade {
    const fn non_g0_strength(self) -> Option<u8> {
        match self {
            Self::G0 => None,
            Self::G1 => Some(4),
            Self::G2 => Some(3),
            Self::G3 => Some(2),
            Self::G4 => Some(1),
        }
    }
}

/// Consequence irreversibility axis defined by CAP ADR `D-7`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsequenceIrreversibility {
    /// Fully reversible state or read-only observation.
    Reversible,
    /// Bounded compensation action exists.
    BoundedCompensation,
    /// External or physical irreversible effect; floor raised to at least `High`.
    IrreversibleConsequence,
    /// Irreversible data destruction or hardware bricking risk; floor locked to `Critical`.
    IrreversibleDataOrHardware,
}

/// Operation domain classification (`D-7c` excludes `FundTransfer`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainCategory {
    /// Physical or virtualized device control.
    DeviceControl,
    /// Host operating-system resource.
    OsResource,
    /// External communication channel.
    Communication,
    /// Persistent storage medium.
    Storage,
    /// Device or controller firmware.
    Firmware,
    /// Financial fund transfer — unconditionally excluded from CAP 1.0 scope.
    FundTransfer,
}

/// Faying Mandate binding model (`D-1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MandateBindingModel {
    /// Cryptographic session/mandate-derived proof plus online freshness check (Model A+B).
    #[serde(rename = "model_a_plus_b")]
    ModelAPlusB,
    /// Mandate reference with online freshness/revocation verification (Model B).
    #[serde(rename = "model_b_online_ref")]
    ModelBOnlineRef,
    /// Standalone offline cached credential — forbidden for `sensitive`, `high`, and `critical`.
    #[serde(rename = "model_c_offline_cache")]
    ModelCOfflineCache,
}

/// Canonical CAP 1.0 wire error codes and companion numeric diagnostic codes (`D-8`, `D-9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CapErrorCode {
    /// Weak or offline-only Mandate binding on elevated operation (`2001`).
    #[serde(rename = "E_WEAK_MANDATE_BINDING")]
    WeakMandateBinding,
    /// Session grade or mode insufficient for effective criticality (`2002`).
    #[serde(rename = "E_GRADE_INSUFFICIENT")]
    GradeInsufficient,
    /// Human Prime `G0` emergency takeover active (`2003`).
    #[serde(rename = "E_G0_EMERGENCY_OVERRIDE")]
    G0EmergencyOverride,
    /// Resource or operation absent from verified manifest (`2004`).
    #[serde(rename = "E_UNKNOWN_OPERATION")]
    UnknownOperation,
    /// Manifest failed Ed25519 signature or trust anchor verification (`2005`).
    #[serde(rename = "E_UNTRUSTED_MANIFEST")]
    UntrustedManifest,
    /// Missing, expired, unattested, or parameter-mismatched Human Prime confirmation (`2006`).
    #[serde(rename = "E_CONFIRMATION_REQUIRED")]
    ConfirmationRequired,
    /// Financial fund transfer outside CAP 1.0 scope (`2007`).
    #[serde(rename = "E_OUT_OF_SCOPE_FUND_TRANSFER")]
    OutOfScopeFundTransfer,
    /// Expired, revoked, stale, or mismatched Mandate / FayID (`2008`).
    #[serde(rename = "E_MANDATE_INVALID")]
    MandateInvalid,
    /// Unsupported CAP protocol version (`2098`).
    #[serde(rename = "E_PROTOCOL_VERSION_UNSUPPORTED")]
    ProtocolVersionUnsupported,
    /// Canonical protocol fallback error (`2099`).
    #[serde(rename = "E_PROTOCOL")]
    Protocol,
}

impl CapErrorCode {
    /// Companion numeric diagnostic code (`2001..=2099`).
    pub const fn diagnostic_code(self) -> u16 {
        match self {
            Self::WeakMandateBinding => 2001,
            Self::GradeInsufficient => 2002,
            Self::G0EmergencyOverride => 2003,
            Self::UnknownOperation => 2004,
            Self::UntrustedManifest => 2005,
            Self::ConfirmationRequired => 2006,
            Self::OutOfScopeFundTransfer => 2007,
            Self::MandateInvalid => 2008,
            Self::ProtocolVersionUnsupported => 2098,
            Self::Protocol => 2099,
        }
    }
}

/// Safe-state descriptor inside a signed production driver capability manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionSafeStateDescriptor {
    /// Stable safe-state identifier (e.g. `hover_and_rtl`).
    pub safe_state_id: String,
    /// Optional human-readable description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Operation descriptor inside a signed production driver capability manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionOperationDescriptor {
    /// Stable operation identifier.
    pub operation_id: String,
    /// Required CAP access mode.
    pub mode: AccessMode,
    /// Driver-declared baseline criticality floor.
    pub criticality: Criticality,
    /// Consequence irreversibility classification (`D-7`).
    pub consequence_irreversibility: ConsequenceIrreversibility,
    /// Optional operation domain classification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_category: Option<DomainCategory>,
}

/// Controllable resource descriptor inside a signed production driver capability manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionResourceDescriptor {
    /// Stable resource identifier.
    pub resource_id: String,
    /// Optional pre-declared safe states ordered by preference.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub safe_states: Vec<ProductionSafeStateDescriptor>,
    /// Declared operations on this resource.
    pub operations: Vec<ProductionOperationDescriptor>,
}

/// Ed25519-signed CAP 1.0 `DriverCapabilityManifest` (`D-5`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedDriverManifest {
    /// Unique manifest identifier.
    pub manifest_id: String,
    /// Driver package or module identifier.
    pub driver_id: String,
    /// Driver manifest version string.
    pub version: String,
    /// RFC 3339 issuance timestamp.
    pub issued_at: String,
    /// Identifier of the signing trust anchor key.
    pub signer_key_id: String,
    /// Declared resources and operations.
    pub resources: Vec<ProductionResourceDescriptor>,
    /// Lowercase/uppercase 64-byte hex Ed25519 signature over canonical unsigned JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_ed25519: Option<String>,
}

/// Store of trusted Ed25519 public keys for verifying `SignedDriverManifest`s (`D-5`).
#[derive(Debug, Clone, Default)]
pub struct TrustAnchorStore {
    keys: HashMap<String, VerifyingKey>,
}

impl TrustAnchorStore {
    /// Creates an empty trust anchor store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a 32-byte raw Ed25519 public key encoded as 64 hex characters.
    pub fn insert_ed25519_hex(
        &mut self,
        key_id: impl Into<String>,
        public_key_hex: &str,
    ) -> Result<(), FaygerError> {
        let bytes = decode_hex_exact::<32>(public_key_hex).ok_or_else(|| {
            FaygerError::new(
                ErrorCode::FmValidationFailed,
                Layer::Assembler,
                "Invalid 32-byte Ed25519 public key hex for CAP trust anchor",
            )
            .with_context("field", ContextValue::string("public_key_hex"))
        })?;
        let key = VerifyingKey::from_bytes(&bytes).map_err(|e| {
            FaygerError::new(
                ErrorCode::FmValidationFailed,
                Layer::Assembler,
                "Rejected malformed Ed25519 public key point",
            )
            .with_context("detail", ContextValue::string(e.to_string()))
        })?;
        self.keys.insert(key_id.into(), key);
        Ok(())
    }

    /// Verifies `manifest`'s Ed25519 signature against the registered `signer_key_id`.
    pub fn verify_manifest(&self, manifest: &SignedDriverManifest) -> bool {
        let Some(sig_hex) = manifest.signature_ed25519.as_deref() else {
            return false;
        };
        let Some(sig_bytes) = decode_hex_exact::<64>(sig_hex) else {
            return false;
        };
        let Some(verifying_key) = self.keys.get(&manifest.signer_key_id) else {
            return false;
        };
        let mut unsigned = manifest.clone();
        unsigned.signature_ed25519 = None;
        let Ok(val) = serde_json::to_value(&unsigned) else {
            return false;
        };
        let payload = canonical_json(&val);
        let signature = Signature::from_bytes(&sig_bytes);
        verifying_key.verify(payload.as_bytes(), &signature).is_ok()
    }
}

/// Production Faying Mandate evidence bound to a CAP grant (`D-1`, `D-2`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionMandateEvidence {
    /// Unique Faying Mandate identifier.
    pub mandate_id: String,
    /// Authorized canonical Fay identifier (`ifay_<base32>` or `cofay_<base32>`).
    pub fay_id: String,
    /// Target resource identifier covered by the Mandate.
    pub resource_id: String,
    /// Permitted access modes under this Mandate.
    pub allowed_modes: Vec<AccessMode>,
    /// Current session Faying grade (`G0..=G4`).
    pub session_grade: FayingGrade,
    /// Binding model used to present the Mandate.
    pub binding_model: MandateBindingModel,
    /// Whether online freshness check succeeded.
    pub online_fresh: bool,
    /// Whether the Mandate has been revoked.
    pub revoked: bool,
    /// Epoch milliseconds when revocation status was last verified.
    pub revocation_checked_at_ms: u64,
    /// Maximum allowed staleness window in milliseconds for revocation freshness.
    pub max_revocation_staleness_ms: u64,
    /// Mandate expiration timestamp in epoch milliseconds.
    pub expires_at_epoch_ms: u64,
    /// Whether cryptographic Model A session-derived binding proof passed.
    pub model_a_derived_proof_valid: bool,
}

/// Per-resource CAP grant carrying `min_grade` and `ProductionMandateEvidence` (`D-4`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapGrant {
    /// Grant identifier.
    pub grant_id: String,
    /// Target resource identifier.
    pub resource_id: String,
    /// Permitted access modes on the grant.
    pub allowed_modes: Vec<AccessMode>,
    /// Minimum required Faying grade (`G1..=G4`).
    pub min_grade: GrantMinGrade,
    /// Optional monotonic criticality override on the grant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub criticality_override: Option<Criticality>,
    /// Bound Faying Mandate evidence.
    pub mandate_binding: ProductionMandateEvidence,
}

/// Parameter-bound explicit Human Prime confirmation proof (`D-6`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanConfirmationProof {
    /// Unique confirmation record identifier.
    pub confirmation_id: String,
    /// Canonical Human Prime identifier (`hid_<base32>`).
    pub human_prime_id: String,
    /// Lowercase 64-hex SHA-256 of canonicalized request `params`.
    pub confirmed_params_sha256: String,
    /// Whether the Host terminal attested full consequence disclosure to Human Prime.
    pub terminal_attested_disclosure: bool,
    /// Confirmation expiration timestamp in epoch milliseconds.
    pub expires_at_epoch_ms: u64,
}

/// Complete production CAP evaluation input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionEvaluationRequest {
    /// CAP protocol version (`"1.0"`).
    pub protocol_version: String,
    /// Request identifier.
    pub request_id: String,
    /// Acting Fay canonical identifier (`ifay_...` or `cofay_...`).
    pub fay_id: String,
    /// Target resource identifier.
    pub resource_id: String,
    /// Target operation identifier.
    pub operation_id: String,
    /// Requested access mode.
    pub requested_mode: AccessMode,
    /// Structured operation parameters (bound via SHA-256 for `high`/`critical`).
    pub params: Value,
    /// Optional caller-supplied domain category check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_category: Option<DomainCategory>,
    /// Optional caller criticality override (monotonically clamped to floor).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller_criticality_override: Option<Criticality>,
    /// Explicit Human Prime `G0` takeover signal.
    pub g0_override: bool,
    /// Current evaluation wall-clock timestamp in epoch milliseconds.
    pub now_epoch_ms: u64,
    /// Signed driver capability manifest.
    pub manifest: SignedDriverManifest,
    /// CAP grant and Mandate binding evidence.
    pub grant: CapGrant,
    /// Optional explicit Human Prime confirmation proof.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_confirmation: Option<HumanConfirmationProof>,
}

/// Policy gate controlling whether verified commands may touch real hardware (`OQ-5`).
///
/// Defaults to closed (`enabled = false`), keeping allowed commands in simulation-only mode.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealExecutionGateConfig {
    enabled: bool,
    audit_approval_ticket: Option<String>,
}

impl RealExecutionGateConfig {
    /// Explicitly enables controlled real execution with a non-empty safety audit ticket.
    pub fn enable_with_ticket(ticket: impl Into<String>) -> Result<Self, FaygerError> {
        let ticket = ticket.into();
        if ticket.trim().is_empty() {
            return Err(FaygerError::new(
                ErrorCode::FmValidationFailed,
                Layer::Assembler,
                "Real hardware execution requires a non-empty audit approval ticket",
            )
            .with_context("field", ContextValue::string("audit_approval_ticket")));
        }
        Ok(Self {
            enabled: true,
            audit_approval_ticket: Some(ticket),
        })
    }

    /// Returns whether real execution has been explicitly enabled.
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the recorded approval ticket, if enabled.
    pub fn approval_ticket(&self) -> Option<&str> {
        self.audit_approval_ticket.as_deref()
    }
}

/// Safe-state directive action emitted upon Human Prime `G0` takeover (`D-3`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum G0SafeStateAction {
    /// Transition resource into its primary pre-declared safe state.
    EnterSafeState,
    /// Immediately halt Fay control and alert Human Prime when no safe state is declared.
    TerminateAndAlert,
}

/// Non-blocking safe-state directive emitted during `G0` emergency override (`D-3`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct G0SafeStateDirective {
    /// Required driver/terminal safe-state action.
    pub action: G0SafeStateAction,
    /// Selected safe-state id when `action == EnterSafeState`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_state_id: Option<String>,
    /// Always `false`: safe-state transitions never block Human Prime takeover.
    pub blocks_human_takeover: bool,
}

/// Persisted confirmation evidence summary written into `ProductionAuditRecord` (`D-6`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmationAuditSummary {
    /// Verified confirmation identifier.
    pub confirmation_id: String,
    /// Verified canonical Human Prime identifier (`hid_...`).
    pub human_prime_id: String,
    /// Verified SHA-256 digest of request parameters.
    pub confirmed_params_sha256: String,
}

/// Final disposition of a production CAP evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionDisposition {
    /// Verified under production CAP rules, but real execution gate is closed -> simulation only.
    AllowSimulationOnly,
    /// Verified under production CAP rules AND real execution gate + safe-state fallback satisfied.
    AllowControlledExecution,
    /// Denied by default-deny safety pipeline.
    Deny,
}

/// Machine-readable audit record emitted by [`evaluate_production_command`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionAuditRecord {
    /// Audit schema version.
    pub schema_version: String,
    /// Request identifier.
    pub request_id: String,
    /// Authorization source (`ProductionFayingMandate`, distinct from `DemoFixture`).
    pub authorization_source: AuthorizationSource,
    /// `true` iff all CAP D-1..D-7 and FayID checks succeeded.
    pub production_verified: bool,
    /// Final execution disposition.
    pub disposition: ProductionDisposition,
    /// Effective criticality after irreversibility floor and monotonic overrides.
    pub effective_criticality: Criticality,
    /// Effective Faying grade when allowed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_grade: Option<FayingGrade>,
    /// `true` if caller or grant attempted to lower criticality below floor.
    pub escalation_clamped: bool,
    /// `true` if Mandate was flagged revoked or exceeded revocation staleness window.
    pub mandate_revoked: bool,
    /// Normative CAP wire error code when denied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<CapErrorCode>,
    /// Companion numeric diagnostic code (`2001..=2099`) when denied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic_code: Option<u16>,
    /// Human-readable audit reason.
    pub reason: String,
    /// Recorded Human Prime confirmation summary when verified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmation_audit: Option<ConfirmationAuditSummary>,
    /// Recorded real execution approval ticket when `AllowControlledExecution`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub real_execution_ticket: Option<String>,
    /// Safe-state directive emitted on `G0` emergency takeover.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_state_directive: Option<G0SafeStateDirective>,
}

/// Deterministic RFC 8785-compatible canonical JSON serializer matching CAP TypeScript `canonicalJson`.
pub fn canonical_json(value: &Value) -> String {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned())
        }
        Value::Array(items) => {
            let joined: Vec<String> = items.iter().map(canonical_json).collect();
            format!("[{}]", joined.join(","))
        }
        Value::Object(map) => {
            let sorted: BTreeMap<&String, &Value> = map.iter().collect();
            let parts: Vec<String> = sorted
                .into_iter()
                .map(|(k, v)| {
                    let key_json = serde_json::to_string(k).unwrap_or_else(|_| "\"\"".to_owned());
                    format!("{key_json}:{}", canonical_json(v))
                })
                .collect();
            format!("{{{}}}", parts.join(","))
        }
    }
}

/// Computes lowercase 64-character SHA-256 hex digest of canonicalized request `params` (`D-6`).
pub fn compute_params_sha256(params: &Value) -> String {
    let canon = canonical_json(params);
    let digest = Sha256::digest(canon.as_bytes());
    encode_hex_lower(&digest)
}

/// Evaluates a production CAP 1.0 command request against `D-1..D-7`, FayID canonical format,
/// and the default-closed `RealExecutionGateConfig`.
pub fn evaluate_production_command(
    req: &ProductionEvaluationRequest,
    trust_anchors: &TrustAnchorStore,
    execution_gate: &RealExecutionGateConfig,
) -> ProductionAuditRecord {
    if req.protocol_version != "1.0" {
        return deny_record(
            &req.request_id,
            Criticality::Critical,
            false,
            false,
            CapErrorCode::ProtocolVersionUnsupported,
            "Unsupported CAP protocol version",
            None,
        );
    }

    // Step 1 (D-7c): Financial fund transfers are unconditionally excluded from CAP 1.0 scope.
    if req.domain_category == Some(DomainCategory::FundTransfer) {
        return deny_record(
            &req.request_id,
            Criticality::Critical,
            false,
            false,
            CapErrorCode::OutOfScopeFundTransfer,
            "Financial fund_transfer operations are excluded from CAP 1.0 scope",
            None,
        );
    }

    let (resource, operation, effective_criticality, escalation_clamped) =
        match resolve_manifest_and_g0(req, trust_anchors) {
            Ok(resolved) => resolved,
            Err(record) => return *record,
        };

    // Step 5 & 6 (D-1, D-2, OQ-4): Validate canonical FayID, revocation timeliness, Mandate & grade matrix.
    let session_grade = req.grant.mandate_binding.session_grade;
    if let Err(record) = verify_mandate_and_grade(
        req,
        operation.mode,
        effective_criticality,
        escalation_clamped,
    ) {
        return *record;
    }

    // Step 7 (D-6): Parameter-bound explicit Human Prime confirmation gate (including under G1).
    let confirmation_audit =
        match verify_confirmation_if_needed(req, effective_criticality, escalation_clamped) {
            Ok(summary) => summary,
            Err(record) => return *record,
        };

    // Step 8 (OQ-5): Controlled real execution gate — default closed, requires safe-state fallback for mutating ops.
    let has_safe_state_fallback =
        req.requested_mode == AccessMode::Read || !resource.safe_states.is_empty();
    let (disposition, reason, real_execution_ticket) =
        if execution_gate.is_enabled() && has_safe_state_fallback {
            (
                ProductionDisposition::AllowControlledExecution,
                "production_verified_controlled_execution".to_owned(),
                execution_gate.approval_ticket().map(str::to_owned),
            )
        } else {
            (
                ProductionDisposition::AllowSimulationOnly,
                "production_verified_simulation_only".to_owned(),
                None,
            )
        };

    ProductionAuditRecord {
        schema_version: PRODUCTION_EVALUATION_SCHEMA_VERSION.to_owned(),
        request_id: req.request_id.clone(),
        authorization_source: AuthorizationSource::ProductionFayingMandate,
        production_verified: true,
        disposition,
        effective_criticality,
        effective_grade: Some(session_grade),
        escalation_clamped,
        mandate_revoked: false,
        error_code: None,
        diagnostic_code: None,
        reason,
        confirmation_audit,
        real_execution_ticket,
        safe_state_directive: None,
    }
}

fn resolve_manifest_and_g0<'a>(
    req: &'a ProductionEvaluationRequest,
    trust_anchors: &TrustAnchorStore,
) -> Result<
    (
        &'a ProductionResourceDescriptor,
        &'a ProductionOperationDescriptor,
        Criticality,
        bool,
    ),
    Box<ProductionAuditRecord>,
> {
    if !trust_anchors.verify_manifest(&req.manifest) {
        return Err(Box::new(deny_record(
            &req.request_id,
            Criticality::Critical,
            false,
            false,
            CapErrorCode::UntrustedManifest,
            "DriverCapabilityManifest failed Ed25519 signature verification",
            None,
        )));
    }
    let resource = req
        .manifest
        .resources
        .iter()
        .find(|r| r.resource_id == req.resource_id);
    let operation = resource.and_then(|r| {
        r.operations
            .iter()
            .find(|op| op.operation_id == req.operation_id)
    });
    let (Some(resource), Some(operation)) = (resource, operation) else {
        return Err(Box::new(deny_record(
            &req.request_id,
            Criticality::Critical,
            false,
            false,
            CapErrorCode::UnknownOperation,
            "Undeclared resource or operation defaults to critical DENY",
            None,
        )));
    };
    if operation.domain_category == Some(DomainCategory::FundTransfer) {
        return Err(Box::new(deny_record(
            &req.request_id,
            Criticality::Critical,
            false,
            false,
            CapErrorCode::OutOfScopeFundTransfer,
            "Manifest-declared fund_transfer operation is excluded from CAP 1.0 scope",
            None,
        )));
    }
    let floor_criticality =
        apply_irreversibility_floor(operation.criticality, operation.consequence_irreversibility);
    let mut effective_criticality = floor_criticality;
    let mut escalation_clamped = false;
    for candidate in [
        req.grant.criticality_override,
        req.caller_criticality_override,
    ]
    .into_iter()
    .flatten()
    {
        if candidate < floor_criticality {
            escalation_clamped = true;
        } else {
            effective_criticality = effective_criticality.raised_to(candidate);
        }
    }
    if req.g0_override || req.grant.mandate_binding.session_grade == FayingGrade::G0 {
        let directive = resource.safe_states.first().map_or_else(
            || G0SafeStateDirective {
                action: G0SafeStateAction::TerminateAndAlert,
                safe_state_id: None,
                blocks_human_takeover: false,
            },
            |primary| G0SafeStateDirective {
                action: G0SafeStateAction::EnterSafeState,
                safe_state_id: Some(primary.safe_state_id.clone()),
                blocks_human_takeover: false,
            },
        );
        return Err(Box::new(deny_record(
            &req.request_id,
            effective_criticality,
            escalation_clamped,
            false,
            CapErrorCode::G0EmergencyOverride,
            "Human Prime G0 emergency takeover active",
            Some(directive),
        )));
    }
    Ok((
        resource,
        operation,
        effective_criticality,
        escalation_clamped,
    ))
}

fn verify_mandate_and_grade(
    req: &ProductionEvaluationRequest,
    operation_mode: AccessMode,
    effective_criticality: Criticality,
    escalation_clamped: bool,
) -> Result<(), Box<ProductionAuditRecord>> {
    let mandate = &req.grant.mandate_binding;
    let revocation_stale = req
        .now_epoch_ms
        .saturating_sub(mandate.revocation_checked_at_ms)
        > mandate.max_revocation_staleness_ms;
    if mandate.revoked || revocation_stale {
        return Err(Box::new(deny_record(
            &req.request_id,
            effective_criticality,
            escalation_clamped,
            true,
            CapErrorCode::MandateInvalid,
            "Mandate revoked or exceeded revocation freshness window",
            None,
        )));
    }
    if !is_canonical_fay_actor_id(&req.fay_id)
        || mandate.expires_at_epoch_ms <= req.now_epoch_ms
        || mandate.fay_id != req.fay_id
        || mandate.resource_id != req.resource_id
        || req.grant.resource_id != req.resource_id
        || operation_mode != req.requested_mode
        || !req.grant.allowed_modes.contains(&req.requested_mode)
        || !mandate.allowed_modes.contains(&req.requested_mode)
    {
        return Err(Box::new(deny_record(
            &req.request_id,
            effective_criticality,
            escalation_clamped,
            false,
            CapErrorCode::MandateInvalid,
            "Mandate or FayID invalid, expired, or scope-mismatched",
            None,
        )));
    }
    verify_binding_model(
        &req.request_id,
        mandate,
        effective_criticality,
        escalation_clamped,
    )?;
    let session_grade = mandate.session_grade;
    let Some(grade_strength) = session_grade.non_g0_strength() else {
        return Err(Box::new(deny_record(
            &req.request_id,
            effective_criticality,
            escalation_clamped,
            false,
            CapErrorCode::G0EmergencyOverride,
            "G0 handled",
            None,
        )));
    };
    if grade_strength < req.grant.min_grade.strength()
        || !is_grade_matrix_allowed(session_grade, req.requested_mode, effective_criticality)
    {
        return Err(Box::new(deny_record(
            &req.request_id,
            effective_criticality,
            escalation_clamped,
            false,
            CapErrorCode::GradeInsufficient,
            "Session Faying grade or mode insufficient for operation criticality",
            None,
        )));
    }
    Ok(())
}

fn verify_binding_model(
    request_id: &str,
    mandate: &ProductionMandateEvidence,
    effective_criticality: Criticality,
    escalation_clamped: bool,
) -> Result<(), Box<ProductionAuditRecord>> {
    if matches!(
        effective_criticality,
        Criticality::High | Criticality::Critical
    ) {
        if mandate.binding_model != MandateBindingModel::ModelAPlusB
            || !mandate.online_fresh
            || !mandate.model_a_derived_proof_valid
        {
            return Err(Box::new(deny_record(
                request_id,
                effective_criticality,
                escalation_clamped,
                false,
                CapErrorCode::WeakMandateBinding,
                "High and critical operations require Model A+B session proof and online freshness",
                None,
            )));
        }
    } else if matches!(
        mandate.binding_model,
        MandateBindingModel::ModelBOnlineRef | MandateBindingModel::ModelAPlusB
    ) {
        if !mandate.online_fresh {
            return Err(Box::new(deny_record(
                request_id,
                effective_criticality,
                escalation_clamped,
                false,
                CapErrorCode::MandateInvalid,
                "Online Mandate binding model requires online_fresh == true",
                None,
            )));
        }
    } else if effective_criticality == Criticality::Sensitive
        && mandate.binding_model == MandateBindingModel::ModelCOfflineCache
    {
        return Err(Box::new(deny_record(
            request_id,
            effective_criticality,
            escalation_clamped,
            false,
            CapErrorCode::WeakMandateBinding,
            "Sensitive operations forbid standalone offline cached Model C credentials",
            None,
        )));
    }
    Ok(())
}

fn verify_confirmation_if_needed(
    req: &ProductionEvaluationRequest,
    effective_criticality: Criticality,
    escalation_clamped: bool,
) -> Result<Option<ConfirmationAuditSummary>, Box<ProductionAuditRecord>> {
    if !matches!(
        effective_criticality,
        Criticality::High | Criticality::Critical
    ) {
        return Ok(None);
    }
    let expected_sha = compute_params_sha256(&req.params);
    let Some(conf) = req.human_confirmation.as_ref() else {
        return Err(Box::new(deny_record(
            &req.request_id,
            effective_criticality,
            escalation_clamped,
            false,
            CapErrorCode::ConfirmationRequired,
            "Missing explicit Human Prime confirmation for high/critical operation",
            None,
        )));
    };
    if conf.confirmation_id.trim().is_empty()
        || !is_canonical_prefixed_base32(&conf.human_prime_id, "hid_")
        || conf.expires_at_epoch_ms <= req.now_epoch_ms
        || !conf.terminal_attested_disclosure
        || conf.confirmed_params_sha256 != expected_sha
    {
        return Err(Box::new(deny_record(
            &req.request_id,
            effective_criticality,
            escalation_clamped,
            false,
            CapErrorCode::ConfirmationRequired,
            "Invalid, expired, unattested, or parameter-mismatched Human Prime confirmation",
            None,
        )));
    }
    Ok(Some(ConfirmationAuditSummary {
        confirmation_id: conf.confirmation_id.clone(),
        human_prime_id: conf.human_prime_id.clone(),
        confirmed_params_sha256: conf.confirmed_params_sha256.clone(),
    }))
}

const fn apply_irreversibility_floor(
    base: Criticality,
    irreversibility: ConsequenceIrreversibility,
) -> Criticality {
    match irreversibility {
        ConsequenceIrreversibility::IrreversibleDataOrHardware => Criticality::Critical,
        ConsequenceIrreversibility::IrreversibleConsequence => base.raised_to(Criticality::High),
        ConsequenceIrreversibility::Reversible
        | ConsequenceIrreversibility::BoundedCompensation => base,
    }
}

const fn is_grade_matrix_allowed(
    grade: FayingGrade,
    mode: AccessMode,
    criticality: Criticality,
) -> bool {
    match criticality {
        Criticality::Critical => matches!(grade, FayingGrade::G1),
        Criticality::High => matches!(grade, FayingGrade::G1 | FayingGrade::G2),
        Criticality::Sensitive => match grade {
            FayingGrade::G1 | FayingGrade::G2 => true,
            FayingGrade::G3 => !matches!(mode, AccessMode::Configure),
            FayingGrade::G0 | FayingGrade::G4 => false,
        },
        Criticality::Routine => match grade {
            FayingGrade::G1 | FayingGrade::G2 | FayingGrade::G3 => true,
            FayingGrade::G4 => matches!(mode, AccessMode::Read | AccessMode::Execute),
            FayingGrade::G0 => false,
        },
    }
}

fn is_canonical_fay_actor_id(id: &str) -> bool {
    is_canonical_prefixed_base32(id, "ifay_") || is_canonical_prefixed_base32(id, "cofay_")
}

fn is_canonical_prefixed_base32(id: &str, prefix: &str) -> bool {
    let Some(rest) = id.strip_prefix(prefix) else {
        return false;
    };
    rest.len() >= 16
        && rest
            .bytes()
            .all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b))
}

fn deny_record(
    request_id: &str,
    effective_criticality: Criticality,
    escalation_clamped: bool,
    mandate_revoked: bool,
    code: CapErrorCode,
    reason: &str,
    safe_state_directive: Option<G0SafeStateDirective>,
) -> ProductionAuditRecord {
    ProductionAuditRecord {
        schema_version: PRODUCTION_EVALUATION_SCHEMA_VERSION.to_owned(),
        request_id: request_id.to_owned(),
        authorization_source: AuthorizationSource::ProductionFayingMandate,
        production_verified: false,
        disposition: ProductionDisposition::Deny,
        effective_criticality,
        effective_grade: None,
        escalation_clamped,
        mandate_revoked,
        error_code: Some(code),
        diagnostic_code: Some(code.diagnostic_code()),
        reason: reason.to_owned(),
        confirmation_audit: None,
        real_execution_ticket: None,
        safe_state_directive,
    }
}

fn decode_hex_exact<const N: usize>(hex: &str) -> Option<[u8; N]> {
    if hex.len() != N * 2 {
        return None;
    }
    let mut out = [0u8; N];
    let bytes = hex.as_bytes();
    for (i, slot) in out.iter_mut().enumerate() {
        let hi = hex_nibble(bytes[i * 2])?;
        let lo = hex_nibble(bytes[i * 2 + 1])?;
        *slot = (hi << 4) | lo;
    }
    Some(out)
}

const fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn encode_hex_lower(bytes: &[u8]) -> String {
    const LUT: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(char::from(LUT[usize::from(b >> 4)]));
        out.push(char::from(LUT[usize::from(b & 0x0f)]));
    }
    out
}
