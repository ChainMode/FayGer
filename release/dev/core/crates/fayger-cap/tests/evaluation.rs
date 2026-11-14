//! Security and audit regression tests for the local CAP evaluator.

#![allow(clippy::expect_used)]

use fayger_cap::{
    evaluate_command, AuthorizationEvidence, AuthorizationSource, Criticality, Decision,
    DriverManifest, EvaluationRequest,
};

fn drone_driver() -> DriverManifest {
    DriverManifest::from_json_str(
        r#"{
          "driver_package": "@drone-drivers/dji-fc",
          "version": "0.1.0",
          "resources": [{
            "id": "flight_controller",
            "operations": [
              { "id": "read_telemetry", "mode": "read", "criticality": "routine" },
              { "id": "upload_firmware", "mode": "configure", "criticality": "high" },
              { "id": "arm_motors", "mode": "execute", "criticality": "critical" }
            ],
            "safe_states": [
              { "id": "return_home", "description": "Return to the configured home point." }
            ]
          }]
        }"#,
    )
    .expect("valid driver")
}

fn request(operation: &str) -> EvaluationRequest {
    EvaluationRequest::new(
        "demo-request-001",
        "urn:ifay:demo:drone-controller",
        "flight_controller",
        operation,
    )
    .expect("valid request")
}

fn evidence(operation: &str) -> AuthorizationEvidence {
    AuthorizationEvidence::active_demo_fixture(
        "demo-mandate-001",
        "urn:ifay:demo:drone-controller",
        "flight_controller",
        operation,
    )
    .expect("valid evidence")
}

#[test]
fn routine_operation_with_matching_demo_evidence_is_allowed_for_simulation() {
    let record = evaluate_command(
        &drone_driver(),
        &request("read_telemetry"),
        &evidence("read_telemetry"),
    )
    .expect("evaluation");

    assert_eq!(record.decision, Decision::AllowSimulation);
    assert_eq!(record.criticality, Criticality::Routine);
    assert_eq!(
        record.authorization.source,
        AuthorizationSource::DemoFixture
    );
    assert!(!record.authorization.production_verified);
    assert!(record.pending_decisions.contains(&"CAP-D-1".to_owned()));
}

#[test]
fn mismatched_authorization_is_denied() {
    let record = evaluate_command(
        &drone_driver(),
        &request("read_telemetry"),
        &evidence("upload_firmware"),
    )
    .expect("evaluation");

    assert_eq!(record.decision, Decision::Deny);
    assert_eq!(record.reason, "authorization_scope_mismatch");
}

#[test]
fn revoked_authorization_is_denied() {
    let revoked = AuthorizationEvidence::revoked_demo_fixture(
        "demo-mandate-001",
        "urn:ifay:demo:drone-controller",
        "flight_controller",
        "read_telemetry",
    )
    .expect("valid revoked evidence");
    let record = evaluate_command(&drone_driver(), &request("read_telemetry"), &revoked)
        .expect("evaluation");

    assert_eq!(record.decision, Decision::Deny);
    assert_eq!(record.reason, "authorization_inactive");
}

#[test]
fn unknown_operation_is_critical_and_denied() {
    let record = evaluate_command(
        &drone_driver(),
        &request("not_declared"),
        &evidence("not_declared"),
    )
    .expect("evaluation");

    assert_eq!(record.criticality, Criticality::Critical);
    assert_eq!(record.decision, Decision::Deny);
    assert_eq!(record.reason, "unknown_operation");
}

#[test]
fn high_operation_is_held_and_critical_operation_is_denied() {
    let high = evaluate_command(
        &drone_driver(),
        &request("upload_firmware"),
        &evidence("upload_firmware"),
    )
    .expect("high evaluation");
    assert_eq!(high.decision, Decision::HoldForPolicy);
    assert_eq!(high.reason, "confirmation_policy_pending");

    let critical = evaluate_command(
        &drone_driver(),
        &request("arm_motors"),
        &evidence("arm_motors"),
    )
    .expect("critical evaluation");
    assert_eq!(critical.decision, Decision::Deny);
    assert_eq!(critical.reason, "critical_operation_blocked_in_demo");
}

#[test]
fn evaluation_record_is_machine_readable_json() {
    let record = evaluate_command(
        &drone_driver(),
        &request("read_telemetry"),
        &evidence("read_telemetry"),
    )
    .expect("evaluation");
    let json = serde_json::to_value(record).expect("serialize");

    assert_eq!(json["schema_version"], "fayger-cap-evaluation/0.1");
    assert_eq!(json["decision"], "allow_simulation");
    assert_eq!(json["authorization"]["production_verified"], false);
}
