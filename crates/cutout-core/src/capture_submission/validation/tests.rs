use super::*;
use crate::{
    CaptureOrigin, CaptureSubmissionIntegrity, CaptureSubmissionProvenance, GattChannel,
    MonotonicTimestamp, PevcapCapture, PevcapHeader, PevcapLocationSample, PevcapPhoneLocation,
    PevcapRecord, WallClockUnixTimestamp, WriteMode,
};

const ARTIFACT: &str = "db240583-29ae-49e3-a101-ddd6878c34cc";
const LIVE: &str = "776fc772-f8d4-4b15-9100-904222052d0f";
const OPERATION: &str = "db0912ef-4c94-45c4-8121-5e43b886ba0f";

fn capture(annotations: &[&str]) -> PevcapCapture {
    let header = PevcapHeader::new(
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "unknown-peripheral",
        None,
        &[],
        &[],
        None,
        None,
        "test",
        [0; 32],
        annotations,
    )
    .unwrap();
    let notification = PevcapRecord::inbound_notification(
        MonotonicTimestamp::new(10),
        GattChannel::from_bytes([0x11; 16]),
        GattChannel::from_bytes([0x22; 16]),
        vec![0xff, 0x00, 0xa5],
    );
    let write = PevcapRecord::outbound_write(
        MonotonicTimestamp::new(20),
        GattChannel::from_bytes([0x33; 16]),
        WriteMode::WithoutResponse,
        vec![0xde, 0xad, 0xbe, 0xef],
    );
    PevcapCapture::new(header, vec![notification, write])
}

fn location() -> PevcapPhoneLocation {
    PevcapPhoneLocation {
        wall_clock_unix_ms: 1_700_000_000_015,
        latitude_degrees: 39.7,
        longitude_degrees: -104.9,
        altitude_meters: 1_600.0,
        horizontal_accuracy_meters: Some(2.0),
        vertical_accuracy_meters: None,
        speed_meters_per_second: None,
        speed_accuracy_meters_per_second: None,
        course_degrees: None,
        course_accuracy_degrees: None,
    }
}

fn metadata() -> CaptureSubmissionMetadata {
    CaptureSubmissionMetadata {
        sha256: "a".repeat(64),
        bytes: 123,
        encoding: PevcapEncoding::Jsonl,
        origin: CaptureOrigin::Automatic,
        integrity: CaptureSubmissionIntegrity::Complete,
        provenance: CaptureSubmissionProvenance {
            source_artifact_id: ARTIFACT.to_owned(),
            live_capture_id: Some(LIVE.to_owned()),
            source_sha256: None,
            transformation: CaptureSubmissionTransformation::DatabaseExportMusicExcluded,
        },
    }
}

#[test]
fn capture_submission_validates_unknown_gps_free_bytes_in_both_encodings() {
    let capture = capture(&["capture_label=ride_start", "capture_label=ride_stop"]);
    assert!(capture.header.resolved_identity.is_none());
    for encoding in [PevcapEncoding::Jsonl, PevcapEncoding::Binary] {
        let bytes = capture.encode(encoding).unwrap();
        let summary = validate_capture_submission(bytes.as_slice(), encoding).unwrap();
        assert_eq!(summary.sha256, format!("{:x}", Sha256::digest(&bytes)));
        assert_eq!(summary.bytes, bytes.len() as u64);
        assert_eq!(summary.events, 2);
        assert_eq!(summary.annotation_count, 2);
        assert_eq!(
            summary.categories,
            CaptureSubmissionCategories {
                device_identifiers: true,
                location: false,
                notes: false,
                annotations: true,
                settings_observations: false,
            }
        );
    }
}

