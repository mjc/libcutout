//! Generic device-session projection; protocol selection stays below the FFI boundary.

use cutout_protocols::{DeviceConnectionSnapshot, DeviceConnectionStep, VehicleKind};

use crate::{
    CutoutSessionStateHandle, DeviceDetectionEvent, DeviceDetectionResolutionRecord,
    MobileConnectionAttemptSnapshotDto, MobileConnectionAttemptTokenDto, MobileGattFingerprintDto,
    MobileParserDiagnosticsDto, MobileProtocolFamilyDto, MobileSessionInputDto,
    MobileSessionStepResultDto, MobileTelemetrySnapshotDto,
};

/// Native layout category, independent from the controller protocol.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileVehicleKindDto {
    /// Physical vehicle kind has not been established.
    Unknown,
    /// Electric unicycle.
    ElectricUnicycle,
    /// Self-balancing board.
    Board,
    /// Scooter.
    Scooter,
    /// Bicycle.
    Bike,
}

/// Verified protocol and independently known model/kind.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileDeviceIdentityDto {
    /// Wire protocol established by the detector.
    pub protocol: MobileProtocolFamilyDto,
    /// Native presentation template.
    pub vehicle_kind: MobileVehicleKindDto,
    /// Exact model label, absent when only the protocol is verified.
    pub model: Option<String>,
}

/// Identity and connection admission observed together.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileDeviceSessionSnapshotDto {
    /// Immutable attempt state.
    pub connection: MobileConnectionAttemptSnapshotDto,
    /// Verified identity, absent for pending and capture-only attempts.
    pub identity: Option<MobileDeviceIdentityDto>,
}

/// Decoded result paired with the exact connection that produced it.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileDeviceSessionStepDto {
    /// Identity and admission captured under the same lock as decoding.
    pub session: MobileDeviceSessionSnapshotDto,
    /// Ordered protocol outputs and a typed failure, if any.
    pub result: MobileSessionStepResultDto,
    /// Current normalized telemetry.
    pub telemetry: MobileTelemetrySnapshotDto,
    /// Current parser diagnostics.
    pub diagnostics: MobileParserDiagnosticsDto,
}

impl From<DeviceConnectionSnapshot> for MobileDeviceSessionSnapshotDto {
    fn from(value: DeviceConnectionSnapshot) -> Self {
        MobileDeviceSessionSnapshotDto {
            connection: (&value.connection).into(),
            identity: value.identity.map(|identity| MobileDeviceIdentityDto {
                protocol: cutout_core::ProtocolFamilyDto::from(identity.protocol).into(),
                vehicle_kind: match identity.vehicle_kind {
                    VehicleKind::Unknown => MobileVehicleKindDto::Unknown,
                    VehicleKind::ElectricUnicycle => MobileVehicleKindDto::ElectricUnicycle,
                    VehicleKind::Board => MobileVehicleKindDto::Board,
                    VehicleKind::Scooter => MobileVehicleKindDto::Scooter,
                    VehicleKind::Bike => MobileVehicleKindDto::Bike,
                },
                model: identity.model.map(|model| model.model.to_string()),
            }),
        }
    }
}

impl From<DeviceConnectionStep> for MobileDeviceSessionStepDto {
    fn from(value: DeviceConnectionStep) -> Self {
        Self {
            session: value.session.into(),
            result: cutout_protocols::ConcreteSessionStepResultDto::from(value.result).into(),
            telemetry: value.telemetry.into(),
            diagnostics: value.diagnostics.into(),
        }
    }
}

