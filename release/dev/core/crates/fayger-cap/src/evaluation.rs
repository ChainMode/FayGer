//! Conservative command evaluation for the local Fayger MVP demonstration.
//!
//! This module does not implement the pending CAP-to-Faying wire contract. It
//! accepts explicitly labeled demo evidence and makes that limitation visible
//! in every audit record.

use crate::{Criticality, DriverManifest};
use fayger_error::{ContextValue, ErrorCode, FaygerError, Layer};
use serde::{Deserialize, Serialize};

/// Stable schema label for machine-readable evaluation records.
pub const EVALUATION_SCHEMA_VERSION: &str = "fayger-cap-evaluation/0.1";

/// One operation evaluation request.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationRequest {
    /// Caller-generated request identifier.
    pub request_id: String,
    /// FayID reference for the acting iFay.
    pub fay_id: String,
    /// Driver resource identifier.
    pub resource_id: String,
    /// Driver operation identifier.
    pub operation_id: String,
}

impl EvaluationRequest {
    /// Creates a request with non-empty identifiers.
    pub fn new(
        request_id: impl Into<String>,
        fay_id: impl Into<String>,
        resource_id: impl Into<String>,
        operation_id: impl Into<String>,
    ) -> Result<Self, FaygerError> {
        let request = Self {
            request_id: request_id.into(),
            fay_id: fay_id.into(),
            resource_id: resource_id.into(),
            operation_id: operation_id.into(),
        };
        for (field, value) in [
            ("request_id", request.request_id.as_str()),
            ("fay_id", request.fay_id.as_str()),
            ("resource_id", request.resource_id.as_str()),
            ("operation_id", request.operation_id.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(evaluation_error("EmptyEvaluationField", field));
            }
        }
        Ok(request)
    }
}

/// Origin of authorization evidence supplied to the evaluator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationSource {
    /// Deterministic, local-only fixture used by the MVP demonstration.
    DemoFixture,
}

/// Local evidence status. This is deliberately not a CAP/Faying wire type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
    /// The local fixture is active.
    Active,
    /// The local fixture has been revoked.
    Revoked,
}

/// Authorization evidence used for a single local evaluation.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizationEvidence {
    /// Opaque evidence reference.
    pub reference: String,
    /// Evidence origin.
    pub source: AuthorizationSource,
    /// Evidence status.
    pub status: EvidenceStatus,
    /// FayID constrained by this fixture.
    pub fay_id: String,
    /// Resource constrained by this fixture.
    pub resource_id: String,
    /// Operation constrained by this fixture.
    pub operation_id: String,
}

impl AuthorizationEvidence {
    /// Creates active local-only evidence for a deterministic MVP fixture.
    pub fn active_demo_fixture(
        reference: impl Into<String>,
        fay_id: impl Into<String>,
        resource_id: impl Into<String>,
        operation_id: impl Into<String>,
    ) -> Result<Self, FaygerError> {
        let evidence = Self {
            reference: reference.into(),
            source: AuthorizationSource::DemoFixture,
            status: EvidenceStatus::Active,
            fay_id: fay_id.into(),
            resource_id: resource_id.into(),
            operation_id: operation_id.into(),
        };
        for (field, value) in [
            ("authorization.reference", evidence.reference.as_str()),
            ("authorization.fay_id", evidence.fay_id.as_str()),
            ("authorization.resource_id", evidence.resource_id.as_str()),
            ("authorization.operation_id", evidence.operation_id.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(evaluation_error("EmptyAuthorizationField", field));
            }
        }
        Ok(evidence)
    }

    /// Creates revoked local-only evidence for denial-path tests and demos.
    pub fn revoked_demo_fixture(
        reference: impl Into<String>,
        fay_id: impl Into<String>,
        resource_id: impl Into<String>,
        operation_id: impl Into<String>,
    ) -> Result<Self, FaygerError> {
        let mut evidence = Self::active_demo_fixture(reference, fay_id, resource_id, operation_id)?;
        evidence.status = EvidenceStatus::Revoked;
        Ok(evidence)
    }