#[test]
fn capture_submission_hashes_exact_jsonl_whitespace_and_opaque_fields() {
    let serialized = capture(&[]).to_jsonl().unwrap();
    let text = serialized.replacen(
        "\"record\":{",
        "\"opaque_future\": [1.00, 9e2], \"record\":{",
        1,
    );
    assert_ne!(text, serialized);
    let input = format!("\n  {}\n\n", text.trim_end());
    let summary = validate_capture_submission(input.as_bytes(), PevcapEncoding::Jsonl).unwrap();
    assert_eq!(
        summary.sha256,
        format!("{:x}", Sha256::digest(input.as_bytes()))
    );
    assert_eq!(summary.bytes, input.len() as u64);
    assert_eq!(summary.events, 2);
    assert!(!summary.categories.annotations);
}

fn inject_music_field(
    capture: &PevcapCapture,
    encoding: PevcapEncoding,
    music: serde_json::Value,
) -> Vec<u8> {
    match encoding {
        PevcapEncoding::Jsonl => {
            let input = capture.to_jsonl().unwrap();
            let mut lines: Vec<serde_json::Value> = input
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            lines[1]["record"]["music"] = music;
            let mut output = Vec::new();
            for line in lines {
                serde_json::to_writer(&mut output, &line).unwrap();
                output.push(b'\n');
            }
            output
        }
        PevcapEncoding::Binary => {
            let mut bytes = capture.to_binary().unwrap();
            let header_size = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
            let record_length_offset = 16 + header_size + 4;
            let payload_offset = record_length_offset + 4;
            let payload_size = u32::from_le_bytes(
                bytes[record_length_offset..payload_offset]
                    .try_into()
                    .unwrap(),
            ) as usize;
            let mut record: serde_json::Value =
                serde_json::from_slice(&bytes[payload_offset..payload_offset + payload_size])
                    .unwrap();
            record["music"] = music;
            let payload = serde_json::to_vec(&record).unwrap();
            bytes[record_length_offset..payload_offset]
                .copy_from_slice(&u32::try_from(payload.len()).unwrap().to_le_bytes());
            bytes.splice(payload_offset..payload_offset + payload_size, payload);
            bytes
        }
    }
}

#[test]
fn capture_submission_rejects_embedded_music_even_null_in_both_encodings() {
    let capture = capture(&[]);
    for music in [
        serde_json::Value::Null,
        serde_json::json!({"track_id": "private"}),
    ] {
        for encoding in [PevcapEncoding::Jsonl, PevcapEncoding::Binary] {
            let bytes = inject_music_field(&capture, encoding, music.clone());
            assert_eq!(
                validate_capture_submission(bytes.as_slice(), encoding),
                Err(CaptureSubmissionValidationError::MusicPresent)
            );
        }
    }
}

#[test]
fn capture_submission_rejects_music_event_tags_as_invalid_current_format() {
    let mut jsonl = capture(&[]).to_jsonl().unwrap();
    jsonl.push_str("{\"kind\":\"music\",\"music\":{\"track_id\":\"private\"}}\n");
    assert_eq!(
        validate_capture_submission(jsonl.as_bytes(), PevcapEncoding::Jsonl),
        Err(CaptureSubmissionValidationError::InvalidPevcap)
    );
}

#[test]
fn capture_submission_discloses_native_description_and_user_notes() {
    for annotation in [
        "capture_description=Unverified wheel",
        "user_note=Observed physical action",
    ] {
        let capture = capture(&[annotation]);
        for encoding in [PevcapEncoding::Jsonl, PevcapEncoding::Binary] {
            let bytes = capture.encode(encoding).unwrap();
            let summary = validate_capture_submission(bytes.as_slice(), encoding).unwrap();
            assert!(
                summary.categories.notes,
                "actual capture annotation must be disclosed: {annotation}"
            );
            assert!(summary.categories.annotations);
        }
    }
}

#[test]
fn capture_submission_discloses_independent_and_embedded_location() {
    for embedded in [false, true] {
        let mut capture = capture(&[]);
        if embedded {
            capture.records.first_mut().unwrap().phone_location = Some(location());
        } else {
            capture.locations.push(
                PevcapLocationSample::new(MonotonicTimestamp::new(15), location(), None, None)
                    .unwrap(),
            );
        }
        for encoding in [PevcapEncoding::Jsonl, PevcapEncoding::Binary] {
            let bytes = capture.encode(encoding).unwrap();
            let summary = validate_capture_submission(bytes.as_slice(), encoding).unwrap();
            assert!(summary.categories.location);
            assert_eq!(summary.events, if embedded { 2 } else { 3 });
        }
    }
}

