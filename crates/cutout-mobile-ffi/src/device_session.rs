//! Generic device-session projection; protocol selection stays below the FFI boundary.

use cutout_core::{ConnectionReadiness, ProtocolFamily, SessionInputDto};
use cutout_protocols::{DeviceSession, VehicleKind};

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

impl crate::MobileSessionState {
    fn device_snapshot(&self) -> MobileDeviceSessionSnapshotDto {
        MobileDeviceSessionSnapshotDto {
            connection: self.state.connection.snapshot().into(),
            identity: self.device.as_ref().map(|device| {
                let identity = device.identity();
                MobileDeviceIdentityDto {
                    protocol: cutout_core::ProtocolFamilyDto::from(identity.protocol).into(),
                    vehicle_kind: match identity.vehicle_kind {
                        VehicleKind::Unknown => MobileVehicleKindDto::Unknown,
                        VehicleKind::ElectricUnicycle => MobileVehicleKindDto::ElectricUnicycle,
                        VehicleKind::Board => MobileVehicleKindDto::Board,
                        VehicleKind::Scooter => MobileVehicleKindDto::Scooter,
                        VehicleKind::Bike => MobileVehicleKindDto::Bike,
                    },
                    model: identity.model.map(|model| model.model.to_string()),
                }
            }),
        }
    }
}

#[uniffi::export]
impl CutoutSessionStateHandle {
    /// Observes bytes only for the attempt that owned the native callback.
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

    /// Returns the protocol-selected session without exposing a model constructor.
    #[must_use]
    pub fn device_session_snapshot(&self) -> MobileDeviceSessionSnapshotDto {
        self.lock_inner().device_snapshot()
    }

    /// Completes detection from retained Rust evidence; terminal attempts cannot promote.
    pub fn resolve_device_session(
        &self,
        token: MobileConnectionAttemptTokenDto,
        identification_complete: bool,
        now_ms: u64,
    ) -> MobileDeviceSessionSnapshotDto {
        let mut inner = self.lock_inner();
        let token = token.into();
        if !inner.state.connection.is_current(&token)
            || inner.state.connection.snapshot().readiness != ConnectionReadiness::Pending
        {
            return inner.device_snapshot();
        }
        if inner
            .state
            .connection
            .expire(&token, crate::MonotonicTimestamp::new(now_ms))
        {
            return inner.device_snapshot();
        }
        let resolution = inner.detector.resolution(&inner.state);
        if let Some(device) = DeviceSession::from_detection(&resolution) {
            let identity = device.identity();
            if identification_complete
                || identity.model.is_some()
                || identity.protocol == ProtocolFamily::Vesc
            {
                inner.state.connection.finish_detection(&token, true);
                inner.device = Some(device);
            }
        } else if identification_complete {
            inner.state.connection.finish_detection(&token, false);
        }
        inner.device_snapshot()
    }

    /// Decodes only for the currently verified attempt and pairs the resulting identity.
    pub fn ingest_device_session(
        &self,
        token: MobileConnectionAttemptTokenDto,
        input: MobileSessionInputDto,
    ) -> Option<MobileDeviceSessionStepDto> {
        let mut inner = self.lock_inner();
        if !inner.state.connection.is_verified(&token.into()) {
            return None;
        }
        let device = inner.device.as_mut()?;
        let result = device.ingest_checked(&SessionInputDto::from(input)).into();
        let telemetry = device.current_snapshot().into();
        let diagnostics = device.diagnostics().into();
        Some(MobileDeviceSessionStepDto {
            session: inner.device_snapshot(),
            result,
            telemetry,
            diagnostics,
        })
    }
}

impl CutoutSessionStateHandle {
    fn observe_connection_event(
        &self,
        token: MobileConnectionAttemptTokenDto,
        event: DeviceDetectionEvent<'_>,
    ) -> Option<DeviceDetectionResolutionRecord> {
        let mut inner = self.lock_inner();
        if !inner.state.connection.is_current(&token.into()) {
            return None;
        }
        let crate::MobileSessionState {
            state, detector, ..
        } = &mut *inner;
        Some(detector.observe(state, event).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VESC_REPLY: &[u8] = &[
        2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101, 114, 104, 97, 115, 104, 0,
        38, 208, 3,
    ];

    #[test]
    fn expired_attempt_cannot_promote_from_late_protocol_reply() {
        let handle = CutoutSessionStateHandle::new();
        let attempt = handle
            .begin_connection_attempt("A".into(), 0)
            .token
            .unwrap();
        handle.expire_connection_attempt(attempt.clone(), 15_000);
        handle.observe_notification(VESC_REPLY.to_vec());
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
        handle.observe_notification(VESC_REPLY.to_vec());
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
}
