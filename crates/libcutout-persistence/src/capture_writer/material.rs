//! Consecutive material comparison; a candidate becomes a baseline only after storage commits.

use super::{CaptureRecordAdmission, CaptureRecordingPolicy};
use cutout_core::{MonotonicTimestamp, PevcapDirection, PevcapRecord};
use serde::{
    Deserialize, Deserializer,
    de::{Error as _, MapAccess, Visitor},
};
use serde_json::value::RawValue;
use std::{collections::BTreeMap, fmt};

pub(super) struct PreparedMaterialRecord(Box<PevcapRecord>);
struct CommittedMaterialRecord(Box<PevcapRecord>);

pub(super) enum MaterialDecision {
    Retain(Option<PreparedMaterialRecord>),
    Suppress,
}

pub(super) struct MaterialBaseline {
    policy: CaptureRecordingPolicy,
    committed: Option<CommittedMaterialRecord>,
}

impl MaterialBaseline {
    pub(super) const fn new(policy: CaptureRecordingPolicy) -> Self {
        Self {
            policy,
            committed: None,
        }
    }

    pub(super) fn reset(&mut self) {
        self.committed = None;
    }

    pub(super) fn prepare(
        &mut self,
        record: &PevcapRecord,
        admission: CaptureRecordAdmission,
    ) -> MaterialDecision {
        let candidate = match (self.policy, admission) {
            (
                CaptureRecordingPolicy::MaterialChanges,
                CaptureRecordAdmission::StationaryTelemetry,
            ) => material_record(record),
            (CaptureRecordingPolicy::EveryObservation, _)
            | (_, CaptureRecordAdmission::EveryObservation) => None,
        };
        let Some(candidate) = candidate else {
            self.reset();
            return MaterialDecision::Retain(None);
        };
        if self
            .committed
            .as_ref()
            .is_some_and(|committed| committed.0 == candidate.0)
        {
            MaterialDecision::Suppress
        } else {
            MaterialDecision::Retain(Some(candidate))
        }
    }

    pub(super) fn commit(&mut self, prepared: Option<PreparedMaterialRecord>) {
        self.committed = prepared.map(|prepared| CommittedMaterialRecord(prepared.0));
    }
}

fn material_record(record: &PevcapRecord) -> Option<PreparedMaterialRecord> {
    if record.direction != PevcapDirection::Inbound
        || record.music.is_some()
        || record.phone_location.is_some()
    {
        return None;
    }
    let semantic = record.semantic_telemetry.as_ref()?;
    if semantic.snapshot_json.len() > crate::storage::LIVE_CAPTURE_EVENT_LIMIT_BYTES {
        return None;
    }
    let mut snapshot: RawObject = serde_json::from_str(&semantic.snapshot_json).ok()?;
    let object = &mut snapshot.0;
    // Only these documented root observation clocks are excluded. Presence and null remain material.
    for key in ["at_ms", "speed_observed_at_ms"] {
        if let Some(value) = object.get_mut(key) {
            normalize_observation_clock(value)?;
        }
    }
    let mut material = record.clone();
    material.monotonic_ms = MonotonicTimestamp::new(0);
    let semantic = material.semantic_telemetry.as_mut()?;
    semantic.observed_at_ms = semantic.observed_at_ms.map(|_| MonotonicTimestamp::new(0));
    semantic.snapshot_json = serde_json::to_string(&snapshot.0).ok()?;
    Some(PreparedMaterialRecord(Box::new(material)))
}

/// Maps raw JSON members without coercing unknown numeric values or accepting ambiguous keys.
struct RawObject(BTreeMap<String, Box<RawValue>>);

impl<'de> Deserialize<'de> for RawObject {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(RawObjectVisitor)
    }
}

struct RawObjectVisitor;

