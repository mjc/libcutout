//! Atomic identity evidence admission for native recording adapters.

use crate::{
    CaptureWriteOutcome, CaptureWriterSlot, CutoutSessionStateHandle,
    DiscoveryElectricUnicycleModel, MobileCaptureWriteOutcomeDto, MobilePevcapCaptureBuilder,
    MobileProtocolFamilyDto, MobileResolvedIdentityDto, MobileVerificationStatusDto,
    MobileVerifiedStringDto, format_pevcap_annotation,
};

/// Capture identity from a protocol-confirmed model, never a provisional picker selection.
#[must_use]
#[uniffi::export]
pub fn capture_resolved_identity(
    model: Option<DiscoveryElectricUnicycleModel>,
) -> Option<MobileResolvedIdentityDto> {
    let (protocol_family, model) = match model? {
        DiscoveryElectricUnicycleModel::Aero => (
            MobileProtocolFamilyDto::VeteranLeaperkimNosfet,
            "NOSFET Aero",
        ),
        DiscoveryElectricUnicycleModel::Falcon => {
            (MobileProtocolFamilyDto::BegodeGotway, "Begode Falcon")
        }
    };
    Some(MobileResolvedIdentityDto {
        protocol_family: Some(protocol_family),
        model: Some(MobileVerifiedStringDto {
            value: model.into(),
            verification: MobileVerificationStatusDto::HardwareVerified,
        }),
        firmware: None,
    })
}
#[uniffi::export]
impl CutoutSessionStateHandle {
    /// Reconciles protocol proof for the capture's original connection attempt.
    /// Pending attempts bind ownership without publishing identity; conflicts revoke proof.
    #[allow(clippy::needless_pass_by_value)] // UniFFI receives an owned object handle.
    pub fn update_capture_identity(
        &self,
        token: crate::MobileConnectionAttemptTokenDto,
        builder: std::sync::Arc<MobilePevcapCaptureBuilder>,
    ) -> Option<MobileCaptureWriteOutcomeDto> {
        let owner = self.lock_inner();
        let token: cutout_core::ConnectionAttemptToken = token.into();
        if builder.platform_id != token.platform_identifier() {
            return None;
        }
        let update = owner.capture_identity(&token)?;
        let mut admission = builder
            .capture_identity_admission
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let expected = crate::CaptureIdentityAdmission {
            owner_id: self.capture_identity_owner_id,
            token: token.clone(),
        };
        match admission.as_ref() {
            Some(bound) if bound != &expected => return None,
            Some(_) => {}
            None => {
                match &update {
                    cutout_protocols::CaptureIdentityUpdate::Invalidated => return None,
                    cutout_protocols::CaptureIdentityUpdate::Unresolved
                    | cutout_protocols::CaptureIdentityUpdate::Resolved(_) => {}
                }
                *admission = Some(expected);
            }
        }
        let (identity, evidence, detail) = match update {
            cutout_protocols::CaptureIdentityUpdate::Unresolved => return None,
            cutout_protocols::CaptureIdentityUpdate::Resolved(identity) => {
                let resolution = owner.detector().resolution(owner.session_state());
                let display_name = resolution
                    .advertised_name
                    .as_ref()
                    .and_then(cutout_core::AdvertisedName::get)
                    .unwrap_or_else(|| token.platform_identifier())
                    .to_owned();
                let candidate = crate::mobile_discovery_candidate_from_detection_resolution(
                    token.platform_identifier().to_owned(),
                    display_name,
                    resolution.into(),
                );
                (identity, Some(candidate.evidence), Some(candidate.detail))
            }
            cutout_protocols::CaptureIdentityUpdate::Invalidated => (
                cutout_core::PevcapResolvedIdentity {
                    protocol_family: None,
                    model: None,
                    firmware: None,
                },
                None,
                None,
            ),
        };
        // Keep both ownership locks through admission. Invalidating protocol proof
        // consumes no annotation capacity and preserves historical provenance.
        builder.update_capture_identity_metadata(identity, evidence, detail)
    }
}

use std::sync::PoisonError;