#[uniffi::export]
impl CutoutSessionStateHandle {
    /// Observes bytes only for the attempt that owned the native callback.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports own byte buffers."
    )]
    pub fn observe_connection_notification(
        &self,
        token: MobileConnectionAttemptTokenDto,
        bytes: Vec<u8>,
    ) -> Option<DeviceDetectionResolutionRecord> {
        self.observe_connection_event(token, DeviceDetectionEvent::Notification { bytes: &bytes })
    }

    /// Feeds one complete GATT inventory to the authoritative detector.
    pub fn observe_connection_gatt(
        &self,
        token: MobileConnectionAttemptTokenDto,
        fingerprints: Vec<MobileGattFingerprintDto>,
    ) -> Option<DeviceDetectionResolutionRecord> {
        let fingerprints = fingerprints.into_iter().map(Into::into).collect::<Vec<_>>();
        self.observe_connection_event(
            token,
            DeviceDetectionEvent::Gatt {
                gatt: &fingerprints,
            },
        )
    }

    /// Retains the selected advertisement as a hint scoped to its attempt.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports own optional byte buffers."
    )]
    pub fn observe_connection_advertisement(
        &self,
        token: MobileConnectionAttemptTokenDto,
        name: Option<Vec<u8>>,
    ) -> Option<DeviceDetectionResolutionRecord> {
        self.observe_connection_event(
            token,
            DeviceDetectionEvent::Advertisement {
                name: name.as_deref(),
            },
        )
    }

    /// Attaches saved controller geometry to the pending attempt without asserting identity.
    pub fn configure_connection_vesc_profile(
        &self,
        token: MobileConnectionAttemptTokenDto,
        profile: crate::VescBoardProfile,
    ) -> bool {
        self.lock_inner()
            .configure_vesc_board_profile(&token.into(), profile.into())
    }

    /// Returns the protocol-selected session without exposing a model constructor.
    #[must_use]
    pub fn device_session_snapshot(&self) -> MobileDeviceSessionSnapshotDto {
        self.lock_inner().snapshot().into()
    }

    /// Completes detection from retained Rust evidence; terminal attempts cannot promote.
    pub fn resolve_device_session(
        &self,
        token: MobileConnectionAttemptTokenDto,
        identification_complete: bool,
        now_ms: u64,
    ) -> MobileDeviceSessionSnapshotDto {
        let mut inner = self.lock_inner();
        inner.resolve(
            &token.into(),
            identification_complete,
            crate::MonotonicTimestamp::new(now_ms),
        );
        inner.snapshot().into()
    }

    /// Computes the complete Rust-owned admission candidate from retained evidence.
    /// Swift consumes this projection; it does not reconstruct route or model policy.
    pub fn connection_admission_candidate(
        &self,
        platform_identifier: String,
        display_name: String,
        allow_closest_match: bool,
    ) -> crate::DiscoveryCandidate {
        let inner = self.lock_inner();
        let resolution: DeviceDetectionResolutionRecord =
            inner.detector().resolution(inner.session_state()).into();
        if allow_closest_match {
            crate::mobile_discovery_candidate_from_closest_detection_resolution(
                platform_identifier,
                display_name,
                resolution,
            )
        } else {
            crate::mobile_discovery_candidate_from_detection_resolution(
                platform_identifier,
                display_name,
                resolution,
            )
        }
    }

    /// Decodes only for the currently verified attempt and pairs the resulting identity.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports an owned input DTO."
    )]
    pub fn ingest_device_session(
        &self,
        token: MobileConnectionAttemptTokenDto,
        input: MobileSessionInputDto,
    ) -> Option<MobileDeviceSessionStepDto> {
        let token = token.into();
        let core_input = input.clone().into();
        let step = self.lock_inner().ingest(&token, &core_input);
        if let Some(step) = &step {
            let telemetry = MobileTelemetrySnapshotDto::from(step.telemetry);
            self.apply_phone_alarm_step(&input, &telemetry, true);
        }
        step.map(Into::into)
    }
}

