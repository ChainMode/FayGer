//! CAP driver manifest primitives for Fayger MVP integration.
//!
//! This crate is intentionally small. It models driver-declared resources,
//! operations, criticality declarations, and safe states so Fayger can attach
//! a minimal controllable-resource layer to FayManifest assembly.
//!
//! It does **not** decide CAP's pending governance questions:
//! - driver manifest signature / registry trust (CAP D-5);
//! - domain-approved safe-state catalogs (CAP D-3);
//! - high/critical confirmation policy (CAP D-6).
//!
//! The safety posture here is conservative: unknown operation criticality is
//! treated as [`Criticality::Critical`], and callers may only raise a declared
//! criticality, never lower it.

use fayger_error::{ContextValue, FaygerError, Layer};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

mod evaluation;
mod production;

pub use evaluation::{
    evaluate_command, AuthorizationAudit, AuthorizationEvidence, AuthorizationSource, Decision,
    EvaluationRecord, EvaluationRequest, EvidenceStatus, EVALUATION_SCHEMA_VERSION,
};
pub use production::{
    canonical_json, compute_params_sha256, evaluate_production_command, CapErrorCode, CapGrant,
    ConfirmationAuditSummary, ConsequenceIrreversibility, DomainCategory, FayingGrade,
    G0SafeStateAction, G0SafeStateDirective, GrantMinGrade, HumanConfirmationProof,
    MandateBindingModel, ProductionAuditRecord, ProductionDisposition, ProductionEvaluationRequest,
    ProductionMandateEvidence, ProductionOperationDescriptor, ProductionResourceDescriptor,
    ProductionSafeStateDescriptor, RealExecutionGateConfig, SignedDriverManifest, TrustAnchorStore,
    PRODUCTION_EVALUATION_SCHEMA_VERSION,
};

/// CAP resource identifier as exposed by a driver manifest.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ResourceId(String);

impl ResourceId {
    /// Creates a non-empty resource id.
    pub fn new(value: impl Into<String>) -> Result<Self, FaygerError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(validation_error("EmptyResourceId", "resource_id"));
        }
        Ok(Self(value))
    }

    /// Returns the raw identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Driver operation identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OperationId(String);

impl OperationId {
    /// Creates a non-empty operation id.
    pub fn new(value: impl Into<String>) -> Result<Self, FaygerError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(validation_error("EmptyOperationId", "operation_id"));
        }
        Ok(Self(value))
    }

    /// Returns the raw identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// CAP operation access mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessMode {
    /// Read-only observation.
    Read,
    /// Write mutable state.
    Write,
    /// Execute an action.
    Execute,
    /// Configure persistent resource settings.
    Configure,
}

/// Conservative operation criticality classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Criticality {
    /// Routine action with low consequence.
    Routine,
    /// Sensitive action needing stricter oversight.
    Sensitive,
    /// High-impact action; confirmation policy is pending CAP D-6.
    High,
    /// Critical action; default for unknown declarations.
    Critical,
}

impl Criticality {
    /// Returns the stricter of two criticality levels.
    #[must_use]
    pub const fn raised_to(self, other: Self) -> Self {
        if self as u8 >= other as u8 {
            self
        } else {
            other
        }
    }
}

impl fmt::Display for Criticality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Routine => "routine",
            Self::Sensitive => "sensitive",
            Self::High => "high",
            Self::Critical => "critical",
        })
    }
}

/// A safe-state that a driver claims it can request for a resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafeState {
    /// Stable safe-state id, e.g. `hover` or `return_home`.
    pub id: String,
    /// Human-readable explanation for review and audit.
    pub description: String,
}

impl SafeState {
    /// Creates a safe-state declaration.
    pub fn new(id: impl Into<String>, description: impl Into<String>) -> Result<Self, FaygerError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(validation_error("EmptySafeState", "safe_states[].id"));
        }
        let description = description.into();
        if description.trim().is_empty() {
            return Err(validation_error(
                "EmptySafeStateDescription",
                "safe_states[].description",
            ));
        }
        Ok(Self { id, description })
    }
}

/// Driver-declared operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationDeclaration {
    /// Stable operation id.
    pub id: OperationId,
    /// CAP mode used by the operation.
    pub mode: AccessMode,
    /// Driver-declared operation criticality.
    pub criticality: Criticality,
}