#[uniffi::export]
impl MobilePevcapCaptureBuilder {
    /// Admits identity and its evidence in one metadata update. Rejection preserves
    /// the last admitted identity and annotations, including label-closure capacity.
    pub fn update_resolved_identity(
        &self,
        identity: MobileResolvedIdentityDto,
        evidence: Option<String>,
        detail: Option<String>,
    ) -> MobileCaptureWriteOutcomeDto {
        self.update_capture_identity_metadata(identity.into(), evidence, detail)
            .unwrap_or(MobileCaptureWriteOutcomeDto::Accepted)
    }
}

impl MobilePevcapCaptureBuilder {
    fn update_capture_identity_metadata(
        &self,
        identity: cutout_core::PevcapResolvedIdentity,
        evidence: Option<String>,
        detail: Option<String>,
    ) -> Option<MobileCaptureWriteOutcomeDto> {
        let writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        match &*writer {
            CaptureWriterSlot::Ready | CaptureWriterSlot::Recording(_) => {}
            CaptureWriterSlot::Finalizing | CaptureWriterSlot::Complete(_) => {
                return Some(MobileCaptureWriteOutcomeDto::Failed);
            }
        }
        let mut metadata = self.metadata();
        let mut annotations = self
            .annotations
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut candidate = annotations.clone();
        let additions = [
            evidence.map(|value| format_pevcap_annotation("resolved_evidence".into(), value)),
            detail.map(|value| format_pevcap_annotation("resolved_detail".into(), value)),
        ]
        .into_iter()
        .flatten()
        .filter(|entry| !annotations.entries().contains(entry));
        if candidate.try_append(additions).is_err() {
            return Some(MobileCaptureWriteOutcomeDto::Rejected);
        }
        let identity = Some(identity);
        if metadata.resolved_identity == identity && candidate.entries() == annotations.entries() {
            return None;
        }
        metadata.resolved_identity.clone_from(&identity);
        metadata.annotations = candidate.entries().to_vec();
        if let Some(writer) = writer.as_ref() {
            match writer.update_metadata(metadata) {
                CaptureWriteOutcome::Accepted => {}
                outcome @ (CaptureWriteOutcome::AdmissionLost { .. }
                | CaptureWriteOutcome::Failed) => {
                    return Some(outcome.into());
                }
            }
        }
        *self
            .resolved_identity
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = identity;
        *annotations = candidate;
        Some(MobileCaptureWriteOutcomeDto::Accepted)
    }
}

#[cfg(test)]
mod tests {
    use super::capture_resolved_identity;
    use crate::{
        DiscoveryElectricUnicycleModel, MobileCaptureWriteOutcomeDto, MobilePevcapCaptureBuilder,
        MobileProtocolFamilyDto, MobileResolvedIdentityDto, MobileVerificationStatusDto,
        MobileWallClockUnixMillisDto,
    };

    const VESC_REPLY: &[u8] = &[
        2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101, 114, 104, 97, 115, 104, 0,
        38, 208, 3,
    ];

    fn builder() -> std::sync::Arc<MobilePevcapCaptureBuilder> {
        MobilePevcapCaptureBuilder::new(
            MobileWallClockUnixMillisDto {
                milliseconds: 1_700_000_000_000,
            },
            "capture-device".into(),
            None,
        )
    }

    fn begin(handle: &crate::CutoutSessionStateHandle) -> crate::MobileConnectionAttemptTokenDto {
        let token = handle
            .begin_connection_attempt("capture-device".into(), 0)
            .token
            .unwrap();
        handle.connection_link_established(token.clone());
        token
    }

    fn identify_vesc(
        handle: &crate::CutoutSessionStateHandle,
        token: &crate::MobileConnectionAttemptTokenDto,
    ) {
        handle.observe_connection_notification(token.clone(), VESC_REPLY.to_vec());
        assert_eq!(
            handle
                .resolve_device_session(token.clone(), false, 1)
                .connection
                .readiness,
            crate::MobileConnectionReadinessDto::Verified
        );
    }

