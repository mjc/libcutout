//! Passive VESC capture interpretation, isolated from live transport and authority.

use crate::{ReadOnlySession, VescGenericModel};
use cutout_core::{
    CapturedVescDrivetrainContext, CapturedVescDrivetrainError, CapturedVescDrivetrainEvidence,
    CapturedVescDrivetrainScope, HostSession, LinkInfo, MonotonicTimestamp, PevcapDirection,
    PevcapRecord, PevcapReplayRecord, PevcapSemanticTelemetry, PevcapTelemetryProvenance,
    ProtocolSession, SessionInput, SessionOutput, TelemetrySnapshot, VescDrivetrain,
    VescSpeedGeometry,
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
    /// Present context cannot establish its claimed historical connection scope.
    InvalidContext,
    /// The characteristic belongs to a different input stream.
    WrongCharacteristic,
    /// The recorded service belongs to a different input stream.
    WrongService,
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
    /// Source fingerprint of the complete protocol/core workspace inputs, when available.
    /// Packaged consumers without those source inputs retain explicit absence.
    #[must_use]
    pub const fn decoder_source_hash(&self) -> Option<&'static str> {
        option_env!("CUTOUT_REPLAY_DECODER_SOURCE_SHA256")
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
    scope: Option<VescReplayScope>,
    last_receipt: Option<MonotonicTimestamp>,
    disposition: VescReplayRecordDisposition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VescReplayScope {
    identity: CapturedVescDrivetrainScope,
    geometry: Option<VescDrivetrain>,
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
        scope: &Result<Option<CapturedVescDrivetrainScope>, CapturedVescDrivetrainError>,
    ) -> Option<VescReplayRejection> {
        if record.characteristic != crate::VESC_NOTIFY_CHANNEL {
            return Some(VescReplayRejection::WrongCharacteristic);
        }
        if record.service.is_some_and(|service| {
            service.as_bytes() != *cutout_core::BluetoothServiceUuid::VESC_NORDIC_UART.as_bytes()
        }) {
            return Some(VescReplayRejection::WrongService);
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
        match scope {
            Err(CapturedVescDrivetrainError::UnsupportedTarget) => {
                Some(VescReplayRejection::UnsupportedTarget)
            }
            Err(_) => Some(VescReplayRejection::InvalidContext),
            Ok(Some(scope)) => self.reject_context(scope, evidence),
            Ok(None) => None,
        }
    }

    fn reject_context(
        &self,
        scope: &CapturedVescDrivetrainScope,
        evidence: &CapturedVescDrivetrainEvidence,
    ) -> Option<VescReplayRejection> {
        let previous = self.scope.as_ref()?;
        if previous.identity.attempt().platform_identifier()
            != scope.attempt().platform_identifier()
        {
            return Some(VescReplayRejection::UnsupportedTarget);
        }
        if previous.identity.attempt() != scope.attempt() {
            return (scope.attempt().generation() <= previous.identity.attempt().generation())
                .then_some(VescReplayRejection::RetiredAttempt);
        }
        if scope.revision() < previous.identity.revision() {
            return Some(VescReplayRejection::StaleRevision);
        }
        if scope.revision() == previous.identity.revision()
            && let CapturedVescDrivetrainEvidence::Local(context) = evidence
            && previous
                .geometry
                .is_some_and(|value| value != context.profile().value())
        {
            return Some(VescReplayRejection::ConflictingRevision);
        }
        None
    }

    fn prepare_record(record: &PevcapRecord, host: &mut HostSession<Self>) {
        match record.direction {
            PevcapDirection::LinkUp => {
                *host = HostSession::new(Self::new(host.protocol_session().policy));
                return;
            }
            PevcapDirection::LinkDown => {
                // The shared driver delivers LinkDown next, disconnecting and resetting
                // the decoder once. Retain the historical projection and its evidence.
                let session = host.protocol_session_mut();
                session.scope = None;
                session.last_receipt = None;
                return;
            }
            PevcapDirection::Outbound => return,
            PevcapDirection::Inbound => {}
        }
        let evidence = record.captured_vesc_drivetrain();
        let scope = record.captured_vesc_drivetrain_scope();
        if let Some(reason) = host.protocol_session().reject(record, &evidence, &scope) {
            let session = host.protocol_session_mut();
            session.disposition = VescReplayRecordDisposition::Ignored(reason);
            session.captured_geometry = evidence;
            session.original.clone_from(&record.semantic_telemetry);
            return;
        }
        if let Ok(Some(identity)) = scope {
            if host
                .protocol_session()
                .scope
                .as_ref()
                .is_some_and(|previous| previous.identity.attempt() != identity.attempt())
            {
                *host = HostSession::new(Self::new(host.protocol_session().policy));
                host.ingest_link_up(LinkInfo {
                    monotonic_ms: record.monotonic_ms,
                    max_write_len: None,
                });
            }
            let geometry = match &evidence {
                CapturedVescDrivetrainEvidence::Local(context) => Some(context.profile().value()),
                _ => host
                    .protocol_session()
                    .scope
                    .as_ref()
                    .filter(|previous| previous.identity == identity)
                    .and_then(|previous| previous.geometry),
            };
            host.protocol_session_mut().scope = Some(VescReplayScope { identity, geometry });
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
            let speed_matches_basis =
                session.inner.replay_calculated_speed_geometry() == basis.geometry();
            session.geometry = basis;
            if !speed_matches_basis {
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
    fn telemetry_retention(&self) -> cutout_core::SessionTelemetryRetention {
        cutout_core::SessionTelemetryRetention::Historical
    }

    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
        if let SessionInput::LinkUp(link) = &input {
            self.last_receipt = Some(link.monotonic_ms);
        }
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
        record_on_platform("board-a", diameter, circumference, revision, generation, at)
    }

    fn record_on_platform(
        platform: &str,
        diameter: u32,
        circumference: u64,
        revision: u64,
        generation: u64,
        at: u64,
    ) -> PevcapRecord {
        PevcapRecord::inbound_notification(
            MonotonicTimestamp::new(at),
            crate::VESC_NOTIFY_CHANNEL,
            GattChannel::from_bytes(
                *cutout_core::BluetoothServiceUuid::VESC_NORDIC_UART.as_bytes(),
            ),
            vec![2, 9, 50, 0, 0, 0, 128, 0, 0, 23, 112, 244, 198, 3],
        )
        .with_semantic_telemetry(PevcapSemanticTelemetry {
            observed_at_ms: Some(MonotonicTimestamp::new(at)),
            provenance: PevcapTelemetryProvenance::LiveSession,
            snapshot_schema_version: 2,
            library_version: "original-library".to_owned(),
            snapshot_json: serde_json::json!({
                "vesc_drivetrain_context": {
                    "connection_attempt": {"generation":generation,"platform_identifier":platform},
                    "revision":revision,"source":"user_configured",
                    "wheel_diameter_micrometres":diameter,"wheel_circumference_mm":circumference,
                    "motor_pole_pairs":7,"motor_to_wheel_ratio_milli":2500
                }, "speed":{"value":{"value":999_999}}
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
    fn captured_geometry_replay_invalid_geometry_cannot_hide_foreign_or_retired_scope() {
        for policy in [
            VescReplayGeometryPolicy::Captured,
            VescReplayGeometryPolicy::Explicit(VescDrivetrain::parse("100", "7", "2.5").unwrap()),
        ] {
            for foreign in [false, true] {
                let mut host = host(policy);
                let current = ingest(&mut host, &record(100_000, 314, 2, 2, 200));
                let mut invalid = record(100_000, 283, 1, 1, 300);
                let rejection = if foreign {
                    let semantic = invalid.semantic_telemetry.as_mut().unwrap();
                    let mut snapshot: serde_json::Value =
                        serde_json::from_str(&semantic.snapshot_json).unwrap();
                    snapshot["vesc_drivetrain_context"]["connection_attempt"]["platform_identifier"] =
                        serde_json::json!("foreign-controller");
                    semantic.snapshot_json = snapshot.to_string();
                    VescReplayRejection::UnsupportedTarget
                } else {
                    VescReplayRejection::RetiredAttempt
                };
                let original = invalid.clone();
                let rejected = ingest(&mut host, &invalid);
                assert_eq!(
                    rejected.disposition,
                    VescReplayRecordDisposition::Ignored(rejection)
                );
                assert_eq!(rejected.telemetry, current.telemetry);
                assert_eq!(rejected.geometry, current.geometry);
                assert_eq!(invalid, original);
                let next = ingest(&mut host, &record(100_000, 314, 2, 2, 250));
                assert_eq!(next.disposition, VescReplayRecordDisposition::Accepted);
            }
        }
    }

    #[test]
    fn captured_geometry_replay_new_attempt_with_invalid_geometry_retires_partial_frame() {
        let mut host = host(VescReplayGeometryPolicy::Captured);
        let mut start = record(90_000, 283, 1, 1, 100);
        start.bytes = start.bytes.slice(..7);
        assert_eq!(ingest(&mut host, &start).telemetry.speed, None);
        let mut replacement = record(100_000, 283, 1, 2, 110);
        replacement.bytes = bytes::Bytes::new();
        assert_eq!(
            ingest(&mut host, &replacement).disposition,
            VescReplayRecordDisposition::Accepted
        );
        let mut suffix = record(90_000, 283, 1, 2, 120);
        suffix.bytes = suffix.bytes.slice(7..);
        assert_eq!(ingest(&mut host, &suffix).telemetry.speed, None);
    }

    #[test]
    fn captured_geometry_replay_invalid_geometry_preserves_revision_fences() {
        let mut host = host(VescReplayGeometryPolicy::Captured);
        ingest(&mut host, &record(90_000, 283, 2, 1, 100));
        assert_eq!(
            ingest(&mut host, &record(100_000, 283, 2, 1, 110))
                .telemetry
                .speed,
            None
        );
        assert_eq!(
            ingest(&mut host, &record(100_000, 314, 2, 1, 120)).disposition,
            VescReplayRecordDisposition::Ignored(VescReplayRejection::ConflictingRevision)
        );

        ingest(&mut host, &record(100_000, 283, 3, 1, 130));
        assert_eq!(
            ingest(&mut host, &record(90_000, 283, 2, 1, 150)).disposition,
            VescReplayRecordDisposition::Ignored(VescReplayRejection::StaleRevision)
        );
        let current = ingest(&mut host, &record(100_000, 314, 3, 1, 140));
        assert_eq!(current.disposition, VescReplayRecordDisposition::Accepted);
        assert!(current.telemetry.speed.is_some());
    }

    #[test]
    fn captured_geometry_replay_invalid_scope_preserves_partial_frame_and_watermark() {
        for (revision, generation, platform) in [(0, 1, "board-a"), (1, 0, "board-a"), (1, 1, " ")]
        {
            let mut host = host(VescReplayGeometryPolicy::Captured);
            let mut start = record(90_000, 283, 1, 1, 100);
            let mut finish = record(90_000, 283, 1, 1, 110);
            start.bytes = start.bytes.slice(..7);
            finish.bytes = finish.bytes.slice(7..);
            let before = ingest(&mut host, &start);
            let rejected = ingest(
                &mut host,
                &record_on_platform(platform, 90_000, 283, revision, generation, 300),
            );
            assert_eq!(
                rejected.disposition,
                VescReplayRecordDisposition::Ignored(VescReplayRejection::InvalidContext)
            );
            assert_eq!(rejected.telemetry, before.telemetry);
            let completed = ingest(&mut host, &finish);
            assert_eq!(completed.disposition, VescReplayRecordDisposition::Accepted);
            assert!(completed.telemetry.speed.is_some());
        }
    }

    #[test]
    fn captured_geometry_replay_rejects_nonlocal_context_with_malformed_geometry() {
        for policy in [
            VescReplayGeometryPolicy::Captured,
            VescReplayGeometryPolicy::Explicit(VescDrivetrain::parse("100", "7", "2.5").unwrap()),
        ] {
            let mut host = host(policy);
            let current = ingest(&mut host, &record(90_000, 283, 1, 1, 100));
            let mut secondary = record(90_000, 283, 1, 1, 200);
            let semantic = secondary.semantic_telemetry.as_mut().unwrap();
            let mut snapshot: serde_json::Value =
                serde_json::from_str(&semantic.snapshot_json).unwrap();
            snapshot["vesc_drivetrain_context"]["target"] =
                serde_json::json!({"can": {"controller_id": 3}});
            snapshot["vesc_drivetrain_context"]["revision"] = serde_json::json!("bad");
            semantic.snapshot_json = snapshot.to_string();
            let original = secondary.clone();
            let result = ingest(&mut host, &secondary);
            assert_eq!(
                result.disposition,
                VescReplayRecordDisposition::Ignored(VescReplayRejection::UnsupportedTarget)
            );
            assert_eq!(result.telemetry, current.telemetry);
            assert_eq!(result.geometry, current.geometry);
            assert_eq!(secondary, original);
        }
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
    fn captured_geometry_replay_rejects_foreign_platform_without_retiring_local_frame() {
        for policy in [
            VescReplayGeometryPolicy::Captured,
            VescReplayGeometryPolicy::Explicit(VescDrivetrain::parse("100", "7", "2.5").unwrap()),
        ] {
            for generation in [3, 2, 1] {
                let mut host = host(policy);
                ingest(&mut host, &record(90_000, 283, 1, 2, 100));
                let mut start = record(90_000, 283, 1, 2, 110);
                let rest = start.bytes.slice(7..);
                start.bytes = start.bytes.slice(..7);
                let before = ingest(&mut host, &start);
                let scope = host.protocol_session().scope.clone();
                let foreign = record_on_platform("board-b", 100_000, 314, 2, generation, 200);
                let original = foreign.clone();

                let ignored = ingest(&mut host, &foreign);
                assert_eq!(
                    ignored.disposition,
                    VescReplayRecordDisposition::Ignored(VescReplayRejection::UnsupportedTarget)
                );
                assert_eq!(ignored.telemetry, before.telemetry);
                assert_eq!(ignored.geometry, before.geometry);
                assert_eq!(
                    ignored.captured_geometry,
                    foreign.captured_vesc_drivetrain()
                );
                assert_eq!(ignored.original, foreign.semantic_telemetry);
                assert_eq!(foreign, original);
                assert_eq!(host.protocol_session().scope, scope);
                assert_eq!(
                    host.protocol_session().last_receipt,
                    Some(start.monotonic_ms)
                );

                let mut end = record(90_000, 283, 1, 2, 120);
                end.bytes = rest;
                let completed = ingest(&mut host, &end);
                assert_eq!(completed.disposition, VescReplayRecordDisposition::Accepted);
                assert_eq!(completed.telemetry.speed, before.telemetry.speed);
                assert_eq!(
                    completed.telemetry.speed_observed_at_ms,
                    Some(end.monotonic_ms)
                );
                let mut outputs = Vec::new();
                host.drain_outputs_into(&mut outputs);
                assert!(
                    !outputs
                        .iter()
                        .any(|output| matches!(output, SessionOutput::Transport(_)))
                );
            }
        }
    }

    #[test]
    fn captured_geometry_replay_accepts_same_platform_advance_and_new_link_platform() {
        for policy in [
            VescReplayGeometryPolicy::Captured,
            VescReplayGeometryPolicy::Explicit(VescDrivetrain::parse("100", "7", "2.5").unwrap()),
        ] {
            let mut host = host(policy);
            ingest(&mut host, &record(90_000, 283, 2, 1, 100));
            let next = record(100_000, 314, 1, 2, 200);
            let advanced = ingest(&mut host, &next);
            assert_eq!(advanced.disposition, VescReplayRecordDisposition::Accepted);
            assert_eq!(
                advanced.telemetry.speed_observed_at_ms,
                Some(next.monotonic_ms)
            );
            assert_eq!(advanced.captured_geometry, next.captured_vesc_drivetrain());

            let link = PevcapRecord::link_up(MonotonicTimestamp::new(300), None);
            VescCaptureReplaySession::observe_record(PevcapReplayRecord::Before(&link), &mut host);
            host.ingest_link_up(LinkInfo {
                monotonic_ms: link.monotonic_ms,
                max_write_len: None,
            });
            let new_platform = record_on_platform("board-b", 90_000, 283, 1, 1, 310);
            let accepted = ingest(&mut host, &new_platform);
            assert_eq!(accepted.disposition, VescReplayRecordDisposition::Accepted);
            assert_eq!(
                accepted.captured_geometry,
                new_platform.captured_vesc_drivetrain()
            );
            assert_eq!(
                accepted.telemetry.speed_observed_at_ms,
                Some(new_platform.monotonic_ms)
            );
            let mut outputs = Vec::new();
            host.drain_outputs_into(&mut outputs);
            assert!(
                !outputs
                    .iter()
                    .any(|output| matches!(output, SessionOutput::Transport(_)))
            );
        }
    }

    #[test]
    fn captured_geometry_replay_first_receipt_cannot_predate_its_link() {
        let mut host = HostSession::new(VescCaptureReplaySession::new(
            VescReplayGeometryPolicy::Captured,
        ));
        host.ingest_link_up(LinkInfo {
            monotonic_ms: MonotonicTimestamp::new(300),
            max_write_len: None,
        });
        let stale = ingest(&mut host, &record(90_000, 283, 1, 1, 250));
        assert_eq!(
            stale.disposition,
            VescReplayRecordDisposition::Ignored(VescReplayRejection::OutOfOrder)
        );
        assert_eq!(stale.telemetry.speed, None);
        let fresh = ingest(&mut host, &record(90_000, 283, 1, 1, 320));
        assert_eq!(
            fresh.telemetry.speed_observed_at_ms,
            Some(MonotonicTimestamp::new(320))
        );
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
    fn captured_geometry_replay_wrong_service_cannot_retire_context_or_partial_frame() {
        let mut host = host(VescReplayGeometryPolicy::Captured);
        let mut first = record(90_000, 283, 1, 1, 100);
        let rest = first.bytes.slice(7..);
        first.bytes = first.bytes.slice(..7);
        let before = ingest(&mut host, &first);
        let scope = host.protocol_session().scope.clone();
        let mut wrong = record(100_000, 314, 2, 2, 200);
        wrong.service = Some(GattChannel::from_bytes([7; 16]));
        let ignored = ingest(&mut host, &wrong);
        assert_eq!(
            ignored.disposition,
            VescReplayRecordDisposition::Ignored(VescReplayRejection::WrongService)
        );
        assert_eq!(ignored.telemetry, before.telemetry);
        assert_eq!(ignored.geometry, before.geometry);
        assert_eq!(ignored.original, wrong.semantic_telemetry);
        assert_eq!(host.protocol_session().scope, scope);
        assert_eq!(
            host.protocol_session().last_receipt,
            Some(first.monotonic_ms)
        );
        let mut end = record(90_000, 283, 1, 1, 120);
        end.bytes = rest;
        let result = ingest(&mut host, &end);
        assert_eq!(result.disposition, VescReplayRecordDisposition::Accepted);
        assert_eq!(
            result
                .telemetry
                .speed
                .unwrap()
                .value
                .as_millimetres_per_second(),
            1617
        );
        assert_eq!(
            result.telemetry.speed_observed_at_ms,
            Some(end.monotonic_ms)
        );
    }

    #[test]
    fn captured_geometry_replay_absent_service_remains_compatible_at_record_boundary() {
        for policy in [
            VescReplayGeometryPolicy::Captured,
            VescReplayGeometryPolicy::Explicit(VescDrivetrain::parse("100", "7", "2.5").unwrap()),
        ] {
            let mut expected_host = host(policy);
            let mut actual_host = host(policy);
            let mut legacy = record(90_000, 283, 1, 1, 100);
            let expected = ingest(&mut expected_host, &legacy);
            legacy.service = None;
            let actual = ingest(&mut actual_host, &legacy);
            assert_eq!(actual, expected);
            assert!(actual.telemetry.speed.is_some());
            assert_eq!(
                actual.telemetry.speed_observed_at_ms,
                Some(legacy.monotonic_ms)
            );
        }
    }

    #[test]
    fn captured_geometry_replay_service_admission_preserves_original_bytes() {
        use cutout_core::{PevcapEncoding, PevcapReader, PevcapReplayMode};
        for policy in [
            VescReplayGeometryPolicy::Captured,
            VescReplayGeometryPolicy::Explicit(VescDrivetrain::parse("100", "7", "2.5").unwrap()),
        ] {
            for (service, disposition) in [
                (
                    Some(GattChannel::from_bytes(
                        *cutout_core::BluetoothServiceUuid::VESC_NORDIC_UART.as_bytes(),
                    )),
                    VescReplayRecordDisposition::Accepted,
                ),
                (
                    Some(GattChannel::from_bytes([0; 16])),
                    VescReplayRecordDisposition::Ignored(VescReplayRejection::WrongService),
                ),
            ] {
                let mut capture = reconnect_capture();
                let good = record(90_000, 283, 1, 1, 100);
                let mut last = record(100_000, 314, 2, 2, 200);
                last.service = service;
                let expected_record = if disposition == VescReplayRecordDisposition::Accepted {
                    &last
                } else {
                    &good
                };
                let mut expected_host = host(policy);
                let expected = ingest(&mut expected_host, expected_record);
                capture.records = vec![
                    PevcapRecord::link_up(MonotonicTimestamp::new(0), None),
                    good,
                    last.clone(),
                ];
                for encoding in [PevcapEncoding::Jsonl, PevcapEncoding::Binary] {
                    let bytes = capture.encode(encoding).unwrap();
                    for mode in [PevcapReplayMode::Whole, PevcapReplayMode::OneByte] {
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
                        let observed = VescCaptureReplaySession::observation(&host);
                        assert_eq!(observed.telemetry, expected.telemetry);
                        assert_eq!(observed.geometry, expected.geometry);
                        assert_eq!(observed.original, last.semantic_telemetry);
                        assert_eq!(observed.disposition, disposition);
                        assert!(
                            !outputs
                                .iter()
                                .any(|output| matches!(output, SessionOutput::Transport(_)))
                        );
                        assert_eq!(capture.encode(encoding).unwrap(), bytes);
                    }
                }
            }
        }
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

    fn reconnect_capture() -> cutout_core::PevcapCapture {
        use cutout_core::{PevcapCapture, PevcapHeader, WallClockUnixTimestamp};
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
        PevcapCapture::new(
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
        )
    }

    #[test]
    fn captured_geometry_replay_shared_driver_preserves_chunks_reconnects_and_passivity() {
        use cutout_core::{PevcapEncoding, PevcapReader, PevcapReplayMode};
        let capture = reconnect_capture();
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
    fn capture_ending_link_down_retains_reported_charging_state() {
        use cutout_core::{PevcapEncoding, PevcapReader, PevcapReplayMode, RideOperatingState};

        let mut info = record(90_000, 283, 1, 1, 100);
        let mut payload = [0_u8; 60];
        payload[..4].copy_from_slice(&[101, 0, 2, 0]);
        payload[4..11].copy_from_slice(b"Refloat");
        payload[24..27].copy_from_slice(&[1, 3, 0]);
        payload[51..55].copy_from_slice(&1_000_u32.to_be_bytes());
        info.bytes = custom_app(&payload);
        let mut ids = info.clone();
        ids.monotonic_ms = MonotonicTimestamp::new(200);
        let mut descriptor = vec![101, 32, 1, 11];
        descriptor.extend_from_slice(b"motor.speed\0");
        ids.bytes = custom_app(&descriptor);
        let mut realtime = info.clone();
        realtime.monotonic_ms = MonotonicTimestamp::new(300);
        let mut data = vec![101, 31, 4, 0];
        data.extend_from_slice(&42_u32.to_be_bytes());
        data.extend_from_slice(&(0x13c1_a608_u32 | (1 << 21)).to_be_bytes());
        data.extend_from_slice(&0x5080_u16.to_be_bytes());
        data.extend_from_slice(&[0; 9]);
        realtime.bytes = custom_app(&data);
        let mut capture = reconnect_capture();
        capture.records = vec![
            PevcapRecord::link_up(MonotonicTimestamp::new(0), None),
            info.clone(),
            ids.clone(),
            realtime.clone(),
            PevcapRecord::link_down(MonotonicTimestamp::new(310)),
        ];
        let mut expected_host = host(VescReplayGeometryPolicy::Captured);
        for record in [&info, &ids, &realtime] {
            ingest(&mut expected_host, record);
        }
        let expected = VescCaptureReplaySession::observation(&expected_host);
        assert_eq!(
            expected.telemetry.operating_state,
            Some(RideOperatingState::Charging)
        );
        for encoding in [PevcapEncoding::Jsonl, PevcapEncoding::Binary] {
            let bytes = capture.encode(encoding).unwrap();
            for mode in [PevcapReplayMode::Whole, PevcapReplayMode::OneByte] {
                let mut host = HostSession::new(VescCaptureReplaySession::new(
                    VescReplayGeometryPolicy::Captured,
                ));
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
                assert_eq!(VescCaptureReplaySession::observation(&host), expected);
                assert!(!host.protocol_session().inner.replay_is_connected());
                assert_eq!(
                    outputs
                        .iter()
                        .filter(|output| matches!(
                            output,
                            SessionOutput::Event(cutout_core::DeviceEvent::LinkDown)
                        ))
                        .count(),
                    1
                );
                assert_eq!(capture.encode(encoding).unwrap(), bytes);
            }
        }
    }

    #[test]
    fn capture_ending_link_down_retains_last_observation_without_new_sample() {
        use cutout_core::{PevcapEncoding, PevcapReader, PevcapReplayMode};
        let mut capture = reconnect_capture();
        let original = record(90_000, 283, 1, 1, 100);
        capture.records = vec![
            PevcapRecord::link_up(MonotonicTimestamp::new(0), None),
            original.clone(),
            PevcapRecord::link_down(MonotonicTimestamp::new(110)),
        ];
        let mut expected_host = host(VescReplayGeometryPolicy::Captured);
        let expected = ingest(&mut expected_host, &original);
        assert!(expected.telemetry.speed.is_some());
        for encoding in [PevcapEncoding::Jsonl, PevcapEncoding::Binary] {
            let bytes = capture.encode(encoding).unwrap();
            for mode in [PevcapReplayMode::Whole, PevcapReplayMode::OneByte] {
                let mut host = HostSession::new(VescCaptureReplaySession::new(
                    VescReplayGeometryPolicy::Captured,
                ));
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
                assert_eq!(VescCaptureReplaySession::observation(&host), expected);
                assert!(!host.protocol_session().inner.replay_is_connected());
                assert!(host.protocol_session().scope.is_none());
                assert_eq!(host.protocol_session().last_receipt, None);
                assert_eq!(
                    outputs
                        .iter()
                        .filter(|output| matches!(
                            output,
                            SessionOutput::Event(cutout_core::DeviceEvent::LinkDown)
                        ))
                        .count(),
                    1
                );
                assert!(
                    !outputs
                        .iter()
                        .any(|output| matches!(output, SessionOutput::Transport(_)))
                );
                assert_eq!(capture.encode(encoding).unwrap(), bytes);
            }
        }
    }

    #[test]
    fn disconnected_capture_rejects_late_bytes_and_link_up_starts_new_scope() {
        use cutout_core::{PevcapEncoding, PevcapReader, PevcapReplayMode};
        let mut capture = reconnect_capture();
        let original = record(90_000, 283, 2, 1, 100);
        let late = record(100_000, 314, 3, 1, 120);
        let mut unknown = record(90_000, 283, 1, 1, 210);
        unknown.semantic_telemetry = None;
        let fresh = record(100_000, 314, 1, 1, 220);
        capture.records = vec![
            PevcapRecord::link_up(MonotonicTimestamp::new(0), None),
            original,
            PevcapRecord::link_down(MonotonicTimestamp::new(110)),
            late.clone(),
            PevcapRecord::link_up(MonotonicTimestamp::new(200), None),
            unknown,
            fresh.clone(),
        ];
        for encoding in [PevcapEncoding::Jsonl, PevcapEncoding::Binary] {
            let bytes = capture.encode(encoding).unwrap();
            for mode in [PevcapReplayMode::Whole, PevcapReplayMode::OneByte] {
                let mut host = HostSession::new(VescCaptureReplaySession::new(
                    VescReplayGeometryPolicy::Captured,
                ));
                let mut outputs = Vec::new();
                let mut observations = Vec::new();
                PevcapReader::new(std::io::Cursor::new(&bytes), encoding)
                    .unwrap()
                    .replay_into_host_with_record_context(
                        mode,
                        &mut host,
                        &mut outputs,
                        true,
                        |boundary, host| {
                            VescCaptureReplaySession::observe_record(boundary, host);
                            if let PevcapReplayRecord::After(_) = boundary {
                                observations.push((
                                    VescCaptureReplaySession::observation(host),
                                    host.protocol_session().inner.replay_is_connected(),
                                ));
                            }
                        },
                    )
                    .unwrap();
                let before_disconnect = &observations[1].0;
                assert!(before_disconnect.telemetry.speed.is_some());
                assert_eq!(&observations[2].0, before_disconnect);
                assert!(!observations[2].1);
                let rejected = &observations[3].0;
                assert_eq!(rejected.telemetry, before_disconnect.telemetry);
                assert_eq!(rejected.geometry, before_disconnect.geometry);
                assert_eq!(rejected.original, late.semantic_telemetry);
                assert_eq!(
                    rejected.disposition,
                    VescReplayRecordDisposition::Ignored(VescReplayRejection::Disconnected)
                );
                assert!(!observations[3].1);
                let reconnected = &observations[4].0;
                assert!(observations[4].1);
                assert_eq!(reconnected.telemetry, TelemetrySnapshot::default());
                assert_eq!(reconnected.geometry, VescReplayGeometryBasis::Unknown);
                assert_eq!(reconnected.original, None);
                assert_eq!(observations[5].0.telemetry.speed, None);
                let final_observation = &observations[6].0;
                assert_eq!(
                    final_observation.disposition,
                    VescReplayRecordDisposition::Accepted
                );
                assert_eq!(
                    final_observation
                        .telemetry
                        .speed
                        .unwrap()
                        .value
                        .as_millimetres_per_second(),
                    1794
                );
                assert_eq!(
                    final_observation.telemetry.speed_observed_at_ms,
                    Some(MonotonicTimestamp::new(220))
                );
                assert_eq!(final_observation.original, fresh.semantic_telemetry);
                assert_eq!(capture.encode(encoding).unwrap(), bytes);
            }
        }
    }

    fn setup_geometry(diameter_metres: f32) -> Vec<u8> {
        let mut payload = vec![91];
        payload.resize(41, 0);
        payload.push(14);
        payload.extend_from_slice(&2.5_f32.to_bits().to_be_bytes());
        payload.extend_from_slice(&diameter_metres.to_bits().to_be_bytes());
        let mut frame = vec![2, u8::try_from(payload.len()).unwrap()];
        frame.extend_from_slice(&payload);
        frame.extend_from_slice(&crate::wire_crc::crc16_xmodem(&payload).to_be_bytes());
        frame.push(3);
        frame
    }

    #[test]
    fn captured_geometry_replay_protocol_geometry_keeps_only_speed_using_its_actual_basis() {
        let mut host = host(VescReplayGeometryPolicy::Captured);
        let mut raw = record(90_000, 283, 1, 1, 100);
        raw.semantic_telemetry = None;
        let rpm = raw.bytes.clone();
        let mut combined = setup_geometry(0.090);
        combined.extend_from_slice(&rpm);
        raw.bytes = combined.into();
        let initial = ingest(&mut host, &raw);
        assert_eq!(
            initial
                .telemetry
                .speed
                .unwrap()
                .value
                .as_millimetres_per_second(),
            1617
        );
        assert!(matches!(
            initial.geometry,
            VescReplayGeometryBasis::ProtocolEvidence(_)
        ));
        let mut combined = setup_geometry(0.100);
        combined.extend_from_slice(&rpm);
        raw.bytes = combined.into();
        raw.monotonic_ms = MonotonicTimestamp::new(200);
        assert_eq!(
            ingest(&mut host, &raw)
                .telemetry
                .speed
                .unwrap()
                .value
                .as_millimetres_per_second(),
            1794
        );
        let mut reverse = rpm.to_vec();
        reverse.extend_from_slice(&setup_geometry(0.090));
        raw.bytes = reverse.into();
        raw.monotonic_ms = MonotonicTimestamp::new(300);
        assert_eq!(
            ingest(&mut host, &raw).telemetry.speed,
            None,
            "RPM before a later geometry change is stale"
        );
        let mut round_trip = setup_geometry(0.100);
        round_trip.extend_from_slice(&rpm);
        round_trip.extend_from_slice(&setup_geometry(0.090));
        raw.bytes = round_trip.into();
        raw.monotonic_ms = MonotonicTimestamp::new(400);
        assert_eq!(
            ingest(&mut host, &raw).telemetry.speed,
            None,
            "speed using intermediate geometry is stale even when final geometry returns to A"
        );
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
        let source_hash = result
            .decoder_source_hash()
            .expect("workspace fingerprint available");
        assert_eq!(source_hash.len(), 64);
        assert_ne!(source_hash, "0".repeat(64));
    }
}
