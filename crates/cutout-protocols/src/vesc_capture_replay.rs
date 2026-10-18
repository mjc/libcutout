//! Passive VESC capture interpretation, isolated from live transport and authority.

use crate::{ReadOnlySession, VescGenericModel};
use cutout_core::{
    CapturedVescDrivetrainContext, CapturedVescDrivetrainEvidence, HostSession, LinkInfo,
    MonotonicTimestamp, PevcapDirection, PevcapRecord, PevcapReplayRecord, PevcapSemanticTelemetry,
    PevcapTelemetryProvenance, ProtocolSession, SessionInput, SessionOutput, TelemetrySnapshot,
    VescDrivetrain, VescSpeedGeometry,
};

/// An offline calculation choice, never a live device profile or control grant.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum VescReplayGeometryPolicy {
    /// Use each record's checked context; raw controller geometry remains separately attributable.
    #[default]
    Captured,
    /// Reinterpret raw local bytes with other geometry, without a saved revision.
    Explicit(VescDrivetrain),
}

/// Calculation basis for the retained local replay projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VescReplayGeometryBasis {
    /// No checked local calculation context was available.
    Unknown,
    /// A historical user profile, distinct from raw device evidence.
    Captured(CapturedVescDrivetrainContext),
    /// An explicit offline reinterpretation, without an invented revision.
    Explicit(VescDrivetrain),
    /// Geometry decoded from this connection's original controller configuration bytes.
    ProtocolEvidence(VescSpeedGeometry),
}

impl VescReplayGeometryBasis {
    fn user_value(&self) -> Option<VescDrivetrain> {
        match self {
            Self::Captured(context) => Some(context.profile().value()),
            Self::Explicit(value) => Some(*value),
            Self::Unknown | Self::ProtocolEvidence(_) => None,
        }
    }
    fn geometry(&self) -> Option<VescSpeedGeometry> {
        match self {
            Self::ProtocolEvidence(value) => Some(*value),
            other => other.user_value().map(VescDrivetrain::geometry),
        }
    }
}

/// Why an original record cannot update the current local replay state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VescReplayRejection {
    /// A local replay cannot attribute another controller's bytes.
    UnsupportedTarget,
    /// The characteristic belongs to a different input stream.
    WrongCharacteristic,
    /// There has been no link-up, or the capture explicitly closed its link.
    Disconnected,
    /// A previously retired attempt cannot become current without a new link boundary.
    RetiredAttempt,
    /// The stored profile revision predates an accepted revision for this attempt.
    StaleRevision,
    /// One revision names different canonical geometry.
    ConflictingRevision,
    /// Receipt time regressed within the current link.
    OutOfOrder,
}

/// Admission of the latest original record, separate from previously retained telemetry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VescReplayRecordDisposition {
    /// The local decoder may consume this record.
    Accepted,
    /// Evidence is retained but cannot mutate local telemetry or partial frames.
    Ignored(VescReplayRejection),
}

/// Replay projection and the unchanged original semantic observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VescReplayObservation {
    /// Freshly decoded or retained telemetry, with original per-field clocks.
    pub telemetry: TelemetrySnapshot,
    /// Calculation context for retained local telemetry.
    pub geometry: VescReplayGeometryBasis,
    /// Checked evidence from the latest record, independent of an explicit override.
    pub captured_geometry: CapturedVescDrivetrainEvidence,
    /// Original JSON, provenance, version and observation time; never rewritten.
    pub original: Option<PevcapSemanticTelemetry>,
    /// An ignored record does not refresh the retained measurements.
    pub disposition: VescReplayRecordDisposition,
}