    fn scope_matches(&self, request: &EvaluationRequest) -> bool {
        self.fay_id == request.fay_id
            && self.resource_id == request.resource_id
            && self.operation_id == request.operation_id
    }
}

/// Evaluation disposition. Only `allow_simulation` can advance the local demo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    /// Safe enough to execute in the explicitly simulated local demo.
    AllowSimulation,
    /// Held until CAP confirmation/grade policy is frozen.
    HoldForPolicy,
    /// Rejected.
    Deny,
}

/// Authorization facts copied into the audit record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizationAudit {
    /// Opaque evidence reference.
    pub reference: String,
    /// Evidence origin.
    pub source: AuthorizationSource,
    /// Whether the local evidence was active.
    pub active: bool,
    /// Whether evidence scope exactly matched the request.
    pub scope_matches: bool,
    /// Always false for the MVP demo fixture.
    pub production_verified: bool,
}

/// Machine-readable record emitted for every evaluation outcome.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationRecord {
    /// Record schema version.
    pub schema_version: String,
    /// Original request.
    pub request: EvaluationRequest,
    /// Driver package used for classification.
    pub driver_package: String,
    /// Effective criticality.
    pub criticality: Criticality,
    /// Evaluation disposition.
    pub decision: Decision,
    /// Stable local reason label.
    pub reason: String,
    /// Authorization audit facts.
    pub authorization: AuthorizationAudit,
    /// Product-owner decisions intentionally not frozen by this record.
    pub pending_decisions: Vec<String>,
}

/// Evaluates one command under conservative MVP-only rules.
///
/// Unknown resources/operations are critical and denied. Critical operations
/// are denied. Sensitive and high operations are held. Only routine operations
/// with active, exactly matching demo evidence may advance to simulation.
pub fn evaluate_command(
    driver: &DriverManifest,
    request: &EvaluationRequest,
    evidence: &AuthorizationEvidence,
) -> Result<EvaluationRecord, FaygerError> {
    driver.validate()?;

    let operation = driver.operation(&request.resource_id, &request.operation_id);
    let criticality = operation.map_or(Criticality::Critical, |operation| operation.criticality);
    let active = evidence.status == EvidenceStatus::Active;
    let scope_matches = evidence.scope_matches(request);

    let (decision, reason) = if operation.is_none() {
        (Decision::Deny, "unknown_operation")
    } else if !active {
        (Decision::Deny, "authorization_inactive")
    } else if !scope_matches {
        (Decision::Deny, "authorization_scope_mismatch")
    } else {
        match criticality {
            Criticality::Routine => (Decision::AllowSimulation, "routine_demo_operation"),
            Criticality::Sensitive | Criticality::High => {
                (Decision::HoldForPolicy, "confirmation_policy_pending")
            }
            Criticality::Critical => (Decision::Deny, "critical_operation_blocked_in_demo"),
        }
    };

    Ok(EvaluationRecord {
        schema_version: EVALUATION_SCHEMA_VERSION.to_owned(),
        request: request.clone(),
        driver_package: driver.driver_package.clone(),
        criticality,
        decision,
        reason: reason.to_owned(),
        authorization: AuthorizationAudit {
            reference: evidence.reference.clone(),
            source: evidence.source,
            active,
            scope_matches,
            production_verified: false,
        },
        pending_decisions: vec![
            "CAP-D-1".to_owned(),
            "CAP-D-2".to_owned(),
            "CAP-D-5".to_owned(),
            "CAP-D-6".to_owned(),
        ],
    })
}

fn evaluation_error(reason: &'static str, field: &str) -> FaygerError {
    FaygerError::new(
        ErrorCode::FmValidationFailed,
        Layer::Assembler,
        "CAP evaluation input validation failed",
    )
    .with_context("reason", ContextValue::string(reason))
    .with_context("field", ContextValue::string(field))
}