#[test]
fn capture_submission_rejects_malformed_truncated_and_trailing_binary_data() {
    for encoding in [PevcapEncoding::Jsonl, PevcapEncoding::Binary] {
        assert_eq!(
            validate_capture_submission(b"not a capture".as_slice(), encoding),
            Err(CaptureSubmissionValidationError::InvalidPevcap)
        );
        let mut bytes = capture(&[]).encode(encoding).unwrap();
        bytes.truncate(bytes.len() - 2);
        assert_eq!(
            validate_capture_submission(bytes.as_slice(), encoding),
            Err(CaptureSubmissionValidationError::InvalidPevcap)
        );
    }
    let mut binary = capture(&[]).encode(PevcapEncoding::Binary).unwrap();
    binary.push(0xff);
    assert_eq!(
        validate_capture_submission(binary.as_slice(), PevcapEncoding::Binary),
        Err(CaptureSubmissionValidationError::InvalidPevcap)
    );
}

#[test]
fn capture_submission_distinguishes_unsupported_container_from_unknown_device() {
    let capture = capture(&[]);
    let jsonl = capture.to_jsonl().unwrap();
    let mut lines = jsonl.lines();
    let mut header: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    header["version"]["major"] = serde_json::json!(99);
    let unsupported = format!("{}\n{}\n", header, lines.collect::<Vec<_>>().join("\n"));
    assert_eq!(
        validate_capture_submission(unsupported.as_bytes(), PevcapEncoding::Jsonl),
        Err(CaptureSubmissionValidationError::UnsupportedFormat)
    );
    let mut binary = capture.to_binary().unwrap();
    binary
        .get_mut(8..10)
        .unwrap()
        .copy_from_slice(&99_u16.to_le_bytes());
    assert_eq!(
        validate_capture_submission(binary.as_slice(), PevcapEncoding::Binary),
        Err(CaptureSubmissionValidationError::UnsupportedFormat)
    );
}

#[test]
fn capture_submission_rejects_header_annotation_overflow() {
    let jsonl = capture(&[]).to_jsonl().unwrap();
    let mut lines = jsonl.lines();
    let mut header: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    header["header"]["annotations"] = serde_json::json!(vec![
        "capture_label=ride_start";
        crate::PEVCAP_MAX_ANNOTATIONS + 1
    ]);
    let overflowing = format!("{}\n{}\n", header, lines.collect::<Vec<_>>().join("\n"));
    assert_eq!(
        validate_capture_submission(overflowing.as_bytes(), PevcapEncoding::Jsonl),
        Err(CaptureSubmissionValidationError::InvalidPevcap)
    );
}

struct FailingReader<'a> {
    bytes: &'a [u8],
    remaining: usize,
}
impl Read for FailingReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.remaining == 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "private fixture detail",
            ));
        }
        let take = self.remaining.min(buffer.len()).min(self.bytes.len());
        let (prefix, rest) = self.bytes.split_at(take);
        buffer.get_mut(..take).unwrap().copy_from_slice(prefix);
        self.bytes = rest;
        self.remaining -= take;
        Ok(take)
    }
}

#[test]
fn capture_submission_classifies_source_read_failures_without_private_error_text() {
    for encoding in [PevcapEncoding::Jsonl, PevcapEncoding::Binary] {
        let bytes = capture(&[]).encode(encoding).unwrap();
        for remaining in [0, 8, bytes.len()] {
            let error = validate_capture_submission(
                FailingReader {
                    bytes: &bytes,
                    remaining,
                },
                encoding,
            )
            .unwrap_err();
            assert_eq!(error, CaptureSubmissionValidationError::ReadFailure);
            assert!(!error.to_string().contains("private fixture detail"));
        }
    }
}

