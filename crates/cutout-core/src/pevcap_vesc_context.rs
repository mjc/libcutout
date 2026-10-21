//! Checked local geometry retained with a captured semantic observation.

use crate::{
    ConnectionAttemptToken, PevcapRecord, PevcapTelemetryProvenance, RequestTarget, VescDrivetrain,
    VescDrivetrainError, VescDrivetrainProfile, VescDrivetrainRevision,
};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;

/// Historical attribution retained independently of usable speed geometry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturedVescDrivetrainScope {
    attempt: ConnectionAttemptToken,
    revision: VescDrivetrainRevision,
}

impl CapturedVescDrivetrainScope {
    /// Device and generation that produced the original observation.
    #[must_use]
    pub const fn attempt(&self) -> &ConnectionAttemptToken {
        &self.attempt
    }

    /// Persisted profile revision asserted by the original observation.
    #[must_use]
    pub const fn revision(&self) -> VescDrivetrainRevision {
        self.revision
    }
}

/// Geometry evidence from one original record; absence never inherits another record's profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CapturedVescDrivetrainEvidence {
    /// The record contains no local geometry assertion.
    Missing,
    /// A geometry assertion cannot satisfy the captured snapshot contract.
    Invalid(CapturedVescDrivetrainError),
    /// This context or transport record names a nonlocal or unknown controller target.
    UnsupportedTarget,
    /// Checked user configuration and original attribution, without live admission authority.
    Local(CapturedVescDrivetrainContext),
}

/// A captured user profile; this token records history and never grants a live connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturedVescDrivetrainContext {
    profile: VescDrivetrainProfile,
    attempt: ConnectionAttemptToken,
    provenance: PevcapTelemetryProvenance,
}

impl CapturedVescDrivetrainContext {
    /// Original checked geometry and its nonzero persisted revision.
    #[must_use]
    pub const fn profile(&self) -> VescDrivetrainProfile {
        self.profile
    }

    /// Original device and attempt scope, independent of the capture producer's platform name.
    #[must_use]
    pub const fn attempt(&self) -> &ConnectionAttemptToken {
        &self.attempt
    }

    /// Whether the source semantic observation was live or already replay-derived.
    #[must_use]
    pub const fn provenance(&self) -> PevcapTelemetryProvenance {
        self.provenance
    }
}

/// Why a present captured geometry assertion is unusable.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CapturedVescDrivetrainError {
    /// A nonlocal or unknown target cannot supply directly connected controller input.
    #[error("unsupported captured VESC controller target")]
    UnsupportedTarget,
    /// JSON syntax, shape, field types, or duplicate contract fields are invalid.
    #[error("invalid captured VESC snapshot JSON")]
    MalformedSnapshot,
    /// Geometry context is defined only by semantic snapshot schema 2.
    #[error("unsupported captured VESC snapshot schema {0}")]
    UnsupportedSnapshotSchema(u16),
    /// Only explicit user configuration is supported by this context contract.
    #[error("unsupported captured VESC geometry source")]
    UnsupportedSource,
    /// Persisted profile revisions must be nonzero.
    #[error("invalid captured VESC geometry revision")]
    InvalidRevision,
    /// Captured scope requires a nonzero generation and bounded nonblank device identifier.
    #[error("invalid captured VESC connection attempt")]
    InvalidAttempt,
    /// Canonical geometry failed the existing checked constructor.
    #[error("invalid captured VESC geometry: {0}")]
    InvalidGeometry(VescDrivetrainError),
    /// Captured circumference disagrees with the geometry that would calculate speed.
    #[error("captured VESC circumference does not match wheel diameter")]
    InconsistentCircumference,
}

impl PevcapRecord {
    /// Checks only this record's local geometry context without changing its bytes or JSON.
    ///
    /// A missing target denotes the legacy local context; the explicit local target is the
    /// string `"local"`. Other controller contexts do not supply geometry for the directly
    /// connected controller. No current device database is read.
    #[must_use]
    pub fn captured_vesc_drivetrain(&self) -> CapturedVescDrivetrainEvidence {
        match self.captured_vesc_context_input() {
            Ok(None) => CapturedVescDrivetrainEvidence::Missing,
            Ok(Some(input)) => match input.geometry() {
                Ok(context) => CapturedVescDrivetrainEvidence::Local(context),
                Err(error) => CapturedVescDrivetrainEvidence::Invalid(error),
            },
            Err(CapturedVescDrivetrainError::UnsupportedTarget) => {
                CapturedVescDrivetrainEvidence::UnsupportedTarget
            }
            Err(error) => CapturedVescDrivetrainEvidence::Invalid(error),
        }
    }