impl<'de> Visitor<'de> for RawObjectVisitor {
    type Value = RawObject;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON object with unique keys")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut fields = BTreeMap::new();
        while let Some((key, value)) = map.next_entry::<String, Box<RawValue>>()? {
            if fields.insert(key, value).is_some() {
                return Err(A::Error::custom("duplicate material comparison key"));
            }
        }
        Ok(RawObject(fields))
    }
}

fn normalize_observation_clock(value: &mut Box<RawValue>) -> Option<()> {
    if value.get() == "null" {
        return Some(());
    }
    if serde_json::from_str::<u64>(value.get()).is_ok() {
        *value = RawValue::from_string("0".into()).ok()?;
        return Some(());
    }
    // The mobile DTO wraps its timestamp. All unknown wrapper values remain verbatim.
    let mut wrapper: RawObject = serde_json::from_str(value.get()).ok()?;
    let milliseconds = wrapper.0.get_mut("milliseconds")?;
    serde_json::from_str::<u64>(milliseconds.get()).ok()?;
    *milliseconds = RawValue::from_string("0".into()).ok()?;
    *value = RawValue::from_string(serde_json::to_string(&wrapper.0).ok()?).ok()?;
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture_writer::tests::stationary_record;

    #[test]
    fn an_uncommitted_candidate_cannot_suppress_the_next_observation() {
        let mut baseline = MaterialBaseline::new(CaptureRecordingPolicy::MaterialChanges);
        assert!(matches!(
            baseline.prepare(
                &stationary_record(1),
                CaptureRecordAdmission::StationaryTelemetry
            ),
            MaterialDecision::Retain(Some(_))
        ));
        let MaterialDecision::Retain(candidate) = baseline.prepare(
            &stationary_record(2),
            CaptureRecordAdmission::StationaryTelemetry,
        ) else {
            panic!("uncommitted event was suppressed");
        };
        baseline.commit(candidate);
        assert!(matches!(
            baseline.prepare(
                &stationary_record(3),
                CaptureRecordAdmission::StationaryTelemetry
            ),
            MaterialDecision::Suppress
        ));
    }
    fn committed_baseline() -> MaterialBaseline {
        let mut baseline = MaterialBaseline::new(CaptureRecordingPolicy::MaterialChanges);
        let MaterialDecision::Retain(candidate) = baseline.prepare(
            &stationary_record(1),
            CaptureRecordAdmission::StationaryTelemetry,
        ) else {
            panic!("first event was suppressed");
        };
        baseline.commit(candidate);
        baseline
    }

    #[test]
    fn decoded_raw_bits_and_semantic_contract_versions_remain_material() {
        let mut baseline = committed_baseline();
        let mut raw = stationary_record(2);
        raw.telemetry = Some(
            serde_json::from_str(
                r#"{"fields":[],"float_fields":[{"id":91,"value_bits":2143294004}]}"#,
            )
            .unwrap(),
        );
        let mut schema = stationary_record(3);
        schema
            .semantic_telemetry
            .as_mut()
            .unwrap()
            .snapshot_schema_version = 2;
        let mut library = stationary_record(4);
        library.semantic_telemetry.as_mut().unwrap().library_version = "other".into();
        for record in [raw, schema, library] {
            assert!(matches!(
                baseline.prepare(&record, CaptureRecordAdmission::StationaryTelemetry),
                MaterialDecision::Retain(Some(_))
            ));
        }
    }

    #[test]
    fn invalid_semantics_and_attached_music_break_the_baseline() {
        let mut invalid = stationary_record(2);
        invalid.semantic_telemetry.as_mut().unwrap().snapshot_json = "[]".into();
        let mut missing = stationary_record(3);
        missing.semantic_telemetry = None;
        let mut music = stationary_record(4);
        music.music = Some(
            cutout_core::PevcapMusicEvent::new(
                cutout_core::MusicProvider::Spotify,
                "spotify:track:test",
                MonotonicTimestamp::new(4),
                cutout_core::WallClockUnixTimestamp::new(104),
                0,
                None,
            )
            .unwrap(),
        );
        for record in [invalid, missing, music] {
            let mut baseline = committed_baseline();
            assert!(matches!(
                baseline.prepare(&record, CaptureRecordAdmission::StationaryTelemetry),
                MaterialDecision::Retain(None)
            ));
            assert!(matches!(
                baseline.prepare(
                    &stationary_record(5),
                    CaptureRecordAdmission::StationaryTelemetry
                ),
                MaterialDecision::Retain(Some(_))
            ));
        }
    }
    #[test]
    fn mobile_clock_wrappers_preserve_unknown_fields_and_presence() {
        let mut first = stationary_record(1);
        first.semantic_telemetry.as_mut().unwrap().snapshot_json =
            r#"{"at_ms":{"milliseconds":1,"other":7},"speed_observed_at_ms":{"milliseconds":1},"speed":0}"#.into();
        let mut second = stationary_record(2);
        second.semantic_telemetry.as_mut().unwrap().snapshot_json =
            r#"{"at_ms":{"milliseconds":2,"other":7},"speed_observed_at_ms":{"milliseconds":2},"speed":0}"#.into();
        let mut baseline = MaterialBaseline::new(CaptureRecordingPolicy::MaterialChanges);
        let MaterialDecision::Retain(candidate) =
            baseline.prepare(&first, CaptureRecordAdmission::StationaryTelemetry)
        else {
            panic!("first event was suppressed");
        };
        baseline.commit(candidate);
        assert!(matches!(
            baseline.prepare(&second, CaptureRecordAdmission::StationaryTelemetry),
            MaterialDecision::Suppress
        ));
        second.semantic_telemetry.as_mut().unwrap().snapshot_json =
            r#"{"at_ms":{"milliseconds":2,"other":8},"speed_observed_at_ms":{"milliseconds":2},"speed":0}"#.into();
        assert!(matches!(
            baseline.prepare(&second, CaptureRecordAdmission::StationaryTelemetry),
            MaterialDecision::Retain(Some(_))
        ));
    }
    #[test]
    fn unknown_numeric_lexemes_are_material_without_f64_rounding() {
        for (first_value, second_value) in [
            ("184467440737095516160", "184467440737095516161"),
            ("0.123456789012345678901", "0.123456789012345678902"),
        ] {
            let mut first = stationary_record(1);
            first.semantic_telemetry.as_mut().unwrap().snapshot_json =
                format!(r#"{{"at_ms":1,"unknown":{first_value}}}"#);
            let mut second = stationary_record(2);
            second.semantic_telemetry.as_mut().unwrap().snapshot_json =
                format!(r#"{{"at_ms":2,"unknown":{second_value}}}"#);
            let mut baseline = MaterialBaseline::new(CaptureRecordingPolicy::MaterialChanges);
            let MaterialDecision::Retain(candidate) =
                baseline.prepare(&first, CaptureRecordAdmission::StationaryTelemetry)
            else {
                panic!("first event suppressed");
            };
            baseline.commit(candidate);
            assert!(
                matches!(
                    baseline.prepare(&second, CaptureRecordAdmission::StationaryTelemetry),
                    MaterialDecision::Retain(Some(_))
                ),
                "unknown JSON precision changed"
            );
        }
    }

    #[test]
    fn duplicate_comparison_object_keys_never_qualify_for_suppression() {
        for json in [
            r#"{"at_ms":1,"unknown":1,"unknown":2}"#,
            r#"{"at_ms":{"milliseconds":1,"milliseconds":2},"unknown":1}"#,
        ] {
            let mut record = stationary_record(1);
            record.semantic_telemetry.as_mut().unwrap().snapshot_json = json.into();
            let mut baseline = MaterialBaseline::new(CaptureRecordingPolicy::MaterialChanges);
            assert!(matches!(
                baseline.prepare(&record, CaptureRecordAdmission::StationaryTelemetry),
                MaterialDecision::Retain(None)
            ));
        }
    }
}