#[test]
fn capture_submission_enforces_duration_for_binary_and_out_of_order_jsonl() {
    for reverse in [false, true] {
        let mut capture = capture(&[]);
        capture.records.first_mut().unwrap().monotonic_ms = MonotonicTimestamp::new(1);
        capture.records.last_mut().unwrap().monotonic_ms =
            MonotonicTimestamp::new(MAX_CAPTURE_SUBMISSION_DURATION_MILLISECONDS + 2);
        if reverse {
            capture.records.reverse();
        }
        for encoding in [PevcapEncoding::Jsonl, PevcapEncoding::Binary] {
            let bytes = capture.encode(encoding).unwrap();
            assert_eq!(
                validate_capture_submission(bytes.as_slice(), encoding),
                Err(CaptureSubmissionValidationError::DurationTooLong)
            );
        }
    }
}

#[test]
fn capture_submission_metadata_preserves_truthful_original_and_derivative_identity() {
    let mut metadata = metadata();
    assert_eq!(metadata.validate(), Ok(()));
    metadata.integrity = CaptureSubmissionIntegrity::Incomplete {
        dropped_messages: 3,
    };
    assert_eq!(metadata.validate(), Ok(()));
    metadata.provenance.live_capture_id = None;
    assert_eq!(
        metadata.validate(),
        Err(CaptureSubmissionValidationError::InvalidMetadata)
    );
    metadata.provenance.transformation = CaptureSubmissionTransformation::Unmodified;
    metadata.provenance.source_sha256 = Some(metadata.sha256.clone());
    assert_eq!(metadata.validate(), Ok(()));
    metadata.provenance.transformation = CaptureSubmissionTransformation::MusicRemoved;
    assert_eq!(
        metadata.validate(),
        Err(CaptureSubmissionValidationError::InvalidMetadata)
    );
    metadata.provenance.source_sha256 = Some("b".repeat(64));
    assert_eq!(metadata.validate(), Ok(()));
    metadata.provenance.transformation = CaptureSubmissionTransformation::Unmodified;
    assert_eq!(
        metadata.validate(),
        Err(CaptureSubmissionValidationError::InvalidMetadata)
    );
}

#[test]
fn capture_submission_metadata_rejects_invalid_ids_digests_and_sizes() {
    let original = metadata();
    for bad in ["", "not-an-id", "DB240583-29AE-49E3-A101-DDD6878C34CC"] {
        let mut metadata = original.clone();
        metadata.provenance.source_artifact_id = bad.to_owned();
        assert_eq!(
            metadata.validate(),
            Err(CaptureSubmissionValidationError::InvalidMetadata)
        );
    }
    for bad in [
        String::new(),
        "A".repeat(64),
        "g".repeat(64),
        "a".repeat(63),
    ] {
        let mut metadata = original.clone();
        metadata.sha256 = bad;
        assert_eq!(
            metadata.validate(),
            Err(CaptureSubmissionValidationError::InvalidMetadata)
        );
    }
    let mut metadata = original;
    metadata.bytes = 0;
    assert_eq!(
        metadata.validate(),
        Err(CaptureSubmissionValidationError::InvalidMetadata)
    );
    metadata.bytes = MAX_CAPTURE_SUBMISSION_BYTES + 1;
    assert_eq!(
        metadata.validate(),
        Err(CaptureSubmissionValidationError::TooLarge)
    );
}