    /// Checks record attribution before geometry can erase identity evidence.
    ///
    /// # Errors
    ///
    /// Rejects unsupported targets, schemas and malformed or invalid scope fields.
    pub fn captured_vesc_drivetrain_scope(
        &self,
    ) -> Result<Option<CapturedVescDrivetrainScope>, CapturedVescDrivetrainError> {
        Ok(self.captured_vesc_context_input()?.map(|input| input.scope))
    }

    fn captured_vesc_context_input(
        &self,
    ) -> Result<Option<CapturedContextInput<'_>>, CapturedVescDrivetrainError> {
        if let Some(RequestTarget::VescCanController { .. }) = self.target {
            return Err(CapturedVescDrivetrainError::UnsupportedTarget);
        }
        let Some(semantic) = self.semantic_telemetry.as_ref() else {
            return Ok(None);
        };
        let JsonObject(snapshot) =
            serde_json::from_str::<JsonObject<CapturedSnapshot<'_>>>(&semantic.snapshot_json)
                .map_err(|_| CapturedVescDrivetrainError::MalformedSnapshot)?;
        let Some(context) = snapshot.vesc_drivetrain_context else {
            return Ok(None);
        };
        let JsonObject(header) =
            serde_json::from_str::<JsonObject<CapturedScopeJson<'_>>>(context.get())
                .map_err(|_| CapturedVescDrivetrainError::MalformedSnapshot)?;
        if header.target == CapturedTarget::Unsupported {
            return Err(CapturedVescDrivetrainError::UnsupportedTarget);
        }
        if semantic.snapshot_schema_version != 2 {
            return Err(CapturedVescDrivetrainError::UnsupportedSnapshotSchema(
                semantic.snapshot_schema_version,
            ));
        }
        Ok(Some(CapturedContextInput {
            json: context,
            scope: header.check()?,
            provenance: semantic.provenance,
        }))
    }
}

// Borrow original fragments so target and identity survive malformed geometry, while
// derived map deserializers still reject duplicate fields rather than choosing a value.
#[derive(Deserialize)]
struct CapturedSnapshot<'a> {
    #[serde(borrow)]
    vesc_drivetrain_context: Option<&'a RawValue>,
}

#[derive(Deserialize)]
struct CapturedScopeJson<'a> {
    #[serde(default)]
    target: CapturedTarget,
    #[serde(borrow)]
    connection_attempt: Option<&'a RawValue>,
    #[serde(borrow)]
    revision: Option<&'a RawValue>,
}

impl CapturedScopeJson<'_> {
    fn check(self) -> Result<CapturedVescDrivetrainScope, CapturedVescDrivetrainError> {
        let revision = serde_json::from_str(required(self.revision)?.get())
            .map_err(|_| CapturedVescDrivetrainError::MalformedSnapshot)?;
        let revision = VescDrivetrainRevision::new(revision)
            .ok_or(CapturedVescDrivetrainError::InvalidRevision)?;
        let JsonObject(attempt) = serde_json::from_str::<JsonObject<CapturedAttemptJson>>(
            required(self.connection_attempt)?.get(),
        )
        .map_err(|_| CapturedVescDrivetrainError::MalformedSnapshot)?;
        if attempt.generation == 0
            || attempt.platform_identifier.trim().is_empty()
            || attempt.platform_identifier.len() > 512
        {
            return Err(CapturedVescDrivetrainError::InvalidAttempt);
        }
        Ok(CapturedVescDrivetrainScope {
            attempt: ConnectionAttemptToken::new(attempt.generation, attempt.platform_identifier),
            revision,
        })
    }
}

struct CapturedContextInput<'a> {
    json: &'a RawValue,
    scope: CapturedVescDrivetrainScope,
    provenance: PevcapTelemetryProvenance,
}

impl CapturedContextInput<'_> {
    fn geometry(self) -> Result<CapturedVescDrivetrainContext, CapturedVescDrivetrainError> {
        let JsonObject(context) =
            serde_json::from_str::<JsonObject<CapturedContextJson>>(self.json.get())
                .map_err(|_| CapturedVescDrivetrainError::MalformedSnapshot)?;
        context.check(self.scope, self.provenance)
    }
}

