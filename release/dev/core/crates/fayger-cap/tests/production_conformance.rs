//! Cross-language conformance and property tests for production CAP D-1..D-7 evaluation.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use fayger_cap::{
    compute_params_sha256, evaluate_production_command, AccessMode, AuthorizationSource,
    CapErrorCode, CapGrant, ConsequenceIrreversibility, Criticality, DomainCategory, FayingGrade,
    G0SafeStateAction, GrantMinGrade, HumanConfirmationProof, MandateBindingModel,
    ProductionDisposition, ProductionEvaluationRequest, ProductionMandateEvidence,
    ProductionOperationDescriptor, ProductionResourceDescriptor, ProductionSafeStateDescriptor,
    RealExecutionGateConfig, SignedDriverManifest, TrustAnchorStore,
};
use proptest::prelude::*;
use serde_json::json;

const ANCHOR_KEY_ID: &str = "key_trust_anchor_01";
const ANCHOR_PUB_HEX: &str = "03a107bff3ce10be1d70dd18e74bc09967e4d6309ba50d5f1ddc8664125531b8";
const REF_MANIFEST_SIG_HEX: &str = "d3d0cc697776e732c6ec292551d1effa587102dc697ed99b61d65db2366e83dc00d3c1f528fa8e037915f006ae771e4c786b8f0495fc040cd1fc32d12f4d7d07";
const SHA_UNLOCK_BAY_ALPHA: &str =
    "0beb72d16aa5320effde0346e45543a268f24473f72140a4374ddfd8e6e8fb8e";
const SHA_FLASH_BANK_0: &str = "93d07a398b33b7f5f2098f7bf74b60298005cd08489289f5258d26ce066ac38e";
const VALID_IFAY_ID: &str = "ifay_aaaaaaaabbbbbbbbccccccccdddddddd";
const VALID_HID: &str = "hid_aaaaaaaabbbbbbbbccccccccdddddddd";
const NOW_MS: u64 = 1_800_000_000_000;

fn reference_anchors() -> TrustAnchorStore {
    let mut store = TrustAnchorStore::new();
    store
        .insert_ed25519_hex(ANCHOR_KEY_ID, ANCHOR_PUB_HEX)
        .expect("valid anchor");
    store
}

fn reference_manifest() -> SignedDriverManifest {
    SignedDriverManifest {
        manifest_id: "mf_drone_001".to_owned(),
        driver_id: "drv_quadrotor_v1".to_owned(),
        version: "1.0.0".to_owned(),
        issued_at: "2026-11-15T00:00:00Z".to_owned(),
        signer_key_id: ANCHOR_KEY_ID.to_owned(),
        resources: vec![
            ProductionResourceDescriptor {
                resource_id: "res_drone_fc".to_owned(),
                safe_states: vec![ProductionSafeStateDescriptor {
                    safe_state_id: "hover_and_rtl".to_owned(),
                    description: Some("Hover in place then return to launch".to_owned()),
                }],
                operations: vec![
                    ProductionOperationDescriptor {
                        operation_id: "read_telemetry".to_owned(),
                        mode: AccessMode::Read,
                        criticality: Criticality::Routine,
                        consequence_irreversibility: ConsequenceIrreversibility::Reversible,
                        domain_category: Some(DomainCategory::DeviceControl),
                    },
                    ProductionOperationDescriptor {
                        operation_id: "tune_pid".to_owned(),
                        mode: AccessMode::Configure,
                        criticality: Criticality::Sensitive,
                        consequence_irreversibility:
                            ConsequenceIrreversibility::BoundedCompensation,
                        domain_category: Some(DomainCategory::DeviceControl),
                    },
                    ProductionOperationDescriptor {
                        operation_id: "unlock_cargo_latch".to_owned(),
                        mode: AccessMode::Execute,
                        criticality: Criticality::Sensitive,
                        consequence_irreversibility:
                            ConsequenceIrreversibility::IrreversibleConsequence,
                        domain_category: Some(DomainCategory::DeviceControl),
                    },
                    ProductionOperationDescriptor {
                        operation_id: "flash_esc_firmware".to_owned(),
                        mode: AccessMode::Write,
                        criticality: Criticality::High,
                        consequence_irreversibility:
                            ConsequenceIrreversibility::IrreversibleDataOrHardware,
                        domain_category: Some(DomainCategory::Firmware),
                    },
                ],
            },
            ProductionResourceDescriptor {
                resource_id: "res_legacy_sensor".to_owned(),
                safe_states: vec![],
                operations: vec![ProductionOperationDescriptor {
                    operation_id: "read_temp".to_owned(),
                    mode: AccessMode::Read,
                    criticality: Criticality::Routine,
                    consequence_irreversibility: ConsequenceIrreversibility::Reversible,
                    domain_category: Some(DomainCategory::OsResource),
                }],
            },
        ],
        signature_ed25519: Some(REF_MANIFEST_SIG_HEX.to_owned()),
    }
}