#[test]
fn capture_submission_receipt_must_match_operation_and_exact_snapshot() {
    let metadata = metadata();
    let receipt = CaptureSubmissionReceipt {
        operation_id: OPERATION.to_owned(),
        sha256: metadata.sha256.clone(),
        bytes: metadata.bytes,
        received_at_milliseconds: 1_700_000_001_000,
    };
    assert_eq!(receipt.validate_for(OPERATION, &metadata), Ok(()));
    assert_eq!(
        receipt.validate_for(ARTIFACT, &metadata),
        Err(CaptureSubmissionValidationError::InvalidMetadata)
    );
    let mut changed = receipt.clone();
    changed.sha256 = "b".repeat(64);
    assert_eq!(
        changed.validate_for(OPERATION, &metadata),
        Err(CaptureSubmissionValidationError::InvalidMetadata)
    );
    changed = receipt;
    changed.bytes += 1;
    assert_eq!(
        changed.validate_for(OPERATION, &metadata),
        Err(CaptureSubmissionValidationError::InvalidMetadata)
    );
}

#[test]
fn capture_submission_classifies_bounded_parser_payload_limits() {
    let mut binary = capture(&[]).to_binary().unwrap();
    binary[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        validate_capture_submission(binary.as_slice(), PevcapEncoding::Binary),
        Err(CaptureSubmissionValidationError::TooLarge)
    );
    let oversized_line = vec![b' '; 1024 * 1024 + 1];
    assert_eq!(
        validate_capture_submission(oversized_line.as_slice(), PevcapEncoding::Jsonl),
        Err(CaptureSubmissionValidationError::TooLarge)
    );
}

#[test]
fn capture_submission_preserves_settings_notes_and_still_rejects_music() {
    use crate::{
        CaptureObservationSource, CaptureObservedValue, CaptureResearchProfile,
        CaptureResearchProfileSelection, CaptureSettingObservationValue,
        CaptureSettingsObservation,
    };
    let mut capture = capture(&[]);
    let observation = CaptureSettingsObservation {
        capture_id: LIVE.into(),
        profile: CaptureResearchProfile {
            profile_id: "manual_unknown_device".into(),
            pev_category: Some("euc".into()),
            model: None,
            firmware: None,
            selection: CaptureResearchProfileSelection::Manual,
        },
        observed_at_ms: 5,
        recorded_at_ms: 30,
        source: CaptureObservationSource::ManufacturerApp,
        values: vec![CaptureSettingObservationValue {
            setting_id: "display_brightness".into(),
            value: CaptureObservedValue::Entered("125".into()),
            unit: Some("%".into()),
            note: Some("as shown".into()),
        }],
        note: None,
    };
    capture.settings_observations.push(observation);
    for encoding in [PevcapEncoding::Jsonl, PevcapEncoding::Binary] {
        let bytes = capture.encode(encoding).unwrap();
        let summary = validate_capture_submission(bytes.as_slice(), encoding).unwrap();
        assert_eq!(summary.events, 3);
        assert!(summary.categories.notes);
        assert!(!summary.categories.annotations);
        assert!(summary.categories.settings_observations);
        assert!(!summary.categories.location);
    }
    for encoding in [PevcapEncoding::Jsonl, PevcapEncoding::Binary] {
        let bytes = inject_music_field(&capture, encoding, serde_json::Value::Null);
        assert_eq!(
            validate_capture_submission(bytes.as_slice(), encoding),
            Err(CaptureSubmissionValidationError::MusicPresent)
        );
    }
}

#[test]
fn capture_submission_categories_read_legacy_persisted_status() {
    let categories: CaptureSubmissionCategories = serde_json::from_str(
        r#"{"device_identifiers":true,"location":false,"notes":false,"annotations":true}"#,
    )
    .unwrap();
    assert!(!categories.settings_observations);
    assert!(categories.device_identifiers);
    assert!(categories.annotations);
}