impl VescReplayObservation {
    /// Derived replay is never promoted to a live observation.
    #[must_use]
    pub const fn provenance(&self) -> PevcapTelemetryProvenance {
        PevcapTelemetryProvenance::Replay
    }
    /// Current interpreting library, distinct from the original snapshot's library version.
    #[must_use]
    pub const fn library_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
    /// Current shared model registry, distinct from the capture header's original hash.
    #[must_use]
    pub fn registry_hash(&self) -> [u8; 32] {
        cutout_core::registry_entries_hash(&crate::MODEL_REGISTRY)
    }
    /// Source fingerprint of the interpreting protocol/core build inputs.
    #[must_use]
    pub const fn decoder_source_hash(&self) -> &'static str {
        env!("CUTOUT_REPLAY_DECODER_SOURCE_SHA256")
    }
}

/// Passive reactor that cannot return executable transport effects or accept commands.
#[derive(Clone, Debug)]
pub struct VescCaptureReplaySession {
    inner: ReadOnlySession<VescGenericModel>,
    policy: VescReplayGeometryPolicy,
    geometry: VescReplayGeometryBasis,
    captured_geometry: CapturedVescDrivetrainEvidence,
    original: Option<PevcapSemanticTelemetry>,
    scope: Option<CapturedVescDrivetrainContext>,
    last_receipt: Option<MonotonicTimestamp>,
    disposition: VescReplayRecordDisposition,
}

impl VescCaptureReplaySession {
    /// Starts an offline interpretation with no database or live-session reference.
    #[must_use]
    pub fn new(policy: VescReplayGeometryPolicy) -> Self {
        Self {
            inner: ReadOnlySession::default(),
            policy,
            geometry: VescReplayGeometryBasis::Unknown,
            captured_geometry: CapturedVescDrivetrainEvidence::Missing,
            original: None,
            scope: None,
            last_receipt: None,
            disposition: VescReplayRecordDisposition::Accepted,
        }
    }

    /// Supplies both record boundaries to the existing PEVCAP driver.
    pub fn observe_record(boundary: PevcapReplayRecord<'_>, host: &mut HostSession<Self>) {
        match boundary {
            PevcapReplayRecord::Before(record) => Self::prepare_record(record, host),
            PevcapReplayRecord::After(record) => Self::finish_record(record, host),
        }
    }

    fn reject(
        &self,
        record: &PevcapRecord,
        evidence: &CapturedVescDrivetrainEvidence,
    ) -> Option<VescReplayRejection> {
        if record.characteristic != crate::VESC_NOTIFY_CHANNEL {
            return Some(VescReplayRejection::WrongCharacteristic);
        }
        if !self.inner.replay_is_connected() {
            return Some(VescReplayRejection::Disconnected);
        }
        if self
            .last_receipt
            .is_some_and(|last| record.monotonic_ms < last)
        {
            return Some(VescReplayRejection::OutOfOrder);
        }
        match evidence {
            CapturedVescDrivetrainEvidence::UnsupportedTarget => {
                Some(VescReplayRejection::UnsupportedTarget)
            }
            CapturedVescDrivetrainEvidence::Local(context) => self.reject_context(context),
            CapturedVescDrivetrainEvidence::Missing
            | CapturedVescDrivetrainEvidence::Invalid(_) => None,
        }
    }

    fn reject_context(
        &self,
        context: &CapturedVescDrivetrainContext,
    ) -> Option<VescReplayRejection> {
        let previous = self.scope.as_ref()?;
        if previous.attempt() != context.attempt() {
            return (context.attempt().generation() <= previous.attempt().generation())
                .then_some(VescReplayRejection::RetiredAttempt);
        }
        let current = context.profile();
        let previous = previous.profile();
        if current.revision() < previous.revision() {
            return Some(VescReplayRejection::StaleRevision);
        }
        (current.revision() == previous.revision() && current.value() != previous.value())
            .then_some(VescReplayRejection::ConflictingRevision)
    }