    #[test]
    fn capture_owner_persists_verified_vesc_without_a_model_before_or_after_writer_start() {
        for identify_before_start in [false, true] {
            let handle = crate::CutoutSessionStateHandle::new();
            let token = begin(&handle);
            if identify_before_start {
                identify_vesc(&handle, &token);
            }
            let builder = builder();
            let path = std::env::temp_dir().join(format!(
                "capture-owner-{}-{identify_before_start}.jsonl",
                std::process::id()
            ));
            let _ = std::fs::remove_file(&path);
            assert!(builder.set_capture_start_monotonic_ms(0));
            assert!(builder.start_writer(path.to_string_lossy().into_owned()));
            if !identify_before_start {
                identify_vesc(&handle, &token);
            }
            assert_eq!(
                handle.update_capture_identity(token.clone(), builder.clone()),
                Some(MobileCaptureWriteOutcomeDto::Accepted)
            );
            let before = builder.metadata();
            assert_eq!(handle.update_capture_identity(token, builder.clone()), None);
            assert_eq!(builder.metadata().annotations, before.annotations);
            assert_eq!(
                builder.record_notification(
                    crate::MobileMonotonicMillisDto { milliseconds: 2 },
                    vec![0; 16],
                    vec![1; 16],
                    VESC_REPLY.to_vec()
                ),
                MobileCaptureWriteOutcomeDto::Accepted
            );
            assert!(builder.finish_writer());
            let capture = cutout_core::PevcapCapture::decode(
                &std::fs::read(&path).unwrap(),
                cutout_core::PevcapEncoding::Jsonl,
            )
            .unwrap();
            assert_eq!(
                capture.header.resolved_identity,
                Some(cutout_core::PevcapResolvedIdentity {
                    protocol_family: Some(cutout_core::ProtocolFamily::Vesc),
                    model: None,
                    firmware: None,
                })
            );
            assert_eq!(capture.records[0].bytes.as_ref(), VESC_REPLY);
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn capture_owner_rejects_unverified_wrong_stale_record_only_and_conflicted_attempts() {
        let handle = crate::CutoutSessionStateHandle::new();
        let token = begin(&handle);
        let builder = builder();
        // Advertisements, pending detection, and provisional physical geometry cannot grant identity.
        handle.observe_connection_advertisement(token.clone(), Some(b"VESC".to_vec()));
        assert_eq!(
            handle.update_capture_identity(token.clone(), builder.clone()),
            None
        );
        assert!(builder.metadata().resolved_identity.is_none());
        identify_vesc(&handle, &token);
        let old = token;
        let token = begin(&handle);
        assert_eq!(handle.update_capture_identity(old, builder.clone()), None);
        handle.resolve_device_session(token.clone(), true, 2);
        assert_eq!(
            handle.update_capture_identity(token.clone(), builder.clone()),
            None
        );
        assert!(builder.metadata().resolved_identity.is_none());
        let token = begin(&handle);
        identify_vesc(&handle, &token);
        let mut wrong = token.clone();
        wrong.platform_identifier = "another-device".into();
        assert_eq!(handle.update_capture_identity(wrong, builder.clone()), None);
        let mut begode_frame = vec![0; 24];
        begode_frame[..2].copy_from_slice(&[0x55, 0xaa]);
        begode_frame[18..].copy_from_slice(&[0, 0x18, 0x5a, 0x5a, 0x5a, 0x5a]);
        handle.observe_connection_notification(token.clone(), begode_frame);
        assert_eq!(handle.update_capture_identity(token, builder.clone()), None);
        assert!(builder.metadata().resolved_identity.is_none());
        assert_eq!(builder.metadata().annotations, Vec::<String>::new());
    }

    fn conflicting_frame() -> Vec<u8> {
        let mut frame = vec![0; 24];
        frame[..2].copy_from_slice(&[0x55, 0xaa]);
        frame[18..].copy_from_slice(&[0, 0x18, 0x5a, 0x5a, 0x5a, 0x5a]);
        frame
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one full-capacity live capture must retain both sides of protocol invalidation"
    )]
    fn capture_owner_invalidates_conflicted_identity_in_export_at_annotation_capacity() {
        let handle = crate::CutoutSessionStateHandle::new();
        let token = begin(&handle);
        identify_vesc(&handle, &token);
        let builder = builder();
        let path =
            std::env::temp_dir().join(format!("capture-conflict-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        assert!(builder.set_capture_start_monotonic_ms(0));
        assert!(builder.start_writer(path.to_string_lossy().into_owned()));
        assert_eq!(
            handle.update_capture_identity(token.clone(), builder.clone()),
            Some(MobileCaptureWriteOutcomeDto::Accepted)
        );
        builder.add_annotation("protocol_detection_resolved=vesc".into());
        while builder.metadata().annotations.len() < cutout_core::PEVCAP_MAX_ANNOTATIONS {
            let index = builder.metadata().annotations.len();
            assert_eq!(
                builder.add_annotation(format!("history={index}")),
                MobileCaptureWriteOutcomeDto::Accepted
            );
        }
        let annotations = builder.metadata().annotations;
        assert_eq!(
            builder.flush_writer_outcome(),
            crate::MobileCaptureFlushOutcomeDto::Flushed
        );
        let published = cutout_core::PevcapCapture::decode(
            &std::fs::read(&path).unwrap(),
            cutout_core::PevcapEncoding::Jsonl,
        )
        .unwrap();
        assert_eq!(
            published
                .header
                .resolved_identity
                .as_ref()
                .unwrap()
                .protocol_family,
            Some(cutout_core::ProtocolFamily::Vesc)
        );
        let conflict = conflicting_frame();
        for (index, bytes) in [VESC_REPLY.to_vec(), conflict.clone()]
            .into_iter()
            .enumerate()
        {
            assert_eq!(
                builder.record_notification(
                    crate::MobileMonotonicMillisDto {
                        milliseconds: u64::try_from(index).unwrap() + 1
                    },
                    vec![0; 16],
                    vec![1; 16],
                    bytes
                ),
                MobileCaptureWriteOutcomeDto::Accepted
            );
        }
        handle.observe_connection_notification(token.clone(), conflict.clone());
        assert_eq!(
            handle.device_session_snapshot().connection.readiness,
            crate::MobileConnectionReadinessDto::Conflicted
        );
        assert_eq!(
            handle.update_capture_identity(token.clone(), builder.clone()),
            Some(MobileCaptureWriteOutcomeDto::Accepted)
        );
        assert_eq!(handle.update_capture_identity(token, builder.clone()), None);
        assert_eq!(builder.metadata().annotations, annotations);
        assert!(builder.finish_writer());
        let capture = cutout_core::PevcapCapture::decode(
            &std::fs::read(&path).unwrap(),
            cutout_core::PevcapEncoding::Jsonl,
        )
        .unwrap();
        assert_eq!(
            capture.header.resolved_identity,
            Some(cutout_core::PevcapResolvedIdentity {
                protocol_family: None,
                model: None,
                firmware: None,
            })
        );
        let binary = capture.encode(cutout_core::PevcapEncoding::Binary).unwrap();
        let binary_capture =
            cutout_core::PevcapCapture::decode(&binary, cutout_core::PevcapEncoding::Binary)
                .unwrap();
        assert_eq!(
            binary_capture.header.resolved_identity,
            capture.header.resolved_identity
        );
        assert_eq!(
            binary_capture.header.annotations,
            capture.header.annotations
        );
        assert_eq!(binary_capture.records, capture.records);
        assert_eq!(
            capture.header.annotations.as_slice(),
            annotations.as_slice()
        );
        assert_eq!(capture.records[0].bytes.as_ref(), VESC_REPLY);
        assert_eq!(capture.records[1].bytes.as_ref(), conflict);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn capture_owner_invalidation_is_fenced_to_the_publishing_attempt_and_writer_receipt() {
        let handle = crate::CutoutSessionStateHandle::new();
        let old = begin(&handle);
        identify_vesc(&handle, &old);
        let old_builder = builder();
        assert_eq!(
            handle.update_capture_identity(old.clone(), old_builder.clone()),
            Some(MobileCaptureWriteOutcomeDto::Accepted)
        );
        let old_metadata = old_builder.metadata();
        let fresh = begin(&handle);
        identify_vesc(&handle, &fresh);
        let fresh_builder = builder();
        assert_eq!(
            handle.update_capture_identity(fresh.clone(), fresh_builder.clone()),
            Some(MobileCaptureWriteOutcomeDto::Accepted)
        );
        handle.observe_connection_notification(fresh.clone(), conflicting_frame());
        assert_eq!(
            handle.update_capture_identity(fresh.clone(), old_builder.clone()),
            None
        );
        assert_eq!(
            handle.update_capture_identity(old, fresh_builder.clone()),
            None
        );
        assert_eq!(
            old_builder.metadata().resolved_identity,
            old_metadata.resolved_identity
        );
        let before = fresh_builder.metadata();
        *fresh_builder.writer.lock().unwrap() = crate::CaptureWriterSlot::Finalizing;
        assert_eq!(
            handle.update_capture_identity(fresh, fresh_builder.clone()),
            Some(MobileCaptureWriteOutcomeDto::Failed)
        );
        assert_eq!(
            fresh_builder.metadata().resolved_identity,
            before.resolved_identity
        );
        assert_eq!(fresh_builder.metadata().annotations, before.annotations);
    }

    #[test]
    fn capture_owner_binds_pending_writers_and_rejects_replacement_or_foreign_owners() {
        let handle = crate::CutoutSessionStateHandle::new();
        let first = begin(&handle);
        let pending = builder();
        assert_eq!(
            handle.update_capture_identity(first.clone(), pending.clone()),
            None
        );
        assert!(pending.metadata().resolved_identity.is_none());
        let foreign = crate::CutoutSessionStateHandle::new();
        let foreign_token = begin(&foreign);
        assert_eq!(
            first, foreign_token,
            "attempt tokens are scoped to their owner"
        );
        identify_vesc(&foreign, &foreign_token);
        assert_eq!(
            foreign.update_capture_identity(foreign_token, pending.clone()),
            None
        );
        let fresh = begin(&handle);
        identify_vesc(&handle, &fresh);
        assert_eq!(
            handle.update_capture_identity(fresh.clone(), pending.clone()),
            None
        );
        assert!(pending.metadata().resolved_identity.is_none());
        let replacement = builder();
        assert_eq!(
            handle.update_capture_identity(first, replacement.clone()),
            None
        );
        assert_eq!(
            handle.update_capture_identity(fresh.clone(), replacement.clone()),
            Some(MobileCaptureWriteOutcomeDto::Accepted)
        );
        let before_disconnect = replacement.metadata();
        handle.connection_link_down(fresh.clone());
        assert_eq!(
            handle.update_capture_identity(fresh, replacement.clone()),
            None
        );
        assert_eq!(
            replacement.metadata().resolved_identity,
            before_disconnect.resolved_identity
        );
        assert_eq!(
            replacement.metadata().annotations,
            before_disconnect.annotations
        );
    }

    #[test]
    fn capture_owner_preserves_confirmed_euc_model_and_atomic_metadata_failures() {
        let handle = crate::CutoutSessionStateHandle::new();
        let token = begin(&handle);
        let mut frame = vec![0; 24];
        frame[..2].copy_from_slice(&[0x55, 0xaa]);
        frame[18..].copy_from_slice(&[0, 0x18, 0x5a, 0x5a, 0x5a, 0x5a]);
        handle.observe_connection_notification(token.clone(), frame);
        handle.observe_connection_notification(token.clone(), b"NAME:Falcon\r\n".to_vec());
        handle.resolve_device_session(token.clone(), true, 1);
        let builder = builder();
        assert_eq!(
            handle.update_capture_identity(token.clone(), builder.clone()),
            Some(MobileCaptureWriteOutcomeDto::Accepted)
        );
        let identity = builder.metadata().resolved_identity.unwrap();
        assert_eq!(
            identity.protocol_family,
            Some(cutout_core::ProtocolFamily::BegodeGotway)
        );
        let model = identity.model.unwrap();
        assert_eq!(model.value, "Begode Falcon");
        assert_eq!(
            model.verification,
            cutout_core::VerificationStatus::HardwareVerified
        );
        let annotations = builder.metadata().annotations;
        assert!(
            annotations
                .iter()
                .any(|entry| entry.starts_with("resolved_evidence="))
        );
        assert_eq!(
            handle.update_capture_identity(token.clone(), builder.clone()),
            None
        );
        assert_eq!(builder.metadata().annotations, annotations);
        let full = self::builder();
        for index in 0..cutout_core::PEVCAP_MAX_ANNOTATIONS - 1 {
            full.add_annotation(format!("note={index}"));
        }
        assert_eq!(
            handle.update_capture_identity(token, full.clone()),
            Some(MobileCaptureWriteOutcomeDto::Rejected)
        );
        assert!(full.metadata().resolved_identity.is_none());
        assert_eq!(
            full.metadata().annotations.len(),
            cutout_core::PEVCAP_MAX_ANNOTATIONS - 1
        );
    }

    #[test]
    fn capture_owner_retains_display_name_provenance_and_publishes_changed_evidence() {
        for (advertised_name, expected_display) in [
            (Some(b"GotWay_002441".to_vec()), "GotWay_002441"),
            (Some(vec![0xff, 0xfe]), "capture-device"),
            (None, "capture-device"),
        ] {
            let handle = crate::CutoutSessionStateHandle::new();
            let token = begin(&handle);
            handle.observe_connection_advertisement(token.clone(), advertised_name);
            handle.observe_connection_notification(token.clone(), conflicting_frame());
            handle.observe_connection_notification(token.clone(), b"NAME:Falcon\r\n".to_vec());
            handle.resolve_device_session(token.clone(), true, 1);
            let builder = builder();
            assert_eq!(
                handle.update_capture_identity(token.clone(), builder.clone()),
                Some(MobileCaptureWriteOutcomeDto::Accepted)
            );
            let metadata = builder.metadata();
            assert!(
                metadata
                    .annotations
                    .iter()
                    .any(|annotation| annotation.starts_with("resolved_detail=")
                        && annotation.ends_with(&format!("; advertised as {expected_display}"))),
                "advertised-name provenance must describe the retained advertisement: {:?}",
                metadata.annotations
            );
            assert_eq!(
                metadata.resolved_identity.unwrap().model.unwrap().value,
                "Begode Falcon"
            );
            assert_eq!(
                handle.update_capture_identity(token.clone(), builder.clone()),
                None
            );
            handle.observe_connection_advertisement(
                token.clone(),
                Some(b"Changed-friendly-name".to_vec()),
            );
            assert_eq!(
                handle.update_capture_identity(token.clone(), builder.clone()),
                Some(MobileCaptureWriteOutcomeDto::Accepted)
            );
            let changed = builder.metadata();
            assert!(
                changed
                    .annotations
                    .iter()
                    .any(|annotation| annotation.starts_with("resolved_detail=")
                        && annotation.ends_with("; advertised as Changed-friendly-name"))
            );
            assert_eq!(handle.update_capture_identity(token, builder.clone()), None);
            assert_eq!(builder.metadata().annotations, changed.annotations);
        }
    }

    #[test]
    fn atomic_capture_metadata_distinguishes_model_and_evidence_changes_from_no_ops() {
        let builder = builder();
        let identity: cutout_core::PevcapResolvedIdentity =
            capture_resolved_identity(Some(DiscoveryElectricUnicycleModel::Falcon))
                .unwrap()
                .into();
        assert_eq!(
            builder.update_capture_identity_metadata(identity.clone(), None, None),
            Some(MobileCaptureWriteOutcomeDto::Accepted)
        );
        assert_eq!(
            builder.update_capture_identity_metadata(identity.clone(), None, None),
            None
        );
        let mut changed = identity;
        changed.model.as_mut().unwrap().value = "Updated model metadata".into();
        assert_eq!(
            builder.update_capture_identity_metadata(changed.clone(), None, None),
            Some(MobileCaptureWriteOutcomeDto::Accepted)
        );
        assert_eq!(builder.metadata().resolved_identity, Some(changed.clone()));
        assert_eq!(
            builder.update_capture_identity_metadata(
                changed.clone(),
                Some("new evidence".into()),
                None
            ),
            Some(MobileCaptureWriteOutcomeDto::Accepted)
        );
        assert_eq!(
            builder.update_capture_identity_metadata(changed, Some("new evidence".into()), None),
            None
        );
        let public_identity =
            capture_resolved_identity(Some(DiscoveryElectricUnicycleModel::Falcon)).unwrap();
        assert_eq!(
            builder.update_resolved_identity(public_identity.clone(), None, None),
            MobileCaptureWriteOutcomeDto::Accepted
        );
        assert_eq!(
            builder.update_resolved_identity(public_identity, None, None),
            MobileCaptureWriteOutcomeDto::Accepted
        );
    }

    #[test]
    fn capture_identity_uses_only_the_confirmed_model_and_shared_protocol_mapping() {
        assert_eq!(capture_resolved_identity(None), None);
        for (model, protocol, name) in [
            (
                DiscoveryElectricUnicycleModel::Aero,
                MobileProtocolFamilyDto::VeteranLeaperkimNosfet,
                "NOSFET Aero",
            ),
            (
                DiscoveryElectricUnicycleModel::Falcon,
                MobileProtocolFamilyDto::BegodeGotway,
                "Begode Falcon",
            ),
        ] {
            let identity = capture_resolved_identity(Some(model)).unwrap();
            assert_eq!(identity.protocol_family, Some(protocol));
            let model = identity.model.unwrap();
            assert_eq!(model.value, name);
            assert_eq!(
                model.verification,
                MobileVerificationStatusDto::HardwareVerified
            );
            assert_eq!(identity.firmware, None);
        }
    }

    #[test]
    fn resolved_identity_batch_rejects_without_publishing_partial_metadata() {
        let builder = MobilePevcapCaptureBuilder::new(
            MobileWallClockUnixMillisDto {
                milliseconds: 1_700_000_000_000,
            },
            "wheel".into(),
            None,
        );
        for index in 0..cutout_core::PEVCAP_MAX_ANNOTATIONS - 1 {
            assert_eq!(
                builder.add_annotation(format!("note={index}")),
                MobileCaptureWriteOutcomeDto::Accepted
            );
        }
        let before = builder.metadata();
        let identity = MobileResolvedIdentityDto {
            protocol_family: Some(MobileProtocolFamilyDto::VeteranLeaperkimNosfet),
            model: None,
            firmware: None,
        };
        let outcome = builder.update_resolved_identity(
            identity,
            Some("verified".into()),
            Some("complete".into()),
        );
        assert_eq!(outcome, MobileCaptureWriteOutcomeDto::Rejected);
        assert_eq!(
            builder.metadata().resolved_identity,
            before.resolved_identity
        );
        assert_eq!(builder.metadata().annotations, before.annotations);
    }

    #[test]
    fn resolved_identity_batch_publishes_all_evidence_with_shared_sanitization() {
        let builder = MobilePevcapCaptureBuilder::new(
            MobileWallClockUnixMillisDto {
                milliseconds: 1_700_000_000_000,
            },
            "wheel".into(),
            None,
        );
        let identity = MobileResolvedIdentityDto {
            protocol_family: Some(MobileProtocolFamilyDto::VeteranLeaperkimNosfet),
            model: None,
            firmware: None,
        };
        assert_eq!(
            builder.update_resolved_identity(
                identity.clone(),
                Some("verified=packet\nnext".into()),
                Some("a\rb".into())
            ),
            MobileCaptureWriteOutcomeDto::Accepted
        );
        let metadata = builder.metadata();
        assert_eq!(metadata.resolved_identity, Some(identity.into()));
        assert_eq!(
            metadata.annotations,
            [
                "resolved_evidence=verified packet next",
                "resolved_detail=a b"
            ]
        );
    }
}