impl CutoutSessionStateHandle {
    fn observe_connection_event(
        &self,
        token: MobileConnectionAttemptTokenDto,
        event: DeviceDetectionEvent<'_>,
    ) -> Option<DeviceDetectionResolutionRecord> {
        self.lock_inner()
            .observe_for_attempt(&token.into(), event)
            .map(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VESC_REPLY: &[u8] = &[
        2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101, 114, 104, 97, 115, 104, 0,
        38, 208, 3,
    ];

    fn veteran_frame_with_model_id(model_id: u16) -> Vec<u8> {
        let mut frame = vec![0_u8; 42];
        frame[..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 38]);
        frame[28..30].copy_from_slice(&(model_id * 1_000).to_be_bytes());
        frame
    }

    #[test]
    fn falcon_detection_admits_reported_model_with_its_settings_profile() {
        let handle = CutoutSessionStateHandle::new();
        let token = handle
            .begin_connection_attempt("falcon-a".into(), 0)
            .token
            .unwrap();
        handle.connection_link_established(token.clone());
        handle.observe_connection_advertisement(token.clone(), Some(b"GotWay_002441".to_vec()));
        // Fragment boundaries from the physical Falcon's connection capture.
        let notifications: &[&[u8]] = &[
            &[
                90, 90, 90, 90, 85, 170, 0, 75, 0, 0, 3, 214, 0, 0, 0, 0, 19, 136, 0, 0,
            ],
            &[
                0, 0, 1, 3, 90, 90, 90, 90, 85, 170, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            &[
                0, 0, 0, 0, 0, 0, 3, 3, 90, 90, 90, 90, 85, 170, 0, 0, 0, 0, 73, 1,
            ],
            &[
                24, 223, 0, 45, 0, 0, 0, 0, 0, 18, 4, 24, 90, 90, 90, 90, 85, 170, 255, 219,
            ],
            &[
                0, 147, 0, 30, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7, 24, 90, 90, 90, 90,
            ],
        ];
        for bytes in notifications {
            handle.observe_connection_notification(token.clone(), bytes.to_vec());
            let pending = handle.resolve_device_session(token.clone(), false, 1_500);
            assert_eq!(
                pending.connection.readiness,
                crate::MobileConnectionReadinessDto::Pending
            );
        }
        handle.observe_begode_name_probe_for_attempt_at(token.clone(), 1_500);
        let still_detecting = handle.resolve_device_session(token.clone(), true, 2_000);
        assert_eq!(
            still_detecting.connection.readiness,
            crate::MobileConnectionReadinessDto::Pending,
            "a native completion callback cannot bypass an outstanding model query"
        );
        handle.observe_connection_notification(token.clone(), b"NAME:Falcon\r\n".to_vec());
        let candidate =
            handle.connection_admission_candidate("falcon-a".into(), "GotWay_002441".into(), false);
        assert_eq!(
            candidate.support,
            crate::DiscoveryCandidateSupport::Supported
        );
        let resolved = handle.resolve_device_session(token, true, 2_333);
        assert_eq!(resolved.identity.unwrap().model.as_deref(), Some("Falcon"));
        assert!(handle.settings().default_charge_profile.is_some());
        assert!(!handle.settings().setting_descriptors.is_empty());
    }

    #[test]
    fn discovery_retains_each_detected_falcon_across_disconnect_and_advertisement_refresh() {
        let handle = CutoutSessionStateHandle::new();
        for (id, name) in [("A", "GotWay_002441"), ("B", "GotWay_002442")] {
            let observation = crate::DiscoveryObservation {
                platform_identifier: id.into(),
                advertised_name: Some(name.as_bytes().to_vec()),
                advertised_service_uuids: vec![],
                manufacturer_data: vec![],
                rssi_dbm: Some(-60),
            };
            handle.observe_discovery(observation.clone());
            let token = handle.begin_connection_attempt(id.into(), 0).token.unwrap();
            handle.connection_link_established(token.clone());
            let mut frame = vec![0; 24];
            frame[..2].copy_from_slice(&[0x55, 0xaa]);
            frame[18..].copy_from_slice(&[0, 0x18, 0x5a, 0x5a, 0x5a, 0x5a]);
            handle.observe_connection_notification(token.clone(), frame);
            handle.observe_connection_notification(token, b"NAME:Falcon\r\n".to_vec());
            handle.disconnect_connection_attempt();
            handle.observe_discovery(observation);
        }
        let snapshot = handle.discovery_snapshot();
        assert_eq!(snapshot.picker_candidates.len(), 2);
        for (id, name) in [("A", "GotWay_002441"), ("B", "GotWay_002442")] {
            let row = snapshot
                .picker_candidates
                .iter()
                .find(|row| row.platform_identifier == id)
                .unwrap();
            assert_eq!(row.display_name, "Begode Falcon");
            assert!(row.detail.contains(name));
            assert_eq!(
                row.electric_unicycle_model,
                Some(crate::DiscoveryElectricUnicycleModel::Falcon)
            );
        }
        // Retained discovery evidence is not admission evidence for a new connection.
        let token = handle
            .begin_connection_attempt("A".into(), 1)
            .token
            .unwrap();
        let pending = handle.resolve_device_session(token, false, 2);
        assert!(pending.identity.is_none());
    }

    #[test]
    fn picker_selection_cannot_reassign_another_connections_detected_identity() {
        let handle = CutoutSessionStateHandle::new();
        for id in ["A", "B"] {
            handle.observe_discovery(crate::DiscoveryObservation {
                platform_identifier: id.into(),
                advertised_name: Some(id.as_bytes().to_vec()),
                advertised_service_uuids: vec![],
                manufacturer_data: vec![],
                rssi_dbm: None,
            });
        }
        let token = handle
            .begin_connection_attempt("A".into(), 0)
            .token
            .unwrap();
        handle.observe_connection_notification(token, b"NAME:Falcon\r\n".to_vec());
        handle.select_discovered_platform("B".into());
        let snapshot = handle.discovery_snapshot();
        assert_eq!(snapshot.picker_candidates.len(), 1);
        assert_eq!(snapshot.picker_candidates[0].platform_identifier, "A");
        handle.begin_connection_attempt("B".into(), 1);
        assert_eq!(
            handle.discovery_snapshot().picker_candidates[0].platform_identifier,
            "A"
        );
    }

    #[test]
    fn discovery_preserves_detection_when_transport_ends_or_conflicts() {
        for conflicting_reply in [None, Some(VESC_REPLY)] {
            let handle = CutoutSessionStateHandle::new();
            handle.observe_discovery(crate::DiscoveryObservation {
                platform_identifier: "A".into(),
                advertised_name: Some(b"GotWay_002441".to_vec()),
                advertised_service_uuids: vec![],
                manufacturer_data: vec![],
                rssi_dbm: None,
            });
            let token = handle
                .begin_connection_attempt("A".into(), 0)
                .token
                .unwrap();
            handle.connection_link_established(token.clone());
            let mut frame = vec![0; 24];
            frame[..2].copy_from_slice(&[0x55, 0xaa]);
            frame[18..].copy_from_slice(&[0, 0x18, 0x5a, 0x5a, 0x5a, 0x5a]);
            handle.observe_connection_notification(token.clone(), frame);
            handle.observe_connection_notification(token.clone(), b"NAME:Falcon\r\n".to_vec());
            handle.resolve_device_session(token.clone(), false, 1);
            let expected = if let Some(bytes) = conflicting_reply {
                handle.observe_connection_notification(token, bytes.to_vec());
                crate::DiscoveryCandidateSupport::Conflicting
            } else {
                handle.connection_link_down(token);
                crate::DiscoveryCandidateSupport::Supported
            };
            let candidates = handle.discovery_snapshot().picker_candidates;
            assert_eq!(candidates.len(), 1);
            assert_eq!(candidates[0].support, expected);
            assert!(handle.device_session_snapshot().identity.is_none());
        }
    }

    #[test]
    fn expired_attempt_cannot_promote_from_late_protocol_reply() {
        let handle = CutoutSessionStateHandle::new();
        let attempt = handle
            .begin_connection_attempt("A".into(), 0)
            .token
            .unwrap();
        handle.expire_connection_attempt(attempt.clone(), 15_000);
        handle.observe_connection_notification(attempt.clone(), VESC_REPLY.to_vec());
        let snapshot = handle.resolve_device_session(attempt.clone(), true, 15_000);
        assert_eq!(
            snapshot.connection.readiness,
            crate::MobileConnectionReadinessDto::RecordOnly
        );
        assert!(snapshot.identity.is_none());
        assert!(!handle.verified_connection_attempt_is_current(attempt));
    }

    #[test]
    fn verified_session_identity_is_retired_when_same_peripheral_is_retried() {
        let handle = CutoutSessionStateHandle::new();
        let old = handle
            .begin_connection_attempt("A".into(), 0)
            .token
            .unwrap();
        handle.connection_link_established(old.clone());
        handle.observe_connection_notification(old.clone(), VESC_REPLY.to_vec());
        let verified = handle.resolve_device_session(old.clone(), false, 0);
        assert_eq!(
            verified.identity.unwrap().vehicle_kind,
            MobileVehicleKindDto::Unknown
        );
        assert!(handle.verified_connection_attempt_is_current(old.clone()));
        let next = handle.begin_connection_attempt("A".into(), 1);
        assert!(handle.device_session_snapshot().identity.is_none());
        let late = handle.resolve_device_session(old, true, 1);
        assert_eq!(late.connection, next);
        assert!(late.identity.is_none());
    }

    #[test]
    fn old_native_notification_cannot_supply_replacement_protocol_identity() {
        let handle = CutoutSessionStateHandle::new();
        let old = handle
            .begin_connection_attempt("A".into(), 0)
            .token
            .unwrap();
        let current = handle
            .begin_connection_attempt("B".into(), 1)
            .token
            .unwrap();
        assert!(
            handle
                .observe_connection_notification(old, VESC_REPLY.to_vec())
                .is_none()
        );
        let snapshot = handle.resolve_device_session(current, false, 1);
        assert!(snapshot.identity.is_none());
        assert_eq!(
            snapshot.connection.readiness,
            crate::MobileConnectionReadinessDto::Pending
        );
    }

    #[test]
    fn delayed_timer_delivery_does_not_extend_identification_deadline() {
        let handle = CutoutSessionStateHandle::new();
        let token = handle
            .begin_connection_attempt("A".into(), 0)
            .token
            .unwrap();
        handle.observe_connection_notification(token.clone(), VESC_REPLY.to_vec());
        let snapshot = handle.resolve_device_session(token, true, 15_000);
        assert_eq!(
            snapshot.connection.readiness,
            crate::MobileConnectionReadinessDto::RecordOnly
        );
        assert!(snapshot.identity.is_none());
    }

    #[test]
    fn supported_veteran_model_can_admit_a_pending_attempt_from_detection_evidence() {
        let handle = CutoutSessionStateHandle::new();
        let token = handle
            .begin_connection_attempt("NF2557".into(), 0)
            .token
            .unwrap();
        handle.connection_link_established(token.clone());
        handle.observe_connection_notification(token.clone(), veteran_frame_with_model_id(43));

        let candidate =
            handle.connection_admission_candidate("NF2557".into(), "NF2557".into(), false);
        assert_eq!(
            candidate.support,
            crate::DiscoveryCandidateSupport::Supported
        );
        assert_eq!(
            candidate.electric_unicycle_model,
            Some(crate::DiscoveryElectricUnicycleModel::Aero)
        );

        let snapshot = handle.resolve_device_session(token.clone(), true, 1);
        assert_eq!(
            snapshot.connection.readiness,
            crate::MobileConnectionReadinessDto::Verified
        );
        assert_eq!(
            snapshot.identity.unwrap().model.as_deref(),
            Some("NOSFET Aero")
        );
        assert!(handle.verified_connection_attempt_is_current(token));
    }
}