fn base_mandate(
    resource_id: &str,
    mode: AccessMode,
    grade: FayingGrade,
    binding_model: MandateBindingModel,
) -> ProductionMandateEvidence {
    ProductionMandateEvidence {
        mandate_id: "mnd_prod_01".to_owned(),
        fay_id: VALID_IFAY_ID.to_owned(),
        resource_id: resource_id.to_owned(),
        allowed_modes: vec![mode],
        session_grade: grade,
        binding_model,
        online_fresh: binding_model != MandateBindingModel::ModelCOfflineCache,
        revoked: false,
        revocation_checked_at_ms: NOW_MS - 5_000,
        max_revocation_staleness_ms: 60_000,
        expires_at_epoch_ms: NOW_MS + 60_000,
        model_a_derived_proof_valid: binding_model == MandateBindingModel::ModelAPlusB,
    }
}

#[test]
fn cross_language_signature_and_routine_default_simulation_gate() {
    let req = ProductionEvaluationRequest {
        protocol_version: "1.0".to_owned(),
        request_id: "req_v01".to_owned(),
        fay_id: VALID_IFAY_ID.to_owned(),
        resource_id: "res_drone_fc".to_owned(),
        operation_id: "read_telemetry".to_owned(),
        requested_mode: AccessMode::Read,
        params: json!({}),
        domain_category: None,
        caller_criticality_override: None,
        g0_override: false,
        now_epoch_ms: NOW_MS,
        manifest: reference_manifest(),
        grant: CapGrant {
            grant_id: "grt_v01".to_owned(),
            resource_id: "res_drone_fc".to_owned(),
            allowed_modes: vec![AccessMode::Read],
            min_grade: GrantMinGrade::G3,
            criticality_override: None,
            mandate_binding: base_mandate(
                "res_drone_fc",
                AccessMode::Read,
                FayingGrade::G3,
                MandateBindingModel::ModelBOnlineRef,
            ),
        },
        human_confirmation: None,
    };

    let audit = evaluate_production_command(
        &req,
        &reference_anchors(),
        &RealExecutionGateConfig::default(),
    );
    assert_eq!(
        audit.authorization_source,
        AuthorizationSource::ProductionFayingMandate
    );
    assert!(audit.production_verified);
    assert_eq!(
        audit.disposition,
        ProductionDisposition::AllowSimulationOnly
    );
    assert_eq!(audit.effective_criticality, Criticality::Routine);
    assert!(audit.error_code.is_none());
}

#[test]
fn controlled_real_execution_requires_explicit_gate_and_safe_state_fallback() {
    let params = json!({
        "image_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "target_bank": 0
    });
    assert_eq!(compute_params_sha256(&params), SHA_FLASH_BANK_0);

    let req = ProductionEvaluationRequest {
        protocol_version: "1.0".to_owned(),
        request_id: "req_v07".to_owned(),
        fay_id: VALID_IFAY_ID.to_owned(),
        resource_id: "res_drone_fc".to_owned(),
        operation_id: "flash_esc_firmware".to_owned(),
        requested_mode: AccessMode::Write,
        params,
        domain_category: None,
        caller_criticality_override: Some(Criticality::Routine),
        g0_override: false,
        now_epoch_ms: NOW_MS,
        manifest: reference_manifest(),
        grant: CapGrant {
            grant_id: "grt_v07".to_owned(),
            resource_id: "res_drone_fc".to_owned(),
            allowed_modes: vec![AccessMode::Write],
            min_grade: GrantMinGrade::G1,
            criticality_override: None,
            mandate_binding: base_mandate(
                "res_drone_fc",
                AccessMode::Write,
                FayingGrade::G1,
                MandateBindingModel::ModelAPlusB,
            ),
        },
        human_confirmation: Some(HumanConfirmationProof {
            confirmation_id: "cnf_v07".to_owned(),
            human_prime_id: VALID_HID.to_owned(),
            confirmed_params_sha256: SHA_FLASH_BANK_0.to_owned(),
            terminal_attested_disclosure: true,
            expires_at_epoch_ms: NOW_MS + 30_000,
        }),
    };

    let gate = RealExecutionGateConfig::enable_with_ticket("SAFETY-APPROVAL-2026-01")
        .expect("valid ticket");
    let audit = evaluate_production_command(&req, &reference_anchors(), &gate);

    assert!(audit.production_verified);
    assert!(audit.escalation_clamped);
    assert_eq!(audit.effective_criticality, Criticality::Critical);
    assert_eq!(
        audit.disposition,
        ProductionDisposition::AllowControlledExecution
    );
    assert_eq!(
        audit
            .confirmation_audit
            .as_ref()
            .expect("confirmation recorded")
            .confirmation_id,
        "cnf_v07"
    );
}

