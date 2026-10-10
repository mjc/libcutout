//! Atomic identity evidence admission for native recording adapters.

use crate::{
    CaptureWriteOutcome, CaptureWriterSlot, DiscoveryElectricUnicycleModel,
    MobileCaptureWriteOutcomeDto, MobilePevcapCaptureBuilder, MobileProtocolFamilyDto,
    MobileResolvedIdentityDto, MobileVerificationStatusDto, MobileVerifiedStringDto,
    format_pevcap_annotation,
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
        let writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        match &*writer {
            CaptureWriterSlot::Ready | CaptureWriterSlot::Recording(_) => {}
            CaptureWriterSlot::Finalizing | CaptureWriterSlot::Complete(_) => {
                return MobileCaptureWriteOutcomeDto::Failed;
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
        .flatten();
        if candidate.try_append(additions).is_err() {
            return MobileCaptureWriteOutcomeDto::Rejected;
        }
        let identity = Some(identity.into());
        if metadata.resolved_identity == identity && candidate.entries() == annotations.entries() {
            return MobileCaptureWriteOutcomeDto::Accepted;
        }
        metadata.resolved_identity.clone_from(&identity);
        metadata.annotations = candidate.entries().to_vec();
        if let Some(writer) = writer.as_ref() {
            match writer.update_metadata(metadata) {
                CaptureWriteOutcome::Accepted => {}
                outcome @ (CaptureWriteOutcome::AdmissionLost { .. }
                | CaptureWriteOutcome::Failed) => {
                    return outcome.into();
                }
            }
        }
        *self
            .resolved_identity
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = identity;
        *annotations = candidate;
        MobileCaptureWriteOutcomeDto::Accepted
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
