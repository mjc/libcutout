//! Bounded reconciliation of semantic telemetry already carried by a PEVCAP event.

use super::LIVE_CAPTURE_EVENT_LIMIT_BYTES;
use cutout_core::{PevcapSemanticTelemetry, PevcapTelemetryProvenance};
use serde::Deserialize;

#[derive(Deserialize)]
struct EventLine {
    kind: String,
    record: Option<RecordBody>,
}

#[derive(Deserialize)]
struct RecordBody {
    #[serde(default)]
    semantic_telemetry: Option<SemanticTelemetryJson>,
}

#[derive(Deserialize)]
struct SemanticTelemetryJson {
    observed_at_ms: Option<u64>,
    provenance: TelemetryProvenanceJson,
    snapshot_schema_version: u16,
    library_version: String,
    snapshot_json: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum TelemetryProvenanceJson {
    LiveSession,
    Replay,
}

/// Decodes semantic telemetry from a bounded record line without rewriting its JSON string.
pub(in crate::storage) fn event_telemetry(payload: &[u8]) -> Option<PevcapSemanticTelemetry> {
    if !(1..=LIVE_CAPTURE_EVENT_LIMIT_BYTES).contains(&payload.len()) {
        return None;
    }
    let line = serde_json::from_slice::<EventLine>(payload).ok()?;
    if line.kind != "record" {
        return None;
    }
    let telemetry = line.record?.semantic_telemetry?;
    let provenance = match telemetry.provenance {
        TelemetryProvenanceJson::LiveSession => PevcapTelemetryProvenance::LiveSession,
        TelemetryProvenanceJson::Replay => PevcapTelemetryProvenance::Replay,
    };
    Some(PevcapSemanticTelemetry {
        observed_at_ms: telemetry
            .observed_at_ms
            .map(cutout_core::MonotonicTimestamp::new),
        provenance,
        snapshot_schema_version: telemetry.snapshot_schema_version,
        library_version: telemetry.library_version,
        snapshot_json: telemetry.snapshot_json,
    })
    .filter(|telemetry| invalid_projection(telemetry).is_none())
}

/// Returns true only when a bounded canonical event carries the exact projection value.
pub(in crate::storage) fn event_matches_projection(
    payload: &[u8],
    projection: &PevcapSemanticTelemetry,
) -> bool {
    event_telemetry(payload).as_ref() == Some(projection)
}

/// Returns the violated stored-projection constraint, if any.
pub(in crate::storage) fn invalid_projection(
    telemetry: &PevcapSemanticTelemetry,
) -> Option<&'static str> {
    if telemetry.snapshot_schema_version == 0 {
        return Some("semantic telemetry schema version");
    }
    if !(1..=64).contains(&telemetry.library_version.len()) {
        return Some("semantic telemetry library version");
    }
    if telemetry
        .observed_at_ms
        .is_some_and(|timestamp| i64::try_from(timestamp.get()).is_err())
    {
        return Some("semantic telemetry timestamp");
    }
    if serde_json::from_str::<serde_json::Value>(&telemetry.snapshot_json).is_err() {
        return Some("semantic telemetry encoding");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record_line(snapshot: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "kind": "record",
            "record": {
                "semantic_telemetry": {
                    "observed_at_ms": 42,
                    "provenance": "live_session",
                    "snapshot_schema_version": 7,
                    "library_version": "libcutout-test",
                    "snapshot_json": snapshot,
                }
            }
        }))
        .unwrap()
    }

    fn telemetry(snapshot_json: &str) -> PevcapSemanticTelemetry {
        PevcapSemanticTelemetry {
            observed_at_ms: Some(cutout_core::MonotonicTimestamp::new(42)),
            provenance: PevcapTelemetryProvenance::LiveSession,
            snapshot_schema_version: 7,
            library_version: "libcutout-test".to_owned(),
            snapshot_json: snapshot_json.to_owned(),
        }
    }

    #[test]
    fn matching_preserves_exact_snapshot_spelling_and_all_metadata() {
        let snapshot = "{ \"speed\": 1.00, \"unknown\": [null] }";
        let payload = record_line(snapshot);
        assert!(event_matches_projection(&payload, &telemetry(snapshot)));
        assert_eq!(event_telemetry(&payload).unwrap().snapshot_json, snapshot);

        let mut different = telemetry(snapshot);
        different.library_version.push_str("-other");
        assert!(!event_matches_projection(&payload, &different));
        let mut rewritten = telemetry("{\"speed\":1.0,\"unknown\":[null]}");
        assert!(!event_matches_projection(&payload, &rewritten));
        rewritten.snapshot_json = snapshot.to_owned();
        rewritten.observed_at_ms = Some(cutout_core::MonotonicTimestamp::new(43));
        assert!(!event_matches_projection(&payload, &rewritten));
    }

    #[test]
    fn invalid_or_oversized_event_does_not_prove_projection_equivalence() {
        assert!(!event_matches_projection(b"X'0001FEFF'", &telemetry("{}")));
        assert!(!event_matches_projection(
            &vec![b' '; LIVE_CAPTURE_EVENT_LIMIT_BYTES + 1],
            &telemetry("{}")
        ));
        assert!(!event_matches_projection(
            br#"{"kind":"metadata","record":{"semantic_telemetry":{"observed_at_ms":42,"provenance":"live_session","snapshot_schema_version":7,"library_version":"libcutout-test","snapshot_json":"{}"}}}"#,
            &telemetry("{}")
        ));
    }

    #[test]
    fn event_telemetry_rejects_metadata_outside_storage_constraints() {
        let payload = record_line("{}");
        let mut event: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        let semantic = event["record"]["semantic_telemetry"].clone();
        for (field, value) in [
            ("snapshot_schema_version", serde_json::json!(0)),
            ("library_version", serde_json::json!("")),
            ("library_version", serde_json::json!("x".repeat(65))),
            ("observed_at_ms", serde_json::json!(i64::MAX as u64 + 1)),
        ] {
            let mut malformed = semantic.as_object().unwrap().clone();
            malformed.insert(field.to_owned(), value);
            event["record"]["semantic_telemetry"] = serde_json::Value::Object(malformed);
            assert!(event_telemetry(&serde_json::to_vec(&event).unwrap()).is_none());
        }
    }
}