fn settings_count_submission(encoding: PevcapEncoding, count: usize) -> Vec<u8> {
    let observation: crate::CaptureSettingsObservation = serde_json::from_value(serde_json::json!({
        "capture_id":LIVE,
        "profile":{"profile_id":"manual_unknown_device","pev_category":"euc","model":null,"firmware":null,"selection":"manual"},
        "observed_at_ms":5,"recorded_at_ms":30,"source":"manufacturer_app",
        "values":[{"setting_id":"display_brightness","value":{"kind":"entered","value":"125"},"unit":"%","note":"as shown"}],
        "note":null
    })).unwrap();
    let mut capture = capture(&[]);
    capture.version = crate::PevcapFormatVersion { major: 1, minor: 5 };
    let mut bytes = capture.encode(encoding).unwrap();
    match encoding {
        PevcapEncoding::Jsonl => {
            let line = observation.to_jsonl_line().unwrap();
            for _ in 0..count {
                bytes.extend_from_slice(line.as_bytes());
                bytes.push(b'\n');
            }
        }
        PevcapEncoding::Binary => {
            bytes.truncate(bytes.len() - 4);
            bytes.extend_from_slice(&u32::try_from(count).unwrap().to_le_bytes());
            let payload = serde_json::to_vec(&observation).unwrap();
            let length = u32::try_from(payload.len()).unwrap().to_le_bytes();
            for _ in 0..count {
                bytes.extend_from_slice(&length);
                bytes.extend_from_slice(&payload);
            }
        }
    }
    bytes
}

#[test]
fn capture_submission_settings_count_limit_preserves_too_many_events_classification() {
    let limit = crate::MAX_CAPTURE_SETTINGS_OBSERVATIONS;
    for encoding in [PevcapEncoding::Jsonl, PevcapEncoding::Binary] {
        let exact = settings_count_submission(encoding, limit);
        let accepted = validate_capture_submission(exact.as_slice(), encoding).unwrap();
        assert_eq!(accepted.events, u64::try_from(limit).unwrap() + 2);
        assert!(accepted.categories.settings_observations);
        let excessive = settings_count_submission(encoding, limit + 1);
        assert_eq!(
            validate_capture_submission(excessive.as_slice(), encoding),
            Err(CaptureSubmissionValidationError::TooManyEvents)
        );
    }
}

#[test]
fn capture_submission_settings_disclosure_matches_retained_note_fields_and_header_annotations() {
    let cases = [
        (None, None, Vec::new(), false, false),
        (Some("snapshot note"), None, Vec::new(), true, false),
        (None, Some("value note"), Vec::new(), true, false),
        (Some(""), Some(""), Vec::new(), true, false),
        (Some(" \t"), Some("\n "), Vec::new(), true, false),
        (None, None, vec!["capture_label=ride_start"], false, true),
        (None, None, vec!["note=retained header note"], true, true),
    ];
    for (snapshot_note, value_note, annotations, notes, labels) in cases {
        let observation: crate::CaptureSettingsObservation =
            serde_json::from_value(serde_json::json!({
                "capture_id":LIVE,
                "profile":{"profile_id":"manual_unknown_device","pev_category":"euc","model":null,"firmware":null,"selection":"manual"},
                "observed_at_ms":5,"recorded_at_ms":30,"source":"manufacturer_app",
                "values":[{"setting_id":"display_brightness","value":{"kind":"entered","value":"125"},"unit":"%","note":value_note}],
                "note":snapshot_note
            })).unwrap();
        let mut capture = capture(&annotations);
        capture.settings_observations.push(observation);
        for encoding in [PevcapEncoding::Jsonl, PevcapEncoding::Binary] {
            let bytes = capture.encode(encoding).unwrap();
            let summary = validate_capture_submission(bytes.as_slice(), encoding).unwrap();
            assert_eq!(summary.categories.notes, notes);
            assert_eq!(summary.categories.annotations, labels);
            assert!(summary.categories.settings_observations);
            assert_eq!(
                summary.annotation_count,
                u64::try_from(annotations.len()).unwrap()
            );
            assert_eq!(summary.sha256, format!("{:x}", Sha256::digest(&bytes)));
            let decoded = PevcapCapture::decode(&bytes, encoding).unwrap();
            let retained = decoded.settings_observations.first().unwrap();
            assert_eq!(retained.note, snapshot_note.map(str::to_owned));
            assert_eq!(
                retained.values.first().unwrap().note,
                value_note.map(str::to_owned)
            );
            assert_eq!(decoded.encode(encoding).unwrap(), bytes);
        }
    }
}