    fn prepare_record(record: &PevcapRecord, host: &mut HostSession<Self>) {
        match record.direction {
            PevcapDirection::LinkUp | PevcapDirection::LinkDown => {
                *host = HostSession::new(Self::new(host.protocol_session().policy));
                return;
            }
            PevcapDirection::Outbound => return,
            PevcapDirection::Inbound => {}
        }
        let evidence = record.captured_vesc_drivetrain();
        if let Some(reason) = host.protocol_session().reject(record, &evidence) {
            let session = host.protocol_session_mut();
            session.disposition = VescReplayRecordDisposition::Ignored(reason);
            session.captured_geometry = evidence;
            session.original.clone_from(&record.semantic_telemetry);
            return;
        }
        if let CapturedVescDrivetrainEvidence::Local(context) = &evidence {
            if host
                .protocol_session()
                .scope
                .as_ref()
                .is_some_and(|previous| previous.attempt() != context.attempt())
            {
                *host = HostSession::new(Self::new(host.protocol_session().policy));
                host.ingest_link_up(LinkInfo {
                    monotonic_ms: record.monotonic_ms,
                    max_write_len: None,
                });
            }
            host.protocol_session_mut().scope = Some(context.clone());
        }
        let session = host.protocol_session_mut();
        let geometry = match (&evidence, session.policy) {
            (_, VescReplayGeometryPolicy::Explicit(value)) => {
                VescReplayGeometryBasis::Explicit(value)
            }
            (
                CapturedVescDrivetrainEvidence::Local(context),
                VescReplayGeometryPolicy::Captured,
            ) => VescReplayGeometryBasis::Captured(context.clone()),
            (_, VescReplayGeometryPolicy::Captured) => session.protocol_basis(),
        };
        let changed = session.geometry.geometry() != geometry.geometry();
        session
            .inner
            .configure_replay_drivetrain(geometry.user_value());
        session.geometry = geometry;
        session.captured_geometry = evidence;
        session.original.clone_from(&record.semantic_telemetry);
        session.last_receipt = Some(record.monotonic_ms);
        session.disposition = VescReplayRecordDisposition::Accepted;
        if changed {
            host.invalidate_calculated_speed();
        }
    }

    fn protocol_basis(&self) -> VescReplayGeometryBasis {
        self.inner.replay_device_geometry().map_or(
            VescReplayGeometryBasis::Unknown,
            VescReplayGeometryBasis::ProtocolEvidence,
        )
    }

    fn finish_record(record: &PevcapRecord, host: &mut HostSession<Self>) {
        if record.direction != PevcapDirection::Inbound
            || host.protocol_session().disposition != VescReplayRecordDisposition::Accepted
        {
            return;
        }
        let session = host.protocol_session_mut();
        if session.inner.application_discovery() == crate::VescApplicationDiscovery::Refloat {
            session.geometry = VescReplayGeometryBasis::Unknown;
            host.invalidate_calculated_speed();
        } else if session.geometry.user_value().is_none() {
            let basis = session.protocol_basis();
            let changed = session.geometry.geometry() != basis.geometry();
            session.geometry = basis;
            if changed {
                host.invalidate_calculated_speed();
            }
        }
    }

    /// Returns the current projection and record evidence, without an effect executor.
    #[must_use]
    pub fn observation(host: &HostSession<Self>) -> VescReplayObservation {
        let session = host.protocol_session();
        VescReplayObservation {
            telemetry: host.current_snapshot(),
            geometry: session.geometry.clone(),
            captured_geometry: session.captured_geometry.clone(),
            original: session.original.clone(),
            disposition: session.disposition,
        }
    }
}