/// Named JSON contract objects never accept Serde's positional struct representation.
struct JsonObject<T>(T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for JsonObject<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for ObjectVisitor<T> {
            type Value = JsonObject<T>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a JSON object")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                T::deserialize(serde::de::value::MapAccessDeserializer::new(map)).map(JsonObject)
            }
        }
        deserializer.deserialize_map(ObjectVisitor(std::marker::PhantomData))
    }
}

#[derive(Deserialize)]
struct CapturedContextJson {
    source: Option<String>,
    wheel_diameter_micrometres: Option<u32>,
    wheel_circumference_mm: Option<u64>,
    motor_pole_pairs: Option<u8>,
    motor_to_wheel_ratio_milli: Option<u32>,
}

#[derive(Deserialize)]
struct CapturedAttemptJson {
    generation: u64,
    platform_identifier: String,
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum CapturedTarget {
    #[default]
    LegacyLocal,
    Local,
    Unsupported,
}

impl<'de> Deserialize<'de> for CapturedTarget {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(if value.as_str() == Some("local") {
            Self::Local
        } else {
            Self::Unsupported
        })
    }
}

impl CapturedContextJson {
    fn check(
        self,
        scope: CapturedVescDrivetrainScope,
        provenance: PevcapTelemetryProvenance,
    ) -> Result<CapturedVescDrivetrainContext, CapturedVescDrivetrainError> {
        if required(self.source)? != "user_configured" {
            return Err(CapturedVescDrivetrainError::UnsupportedSource);
        }
        let value = VescDrivetrain::from_parts(
            required(self.wheel_diameter_micrometres)?,
            required(self.motor_pole_pairs)?,
            required(self.motor_to_wheel_ratio_milli)?,
        )
        .map_err(CapturedVescDrivetrainError::InvalidGeometry)?;
        if value.geometry().wheel_circumference.as_millimetres()
            != required(self.wheel_circumference_mm)?
        {
            return Err(CapturedVescDrivetrainError::InconsistentCircumference);
        }
        Ok(CapturedVescDrivetrainContext {
            profile: VescDrivetrainProfile::user_configured(value, scope.revision),
            attempt: scope.attempt,
            provenance,
        })
    }
}