#[test]
fn enforces_d1_to_d7_denial_matrix_and_fayid_canonical_format() {
    let anchors = reference_anchors();
    let gate = RealExecutionGateConfig::default();

    // 1. D-7c: fund_transfer out of scope
    let mut fund_req = ProductionEvaluationRequest {
        protocol_version: "1.0".to_owned(),
        request_id: "req_fund".to_owned(),
        fay_id: VALID_IFAY_ID.to_owned(),
        resource_id: "res_drone_fc".to_owned(),
        operation_id: "read_telemetry".to_owned(),
        requested_mode: AccessMode::Read,
        params: json!({"amount": 100}),
        domain_category: Some(DomainCategory::FundTransfer),
        caller_criticality_override: None,
        g0_override: false,
        now_epoch_ms: NOW_MS,
        manifest: reference_manifest(),
        grant: CapGrant {
            grant_id: "grt_fund".to_owned(),
            resource_id: "res_drone_fc".to_owned(),
            allowed_modes: vec![AccessMode::Read],
            min_grade: GrantMinGrade::G1,
            criticality_override: None,
            mandate_binding: base_mandate(
                "res_drone_fc",
                AccessMode::Read,
                FayingGrade::G1,
                MandateBindingModel::ModelAPlusB,
            ),
        },
        human_confirmation: None,
    };
    let fund_audit = evaluate_production_command(&fund_req, &anchors, &gate);
    assert_eq!(fund_audit.disposition, ProductionDisposition::Deny);
    assert_eq!(
        fund_audit.error_code,
        Some(CapErrorCode::OutOfScopeFundTransfer)
    );
    assert!(!fund_audit.production_verified);

    // 2. OQ-4: Non-canonical fay_id rejected
    fund_req.domain_category = None;
    fund_req.fay_id = "legacy-non-canonical-id".to_owned();
    let bad_id_audit = evaluate_production_command(&fund_req, &anchors, &gate);
    assert_eq!(bad_id_audit.disposition, ProductionDisposition::Deny);
    assert_eq!(bad_id_audit.error_code, Some(CapErrorCode::MandateInvalid));

    // 3. D-6: G1 high operation WITHOUT confirmation is denied
    let g1_unconfirmed = ProductionEvaluationRequest {
        protocol_version: "1.0".to_owned(),
        request_id: "req_v05".to_owned(),
        fay_id: VALID_IFAY_ID.to_owned(),
        resource_id: "res_drone_fc".to_owned(),
        operation_id: "unlock_cargo_latch".to_owned(),
        requested_mode: AccessMode::Execute,
        params: json!({"latch_id": "bay_alpha"}),
        domain_category: None,
        caller_criticality_override: None,
        g0_override: false,
        now_epoch_ms: NOW_MS,
        manifest: reference_manifest(),
        grant: CapGrant {
            grant_id: "grt_v05".to_owned(),
            resource_id: "res_drone_fc".to_owned(),
            allowed_modes: vec![AccessMode::Execute],
            min_grade: GrantMinGrade::G1,
            criticality_override: None,
            mandate_binding: base_mandate(
                "res_drone_fc",
                AccessMode::Execute,
                FayingGrade::G1,
                MandateBindingModel::ModelAPlusB,
            ),
        },
        human_confirmation: None,
    };
    let unconfirmed_audit = evaluate_production_command(&g1_unconfirmed, &anchors, &gate);
    assert_eq!(unconfirmed_audit.effective_criticality, Criticality::High);
    assert_eq!(
        unconfirmed_audit.error_code,
        Some(CapErrorCode::ConfirmationRequired)
    );
    assert_eq!(
        compute_params_sha256(&json!({"latch_id": "bay_alpha"})),
        SHA_UNLOCK_BAY_ALPHA
    );

    // 4. D-1/Revocation staleness: stale revocation check or revoked mandate is denied and audited
    let mut revoked_req = g1_unconfirmed;
    revoked_req.operation_id = "read_telemetry".to_owned();
    revoked_req.requested_mode = AccessMode::Read;
    revoked_req.grant.allowed_modes = vec![AccessMode::Read];
    revoked_req.grant.mandate_binding.allowed_modes = vec![AccessMode::Read];
    revoked_req.grant.mandate_binding.revoked = true;
    let revoked_audit = evaluate_production_command(&revoked_req, &anchors, &gate);
    assert_eq!(revoked_audit.disposition, ProductionDisposition::Deny);
    assert_eq!(revoked_audit.error_code, Some(CapErrorCode::MandateInvalid));
    assert!(revoked_audit.mandate_revoked);
}