impl ProtocolSession for VescCaptureReplaySession {
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
        match input {
            SessionInput::Command(_) => {}
            SessionInput::Notification { .. }
                if self.disposition != VescReplayRecordDisposition::Accepted => {}
            other => {
                let mut decoded = Vec::new();
                self.inner.handle(other, &mut decoded);
                output.extend(decoded.into_iter().filter(|item| match item {
                    SessionOutput::Transport(_) => false,
                    _ => true,
                }));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cutout_core::{GattChannel, LinkInfo, MonotonicTimestamp};

    fn record(
        diameter: u32,
        circumference: u64,
        revision: u64,
        generation: u64,
        at: u64,
    ) -> PevcapRecord {
        PevcapRecord::inbound_notification(
            MonotonicTimestamp::new(at),
            crate::VESC_NOTIFY_CHANNEL,
            GattChannel::from_bytes([0; 16]),
            vec![2, 9, 50, 0, 0, 0, 128, 0, 0, 23, 112, 244, 198, 3],
        )
        .with_semantic_telemetry(PevcapSemanticTelemetry {
            observed_at_ms: Some(MonotonicTimestamp::new(at)),
            provenance: PevcapTelemetryProvenance::LiveSession,
            snapshot_schema_version: 2,
            library_version: "original-library".to_owned(),
            snapshot_json: serde_json::json!({
                "vesc_drivetrain_context": {
                    "connection_attempt": {"generation":generation,"platform_identifier":"board-a"},
                    "revision":revision,"source":"user_configured",
                    "wheel_diameter_micrometres":diameter,"wheel_circumference_mm":circumference,
                    "motor_pole_pairs":7,"motor_to_wheel_ratio_milli":2500
                }, "speed":{"value":{"value":999999}}
            })
            .to_string(),
        })
    }

    fn custom_app(payload: &[u8]) -> bytes::Bytes {
        let mut frame = arrayvec::ArrayVec::<u8, { crate::vesc_codec::VESC_MAX_FRAME_LEN }>::new();
        crate::refloat_codec::encode_custom_app_frame(payload, &mut frame).unwrap();
        bytes::Bytes::copy_from_slice(&frame)
    }

    fn host(policy: VescReplayGeometryPolicy) -> HostSession<VescCaptureReplaySession> {
        let mut host = HostSession::new(VescCaptureReplaySession::new(policy));
        host.ingest_link_up(LinkInfo {
            monotonic_ms: MonotonicTimestamp::new(0),
            max_write_len: None,
        });
        host
    }

    fn ingest(
        host: &mut HostSession<VescCaptureReplaySession>,
        record: &PevcapRecord,
    ) -> VescReplayObservation {
        VescCaptureReplaySession::observe_record(
            cutout_core::PevcapReplayRecord::Before(record),
            host,
        );
        host.ingest(SessionInput::Notification {
            channel: record.characteristic,
            bytes: &record.bytes,
            monotonic_ms: record.monotonic_ms,
        });
        VescCaptureReplaySession::observe_record(
            cutout_core::PevcapReplayRecord::After(record),
            host,
        );
        VescCaptureReplaySession::observation(host)
    }

    #[test]
    fn captured_geometry_replay_uses_each_original_revision_without_rewriting_proof() {
        let mut host = host(VescReplayGeometryPolicy::Captured);
        let original = record(90_000, 283, 1, 1, 100);
        let before = original.clone();
        let first = ingest(&mut host, &original);
        assert_eq!(
            first
                .telemetry
                .speed
                .unwrap()
                .value
                .as_millimetres_per_second(),
            1617
        );
        assert_eq!(first.original, original.semantic_telemetry);
        assert_eq!(original, before);
        assert_eq!(first.provenance(), PevcapTelemetryProvenance::Replay);
        let changed = ingest(&mut host, &record(100_000, 314, 2, 1, 200));
        assert_eq!(
            changed
                .telemetry
                .speed
                .unwrap()
                .value
                .as_millimetres_per_second(),
            1794
        );
        assert_eq!(
            first
                .telemetry
                .speed
                .unwrap()
                .value
                .as_millimetres_per_second(),
            1617
        );
        let VescReplayGeometryBasis::Captured(context) = changed.geometry else {
            panic!("captured basis missing")
        };
        assert_eq!(context.profile().revision().get(), 2);
        assert_eq!(changed.telemetry.trip_distance, None);
        let mut outputs = Vec::new();
        host.drain_outputs_into(&mut outputs);
        assert!(
            !outputs
                .iter()
                .any(|o| matches!(o, SessionOutput::Transport(_)))
        );
    }

    #[test]
    fn captured_geometry_replay_missing_or_invalid_context_clears_old_calculated_speed() {
        let mut host = host(VescReplayGeometryPolicy::Captured);
        let known = ingest(&mut host, &record(90_000, 283, 1, 1, 100));
        assert_eq!(
            known
                .telemetry
                .speed
                .unwrap()
                .value
                .as_millimetres_per_second(),
            1617
        );
        let mut missing = record(90_000, 283, 1, 1, 200);
        missing.semantic_telemetry = None;
        let result = ingest(&mut host, &missing);
        assert_eq!(result.telemetry.speed, None);
        assert_eq!(result.telemetry.speed_observed_at_ms, None);
        assert_eq!(result.geometry, VescReplayGeometryBasis::Unknown);
        let invalid = ingest(&mut host, &record(100_000, 283, 2, 1, 300));
        assert_eq!(invalid.telemetry.speed, None);
        assert!(matches!(
            invalid.captured_geometry,
            CapturedVescDrivetrainEvidence::Invalid(_)
        ));
    }

    #[test]
    fn captured_geometry_replay_secondary_target_never_updates_local_measurements() {
        for policy in [
            VescReplayGeometryPolicy::Captured,
            VescReplayGeometryPolicy::Explicit(VescDrivetrain::parse("100", "7", "2.5").unwrap()),
        ] {
            let mut host = host(policy);
            let mut secondary = record(90_000, 283, 1, 1, 100);
            secondary.target = Some(cutout_core::RequestTarget::VescCanController {
                controller_id: cutout_core::VescControllerId::new(3),
            });
            secondary.bytes = vec![
                2, 23, 50, 0, 0, 1, 143, 1, 184, 1, 134, 0, 0, 8, 52, 0, 0, 4, 216, 0, 0, 39, 16,
                1, 242, 169, 202, 3,
            ]
            .into();
            let result = ingest(&mut host, &secondary);
            assert_eq!(result.telemetry.voltage, None);
            assert_eq!(result.telemetry.battery_current, None);
            assert_eq!(result.telemetry.speed, None);
        }
    }

    #[test]
    fn captured_geometry_replay_rejects_retired_attempt_revision_and_old_receipt() {
        for stale in [
            record(90_000, 283, 1, 1, 300),
            record(90_000, 283, 1, 2, 300),
            record(90_000, 283, 2, 2, 300),
            record(90_000, 283, 3, 2, 50),
        ] {
            let mut host = host(VescReplayGeometryPolicy::Captured);
            let current = ingest(&mut host, &record(100_000, 314, 2, 2, 200));
            let replayed = ingest(&mut host, &stale);
            assert_eq!(
                replayed.telemetry.speed_observed_at_ms,
                current.telemetry.speed_observed_at_ms
            );
            assert_eq!(replayed.telemetry.speed, current.telemetry.speed);
        }
    }

    #[test]
    fn captured_geometry_replay_wrong_characteristic_cannot_retire_context_or_partial_frame() {
        let mut host = host(VescReplayGeometryPolicy::Captured);
        let mut first = record(90_000, 283, 1, 1, 100);
        let rest = first.bytes.slice(7..);
        first.bytes = first.bytes.slice(..7);
        ingest(&mut host, &first);
        let mut wrong = record(100_000, 314, 2, 2, 110);
        wrong.characteristic = GattChannel::from_bytes([7; 16]);
        ingest(&mut host, &wrong);
        let mut end = record(90_000, 283, 1, 1, 120);
        end.bytes = rest;
        let result = ingest(&mut host, &end);
        assert_eq!(
            result
                .telemetry
                .speed
                .unwrap()
                .value
                .as_millimetres_per_second(),
            1617
        );
    }

    #[test]
    fn captured_geometry_replay_late_refloat_clears_canonical_calculated_speed() {
        let mut host = host(VescReplayGeometryPolicy::Captured);
        let known = ingest(&mut host, &record(90_000, 283, 1, 1, 100));
        let mut info = record(90_000, 283, 1, 1, 200);
        let mut payload = [0_u8; 60];
        payload[..4].copy_from_slice(&[101, 0, 2, 0]);
        payload[4..11].copy_from_slice(b"Refloat");
        payload[24..27].copy_from_slice(&[1, 3, 0]);
        payload[51..55].copy_from_slice(&1_000_u32.to_be_bytes());
        info.bytes = custom_app(&payload);
        ingest(&mut host, &info);
        assert!(known.telemetry.speed.is_some());
        assert_eq!(host.current_snapshot().speed, None);
        assert_eq!(host.current_snapshot().speed_observed_at_ms, None);
        assert_eq!(host.current_snapshot().at_ms, known.telemetry.at_ms);
        let mut reported = record(90_000, 283, 1, 1, 300);
        // Advertise a realtime speed field before its half-float value.
        let mut ids = info.clone();
        ids.monotonic_ms = MonotonicTimestamp::new(250);
        let mut descriptor = vec![101, 32, 1, 11];
        descriptor.extend_from_slice(b"motor.speed");
        descriptor.push(0);
        ids.bytes = custom_app(&descriptor);
        ingest(&mut host, &ids);
        let mut data = vec![101, 31, 4, 0];
        data.extend_from_slice(&42_u32.to_be_bytes());
        data.extend_from_slice(&[0x13, 0xc1, 0xa6, 8]);
        data.extend_from_slice(&0x5080_u16.to_be_bytes()); // 36 kilometres/hour
        data.extend_from_slice(&[0; 9]);
        reported.bytes = custom_app(&data);
        let actual = ingest(&mut host, &reported);
        assert_eq!(
            actual
                .telemetry
                .speed
                .unwrap()
                .value
                .as_millimetres_per_second(),
            10000
        );
        assert_eq!(
            actual.telemetry.speed.unwrap().source,
            cutout_core::ValueSource::Reported
        );
    }

    #[test]
    fn captured_geometry_replay_first_context_preserves_an_unknown_partial_frame() {
        let mut host = host(VescReplayGeometryPolicy::Captured);
        let mut start = record(90_000, 283, 1, 1, 100);
        let mut finish = start.clone();
        finish.bytes = start.bytes.slice(7..);
        finish.monotonic_ms = MonotonicTimestamp::new(110);
        start.bytes = start.bytes.slice(..7);
        start.semantic_telemetry = None;
        assert_eq!(ingest(&mut host, &start).telemetry.speed, None);
        let result = ingest(&mut host, &finish);
        assert_eq!(
            result
                .telemetry
                .speed
                .unwrap()
                .value
                .as_millimetres_per_second(),
            1617
        );
    }

    #[test]
    fn captured_geometry_replay_shared_driver_preserves_chunks_reconnects_and_passivity() {
        use cutout_core::{
            PevcapCapture, PevcapEncoding, PevcapHeader, PevcapReader, PevcapReplayMode,
            WallClockUnixTimestamp,
        };
        let mut first = record(90_000, 283, 1, 1, 100);
        let mut end = first.clone();
        end.monotonic_ms = MonotonicTimestamp::new(110);
        end.bytes = first.bytes.slice(7..);
        first.bytes = first.bytes.slice(..7);
        let mut unknown = record(90_000, 283, 1, 1, 310);
        unknown.semantic_telemetry = None;
        let header = PevcapHeader::new(
            WallClockUnixTimestamp::new(123),
            "original-board",
            None,
            &[],
            &[],
            None,
            None,
            "original-library",
            [9; 32],
            &[],
        )
        .unwrap();
        let capture = PevcapCapture::new(
            header,
            vec![
                PevcapRecord::link_up(MonotonicTimestamp::new(0), None),
                PevcapRecord::outbound_write(
                    MonotonicTimestamp::new(1),
                    crate::VESC_NOTIFY_CHANNEL,
                    cutout_core::WriteMode::WithResponse,
                    vec![2, 1, 4, 64, 132, 3],
                ),
                first,
                end,
                record(100_000, 314, 2, 1, 200),
                PevcapRecord::link_down(MonotonicTimestamp::new(210)),
                record(100_000, 314, 2, 1, 220),
                PevcapRecord::link_up(MonotonicTimestamp::new(300), None),
                unknown,
                record(90_000, 283, 1, 2, 320),
            ],
        );
        let lengths = capture.arbitrary_notification_chunk_lengths();
        for policy in [
            VescReplayGeometryPolicy::Captured,
            VescReplayGeometryPolicy::Explicit(VescDrivetrain::parse("100", "7", "2.5").unwrap()),
        ] {
            let mut expected = None;
            for encoding in [PevcapEncoding::Jsonl, PevcapEncoding::Binary] {
                let bytes = capture.encode(encoding).unwrap();
                for mode in [
                    PevcapReplayMode::Whole,
                    PevcapReplayMode::OneByte,
                    PevcapReplayMode::Lengths(&lengths),
                ] {
                    let mut host = HostSession::new(VescCaptureReplaySession::new(policy));
                    let mut outputs = Vec::new();
                    PevcapReader::new(std::io::Cursor::new(&bytes), encoding)
                        .unwrap()
                        .replay_into_host_with_record_context(
                            mode,
                            &mut host,
                            &mut outputs,
                            true,
                            VescCaptureReplaySession::observe_record,
                        )
                        .unwrap();
                    let observation = VescCaptureReplaySession::observation(&host);
                    let wanted = if matches!(policy, VescReplayGeometryPolicy::Captured) {
                        1617
                    } else {
                        1794
                    };
                    assert_eq!(
                        observation
                            .telemetry
                            .speed
                            .unwrap()
                            .value
                            .as_millimetres_per_second(),
                        wanted
                    );
                    assert_eq!(
                        observation.telemetry.speed_observed_at_ms,
                        Some(MonotonicTimestamp::new(320))
                    );
                    assert_eq!(observation.telemetry.trip_distance, None);
                    assert_eq!(
                        observation.original,
                        capture.records.last().unwrap().semantic_telemetry
                    );
                    if let Some(previous) = &expected {
                        assert_eq!(&observation, previous);
                    } else {
                        expected = Some(observation);
                    }
                    host.issue_command(cutout_core::DeviceCommand::RequestTelemetry);
                    host.ingest(SessionInput::Tick {
                        monotonic_ms: MonotonicTimestamp::new(10_000),
                    });
                    host.drain_outputs_into(&mut outputs);
                    assert!(
                        !outputs
                            .iter()
                            .any(|item| matches!(item, SessionOutput::Transport(_)))
                    );
                    assert_eq!(capture.encode(encoding).unwrap(), bytes);
                }
            }
        }
    }

    #[test]
    fn captured_geometry_replay_identifies_interpreter_without_overwriting_original_version() {
        let mut host = host(VescReplayGeometryPolicy::Captured);
        let result = ingest(&mut host, &record(90_000, 283, 1, 1, 100));
        assert_eq!(
            result.original.as_ref().unwrap().library_version,
            "original-library"
        );
        assert_eq!(result.library_version(), env!("CARGO_PKG_VERSION"));
        assert_eq!(
            result.registry_hash(),
            cutout_core::registry_entries_hash(&crate::MODEL_REGISTRY)
        );
        assert_eq!(result.decoder_source_hash().len(), 64);
        assert_ne!(result.decoder_source_hash(), "0".repeat(64));
    }
}