fn required<T>(value: Option<T>) -> Result<T, CapturedVescDrivetrainError> {
    value.ok_or(CapturedVescDrivetrainError::MalformedSnapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        GattChannel, MonotonicTimestamp, PevcapSemanticTelemetry, RequestTarget, VescControllerId,
        VescDrivetrain,
    };
    use serde_json::{Value, json};

    fn context() -> Value {
        json!({
            "connection_attempt": {"generation": 7, "platform_identifier": "controller-a"},
            "revision": 3,
            "source": "user_configured",
            "wheel_diameter_micrometres": 90_000,
            "wheel_circumference_mm": 283,
            "motor_pole_pairs": 7,
            "motor_to_wheel_ratio_milli": 2_500
        })
    }

    fn record(snapshot: &Value) -> PevcapRecord {
        PevcapRecord::inbound_notification(
            MonotonicTimestamp::new(100),
            GattChannel::from_bytes([1; 16]),
            GattChannel::from_bytes([2; 16]),
            vec![2, 3, 4],
        )
        .with_semantic_telemetry(PevcapSemanticTelemetry {
            observed_at_ms: Some(MonotonicTimestamp::new(99)),
            provenance: PevcapTelemetryProvenance::LiveSession,
            snapshot_schema_version: 2,
            library_version: "original-version".to_owned(),
            snapshot_json: snapshot.to_string(),
        })
    }

    fn assert_local(record: &PevcapRecord) -> CapturedVescDrivetrainContext {
        let CapturedVescDrivetrainEvidence::Local(context) = record.captured_vesc_drivetrain()
        else {
            panic!("expected checked captured local geometry");
        };
        context
    }

    #[test]
    fn captured_vesc_context_legacy_local_retains_profile_attempt_provenance_and_original() {
        let record =
            record(&json!({"vesc_drivetrain_context": context(), "speed": {"value": 1_617}}));
        let original = record.clone();
        let checked = assert_local(&record);
        assert_eq!(
            checked.profile().value(),
            VescDrivetrain::parse("90", "7", "2.5").unwrap()
        );
        assert_eq!(checked.profile().revision().get(), 3);
        assert_eq!(
            checked.profile().source(),
            crate::VescDrivetrainSource::UserConfigured
        );
        assert_eq!(checked.attempt().generation(), 7);
        assert_eq!(checked.attempt().platform_identifier(), "controller-a");
        assert_eq!(checked.provenance(), PevcapTelemetryProvenance::LiveSession);
        assert_eq!(record, original);
    }

    #[test]
    fn captured_vesc_context_accepts_explicit_local_and_preserves_replay_source() {
        let mut context = context();
        context["target"] = json!("local");
        let mut record = record(&json!({"vesc_drivetrain_context": context}));
        record.target = Some(RequestTarget::Local);
        record.semantic_telemetry.as_mut().unwrap().provenance = PevcapTelemetryProvenance::Replay;
        assert_eq!(
            assert_local(&record).provenance(),
            PevcapTelemetryProvenance::Replay
        );
    }

    #[test]
    fn captured_vesc_context_missing_null_and_configured_arrays_remain_unknown() {
        for snapshot in [
            json!({}),
            json!({"vesc_drivetrain_context": null}),
            json!({"vesc_drivetrain_contexts": [context()]}),
        ] {
            let record = record(&snapshot);
            assert_eq!(
                record.captured_vesc_drivetrain(),
                CapturedVescDrivetrainEvidence::Missing
            );
        }
        let mut record = record(&json!({}));
        record.semantic_telemetry = None;
        assert_eq!(
            record.captured_vesc_drivetrain(),
            CapturedVescDrivetrainEvidence::Missing
        );
    }

    #[test]
    fn captured_vesc_context_nonlocal_or_unknown_target_never_becomes_local() {
        assert_eq!(
            record(&json!({"vesc_drivetrain_context": {"target": {"can": {"controller_id": 3}}}}))
                .captured_vesc_drivetrain(),
            CapturedVescDrivetrainEvidence::UnsupportedTarget
        );
        for target in [
            json!({"can": {"controller_id": 3, "controller_uuid": [0,0,0,0,0,0,0,0,0,0,0,3]}}),
            json!({"kind": "local"}),
            json!({"kind": "future"}),
            Value::Null,
        ] {
            let mut context = context();
            context["target"] = target;
            assert_eq!(
                record(&json!({"vesc_drivetrain_context": context})).captured_vesc_drivetrain(),
                CapturedVescDrivetrainEvidence::UnsupportedTarget
            );
        }
        let mut record = record(&json!({"vesc_drivetrain_context": context()}));
        record.target = Some(RequestTarget::VescCanController {
            controller_id: VescControllerId::new(3),
        });
        assert_eq!(
            record.captured_vesc_drivetrain(),
            CapturedVescDrivetrainEvidence::UnsupportedTarget
        );
    }

    #[test]
    fn captured_vesc_context_rejects_unknown_schema_source_and_zero_revision() {
        let mut wrong_schema = record(&json!({"vesc_drivetrain_context": context()}));
        wrong_schema
            .semantic_telemetry
            .as_mut()
            .unwrap()
            .snapshot_schema_version = 3;
        assert_eq!(
            wrong_schema.captured_vesc_drivetrain(),
            CapturedVescDrivetrainEvidence::Invalid(
                CapturedVescDrivetrainError::UnsupportedSnapshotSchema(3)
            )
        );
        let mut source = context();
        source["source"] = json!("controller_reported");
        assert_eq!(
            record(&json!({"vesc_drivetrain_context": source})).captured_vesc_drivetrain(),
            CapturedVescDrivetrainEvidence::Invalid(CapturedVescDrivetrainError::UnsupportedSource)
        );
        let mut revision = context();
        revision["revision"] = json!(0);
        assert_eq!(
            record(&json!({"vesc_drivetrain_context": revision})).captured_vesc_drivetrain(),
            CapturedVescDrivetrainEvidence::Invalid(CapturedVescDrivetrainError::InvalidRevision)
        );
    }

    #[test]
    fn captured_vesc_context_rejects_invalid_attempts_without_normalizing_identity() {
        for attempt in [
            json!({"generation": 0, "platform_identifier": "controller-a"}),
            json!({"generation": 1, "platform_identifier": "  "}),
            json!({"generation": 1, "platform_identifier": "a".repeat(513)}),
        ] {
            let mut context = context();
            context["connection_attempt"] = attempt;
            assert_eq!(
                record(&json!({"vesc_drivetrain_context": context})).captured_vesc_drivetrain(),
                CapturedVescDrivetrainEvidence::Invalid(
                    CapturedVescDrivetrainError::InvalidAttempt
                )
            );
        }
    }

    #[test]
    fn captured_vesc_context_reuses_geometry_validation_and_checks_circumference() {
        for (field, value, expected) in [
            (
                "wheel_diameter_micrometres",
                0,
                CapturedVescDrivetrainError::InvalidGeometry(VescDrivetrainError::WheelDiameter),
            ),
            (
                "motor_pole_pairs",
                0,
                CapturedVescDrivetrainError::InvalidGeometry(VescDrivetrainError::MotorPolePairs),
            ),
            (
                "motor_to_wheel_ratio_milli",
                0,
                CapturedVescDrivetrainError::InvalidGeometry(VescDrivetrainError::Reduction),
            ),
            (
                "wheel_circumference_mm",
                284,
                CapturedVescDrivetrainError::InconsistentCircumference,
            ),
        ] {
            let mut context = context();
            context[field] = json!(value);
            assert_eq!(
                record(&json!({"vesc_drivetrain_context": context})).captured_vesc_drivetrain(),
                CapturedVescDrivetrainEvidence::Invalid(expected)
            );
        }
    }

    #[test]
    fn captured_vesc_context_rejects_malformed_types_and_duplicate_contract_fields() {
        let mut malformed = context();
        malformed["revision"] = json!(3.5);
        let mut records = vec![record(&json!({"vesc_drivetrain_context": malformed}))];
        let mut duplicate = record(&json!({}));
        duplicate.semantic_telemetry.as_mut().unwrap().snapshot_json = format!(
            "{{\"vesc_drivetrain_context\":{},\"vesc_drivetrain_context\":{}}}",
            context(),
            context()
        );
        records.push(duplicate);
        for duplicate_context in [
            context()
                .to_string()
                .replace("\"revision\":3", "\"revision\":3,\"revision\":4"),
            context()
                .to_string()
                .replace("\"generation\":7", "\"generation\":7,\"generation\":8"),
        ] {
            let mut duplicate = record(&json!({}));
            duplicate.semantic_telemetry.as_mut().unwrap().snapshot_json =
                format!("{{\"vesc_drivetrain_context\":{duplicate_context}}}");
            records.push(duplicate);
        }
        let mut invalid_json = record(&json!({}));
        invalid_json
            .semantic_telemetry
            .as_mut()
            .unwrap()
            .snapshot_json = "{".to_owned();
        records.push(invalid_json);
        for record in records {
            let original = record.clone();
            assert_eq!(
                record.captured_vesc_drivetrain(),
                CapturedVescDrivetrainEvidence::Invalid(
                    CapturedVescDrivetrainError::MalformedSnapshot
                )
            );
            assert_eq!(record, original);
        }
    }

    #[test]
    fn captured_vesc_context_requires_named_json_objects() {
        let positional_context = json!([
            "local",
            [7, "controller-a"],
            3,
            "user_configured",
            90_000,
            283,
            7,
            2_500
        ]);
        let mut positional_attempt = context();
        positional_attempt["connection_attempt"] = json!([7, "controller-a"]);
        for snapshot in [
            json!([context()]),
            json!({"vesc_drivetrain_context": positional_context}),
            json!({"vesc_drivetrain_context": positional_attempt}),
        ] {
            let record = record(&snapshot);
            let original = record.clone();
            assert_eq!(
                record.captured_vesc_drivetrain(),
                CapturedVescDrivetrainEvidence::Invalid(
                    CapturedVescDrivetrainError::MalformedSnapshot
                ),
                "{snapshot}"
            );
            assert_eq!(record, original);
        }
    }

    #[test]
    fn captured_vesc_context_preserves_nonlocal_target_before_geometry_validation() {
        for malformed in [
            json!({"revision": "bad"}),
            json!({"connection_attempt": false}),
            json!({"motor_pole_pairs": -1}),
        ] {
            for target in [json!({"can": {"controller_id": 3}}), json!("future")] {
                let mut context = context();
                context["target"] = target;
                for (key, value) in malformed.as_object().unwrap() {
                    context[key] = value.clone();
                }
                let record = record(&json!({"vesc_drivetrain_context": context}));
                let original = record.clone();
                assert_eq!(
                    record.captured_vesc_drivetrain(),
                    CapturedVescDrivetrainEvidence::UnsupportedTarget
                );
                assert_eq!(record, original);
            }
        }
    }
}