impl OperationDeclaration {
    /// Creates an operation declaration.
    pub const fn new(id: OperationId, mode: AccessMode, criticality: Criticality) -> Self {
        Self {
            id,
            mode,
            criticality,
        }
    }
}

/// Driver-declared controllable resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceDeclaration {
    /// Resource id.
    pub id: ResourceId,
    /// Supported operations.
    pub operations: Vec<OperationDeclaration>,
    /// Safe states supported by the driver for this resource.
    #[serde(default)]
    pub safe_states: Vec<SafeState>,
}

impl ResourceDeclaration {
    /// Creates a resource declaration.
    pub const fn new(
        id: ResourceId,
        operations: Vec<OperationDeclaration>,
        safe_states: Vec<SafeState>,
    ) -> Self {
        Self {
            id,
            operations,
            safe_states,
        }
    }
}

/// Minimal CAP driver manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriverManifest {
    /// Driver package identifier, e.g. `@drone-drivers/dji-fc`.
    pub driver_package: String,
    /// Manifest version.
    pub version: String,
    /// Controllable resources exposed by this driver.
    pub resources: Vec<ResourceDeclaration>,
}

impl DriverManifest {
    /// Parses and validates a driver manifest from JSON.
    pub fn from_json_str(input: &str) -> Result<Self, FaygerError> {
        let manifest: Self = serde_json::from_str(input).map_err(|e| {
            FaygerError::new(
                fayger_error::ErrorCode::FmParseFail,
                Layer::Assembler,
                "CAP driver manifest JSON parse failed",
            )
            .with_context("detail", ContextValue::string(e.to_string()))
        })?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validates conservative structural invariants.
    pub fn validate(&self) -> Result<(), FaygerError> {
        if self.driver_package.trim().is_empty() {
            return Err(validation_error("EmptyDriverPackage", "driver_package"));
        }
        if self.version.trim().is_empty() {
            return Err(validation_error("EmptyVersion", "version"));
        }
        if self.resources.is_empty() {
            return Err(validation_error("NoResources", "resources"));
        }

        let mut resource_ids = HashSet::new();
        for resource in &self.resources {
            if !resource_ids.insert(resource.id.as_str()) {
                return Err(validation_error("DuplicateResource", "resources[].id"));
            }
            if resource.operations.is_empty() {
                return Err(validation_error("NoOperations", "resources[].operations"));
            }

            let mut operation_ids = HashSet::new();
            for operation in &resource.operations {
                if !operation_ids.insert(operation.id.as_str()) {
                    return Err(validation_error(
                        "DuplicateOperation",
                        "resources[].operations[].id",
                    ));
                }
            }
        }

        Ok(())
    }

    /// Returns the declared criticality for a resource operation.
    ///
    /// Unknown resources or operations default to [`Criticality::Critical`].
    pub fn criticality_for(&self, resource_id: &str, operation_id: &str) -> Criticality {
        self.resources
            .iter()
            .find(|r| r.id.as_str() == resource_id)
            .and_then(|r| {
                r.operations
                    .iter()
                    .find(|op| op.id.as_str() == operation_id)
                    .map(|op| op.criticality)
            })
            .unwrap_or(Criticality::Critical)
    }

    /// Returns a declared operation, if both resource and operation are known.
    pub fn operation(
        &self,
        resource_id: &str,
        operation_id: &str,
    ) -> Option<&OperationDeclaration> {
        self.resources
            .iter()
            .find(|resource| resource.id.as_str() == resource_id)
            .and_then(|resource| {
                resource
                    .operations
                    .iter()
                    .find(|operation| operation.id.as_str() == operation_id)
            })
    }

    /// Returns a safe-state declaration by id.
    pub fn safe_state(&self, resource_id: &str, safe_state_id: &str) -> Option<&SafeState> {
        self.resources
            .iter()
            .find(|r| r.id.as_str() == resource_id)
            .and_then(|r| r.safe_states.iter().find(|s| s.id == safe_state_id))
    }
}

/// Applies a caller-requested raise-only override.
#[must_use]
pub const fn raise_criticality(declared: Criticality, requested: Criticality) -> Criticality {
    declared.raised_to(requested)
}

fn validation_error(reason: &'static str, field: &'static str) -> FaygerError {
    FaygerError::new(
        fayger_error::ErrorCode::FmValidationFailed,
        Layer::Assembler,
        "CAP driver manifest validation failed",
    )
    .with_context("reason", ContextValue::string(reason))
    .with_context("field", ContextValue::string(field))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::{
        raise_criticality, AccessMode, Criticality, DriverManifest, OperationDeclaration,
        OperationId, ResourceDeclaration, ResourceId, SafeState,
    };

    fn drone_manifest_json() -> &'static str {
        r#"{
          "driver_package": "@drone-drivers/dji-fc",
          "version": "0.1.0",
          "resources": [
            {
              "id": "flight_controller",
              "operations": [
                { "id": "read_telemetry", "mode": "read", "criticality": "routine" },
                { "id": "arm_motors", "mode": "execute", "criticality": "critical" },
                { "id": "upload_firmware", "mode": "configure", "criticality": "high" }
              ],
              "safe_states": [
                { "id": "hover", "description": "Hold current position when the vehicle is airborne." },
                { "id": "return_home", "description": "Navigate back to the configured home point." }
              ]
            }
          ]
        }"#
    }

    #[test]
    fn parses_minimal_drone_driver_manifest() {
        let manifest = DriverManifest::from_json_str(drone_manifest_json()).expect("parse");

        assert_eq!(manifest.driver_package, "@drone-drivers/dji-fc");
        assert_eq!(
            manifest.criticality_for("flight_controller", "read_telemetry"),
            Criticality::Routine
        );
        assert_eq!(
            manifest.criticality_for("flight_controller", "arm_motors"),
            Criticality::Critical
        );
        assert_eq!(
            manifest
                .safe_state("flight_controller", "return_home")
                .expect("safe state")
                .description,
            "Navigate back to the configured home point."
        );
    }

    #[test]
    fn unknown_operation_defaults_to_critical() {
        let manifest = DriverManifest::from_json_str(drone_manifest_json()).expect("parse");

        assert_eq!(
            manifest.criticality_for("flight_controller", "unknown_operation"),
            Criticality::Critical
        );
        assert_eq!(
            manifest.criticality_for("unknown_resource", "read_telemetry"),
            Criticality::Critical
        );
    }

    #[test]
    fn caller_can_raise_but_not_lower_criticality() {
        assert_eq!(
            raise_criticality(Criticality::Critical, Criticality::Routine),
            Criticality::Critical
        );
        assert_eq!(
            raise_criticality(Criticality::Sensitive, Criticality::High),
            Criticality::High
        );
    }

    #[test]
    fn rejects_duplicate_resource_and_operation_ids() {
        let duplicated_resource = r#"{
          "driver_package": "@drone-drivers/dji-fc",
          "version": "0.1.0",
          "resources": [
            { "id": "flight_controller", "operations": [{ "id": "read", "mode": "read", "criticality": "routine" }] },
            { "id": "flight_controller", "operations": [{ "id": "write", "mode": "write", "criticality": "sensitive" }] }
          ]
        }"#;
        assert!(DriverManifest::from_json_str(duplicated_resource).is_err());

        let duplicated_operation = r#"{
          "driver_package": "@drone-drivers/dji-fc",
          "version": "0.1.0",
          "resources": [
            {
              "id": "flight_controller",
              "operations": [
                { "id": "read", "mode": "read", "criticality": "routine" },
                { "id": "read", "mode": "write", "criticality": "sensitive" }
              ]
            }
          ]
        }"#;
        assert!(DriverManifest::from_json_str(duplicated_operation).is_err());
    }

    #[test]
    fn constructors_reject_empty_identifiers() {
        assert!(ResourceId::new("").is_err());
        assert!(OperationId::new(" ").is_err());
        assert!(SafeState::new("hover", "").is_err());
    }

    #[test]
    fn manual_struct_validation_matches_json_path() {
        let manifest = DriverManifest {
            driver_package: "@drone-drivers/dji-fc".to_owned(),
            version: "0.1.0".to_owned(),
            resources: vec![ResourceDeclaration::new(
                ResourceId::new("flight_controller").expect("resource id"),
                vec![OperationDeclaration::new(
                    OperationId::new("read_telemetry").expect("operation id"),
                    AccessMode::Read,
                    Criticality::Routine,
                )],
                vec![SafeState::new("hover", "Hold current position.").expect("safe state")],
            )],
        };

        assert!(manifest.validate().is_ok());
    }
}