#[test]
fn g0_emergency_override_emits_non_blocking_safe_state_or_terminate_directive() {
    let anchors = reference_anchors();
    let gate = RealExecutionGateConfig::default();

    let drone_g0 = ProductionEvaluationRequest {
        protocol_version: "1.0".to_owned(),
        request_id: "req_g0_1".to_owned(),
        fay_id: VALID_IFAY_ID.to_owned(),
        resource_id: "res_drone_fc".to_owned(),
        operation_id: "read_telemetry".to_owned(),
        requested_mode: AccessMode::Read,
        params: json!({}),
        domain_category: None,
        caller_criticality_override: None,
        g0_override: true,
        now_epoch_ms: NOW_MS,
        manifest: reference_manifest(),
        grant: CapGrant {
            grant_id: "grt_g0_1".to_owned(),
            resource_id: "res_drone_fc".to_owned(),
            allowed_modes: vec![AccessMode::Read],
            min_grade: GrantMinGrade::G1,
            criticality_override: None,
            mandate_binding: base_mandate(
                "res_drone_fc",
                AccessMode::Read,
                FayingGrade::G1,
                MandateBindingModel::ModelAPlusB,
            ),
        },
        human_confirmation: None,
    };
    let res1 = evaluate_production_command(&drone_g0, &anchors, &gate);
    assert_eq!(res1.error_code, Some(CapErrorCode::G0EmergencyOverride));
    let dir1 = res1.safe_state_directive.expect("directive");
    assert_eq!(dir1.action, G0SafeStateAction::EnterSafeState);
    assert_eq!(dir1.safe_state_id.as_deref(), Some("hover_and_rtl"));
    assert!(!dir1.blocks_human_takeover);

    let sensor_g0 = ProductionEvaluationRequest {
        resource_id: "res_legacy_sensor".to_owned(),
        operation_id: "read_temp".to_owned(),
        g0_override: false,
        grant: CapGrant {
            grant_id: "grt_g0_2".to_owned(),
            resource_id: "res_legacy_sensor".to_owned(),
            allowed_modes: vec![AccessMode::Read],
            min_grade: GrantMinGrade::G1,
            criticality_override: None,
            mandate_binding: base_mandate(
                "res_legacy_sensor",
                AccessMode::Read,
                FayingGrade::G0,
                MandateBindingModel::ModelAPlusB,
            ),
        },
        ..drone_g0
    };
    let res2 = evaluate_production_command(&sensor_g0, &anchors, &gate);
    let dir2 = res2.safe_state_directive.expect("directive");
    assert_eq!(dir2.action, G0SafeStateAction::TerminateAndAlert);
    assert!(dir2.safe_state_id.is_none());
    assert!(!dir2.blocks_human_takeover);
}

proptest! {
    #[test]
    fn prop_critical_never_allows_below_g1(
        grade_idx in 0usize..3usize,
    ) {
        let non_g1 = [FayingGrade::G2, FayingGrade::G3, FayingGrade::G4][grade_idx];
        let min_grade = match non_g1 {
            FayingGrade::G2 => GrantMinGrade::G2,
            FayingGrade::G3 => GrantMinGrade::G3,
            _ => GrantMinGrade::G4,
        };
        let req = ProductionEvaluationRequest {
            protocol_version: "1.0".to_owned(),
            request_id: "req_prop".to_owned(),
            fay_id: VALID_IFAY_ID.to_owned(),
            resource_id: "res_drone_fc".to_owned(),
            operation_id: "flash_esc_firmware".to_owned(),
            requested_mode: AccessMode::Write,
            params: json!({
                "image_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "target_bank": 0
            }),
            domain_category: None,
            caller_criticality_override: Some(Criticality::Routine),
            g0_override: false,
            now_epoch_ms: NOW_MS,
            manifest: reference_manifest(),
            grant: CapGrant {
                grant_id: "grt_prop".to_owned(),
                resource_id: "res_drone_fc".to_owned(),
                allowed_modes: vec![AccessMode::Write],
                min_grade,
                criticality_override: None,
                mandate_binding: base_mandate(
                    "res_drone_fc",
                    AccessMode::Write,
                    non_g1,
                    MandateBindingModel::ModelAPlusB,
                ),
            },
            human_confirmation: Some(HumanConfirmationProof {
                confirmation_id: "cnf_prop".to_owned(),
                human_prime_id: VALID_HID.to_owned(),
                confirmed_params_sha256: SHA_FLASH_BANK_0.to_owned(),
                terminal_attested_disclosure: true,
                expires_at_epoch_ms: NOW_MS + 30_000,
            }),
        };

        let audit = evaluate_production_command(
            &req,
            &reference_anchors(),
            &RealExecutionGateConfig::default(),
        );
        prop_assert_eq!(audit.disposition, ProductionDisposition::Deny);
        prop_assert_eq!(audit.error_code, Some(CapErrorCode::GradeInsufficient));
        prop_assert_eq!(audit.effective_criticality, Criticality::Critical);
    }
}
