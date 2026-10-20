//! Atomic identity evidence admission for native recording adapters.

use crate::{
    DiscoveryElectricUnicycleModel, MobileCaptureWriteOutcomeDto, MobilePevcapCaptureBuilder,
    MobileProtocolFamilyDto, MobileResolvedIdentityDto, MobileVerificationStatusDto,
    MobileVerifiedStringDto,
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
impl MobilePevcapCaptureBuilder {
    /// Admits identity and its evidence in one metadata update. Rejection preserves
    /// the last admitted identity and annotations, including label-closure capacity.
    #[must_use]
    pub fn update_resolved_identity(
        &self,
        identity: MobileResolvedIdentityDto,
        evidence: Option<String>,
        detail: Option<String>,
    ) -> MobileCaptureWriteOutcomeDto {
        match self
            .writer
            .update_resolved_identity(identity.into(), evidence, detail)
        {
            Ok(()) => MobileCaptureWriteOutcomeDto::Accepted,
            Err(cutout_application::capture_writer::CaptureAnnotationError::CapacityReached) => {
                MobileCaptureWriteOutcomeDto::Rejected
            }
            Err(cutout_application::capture_writer::CaptureAnnotationError::AdmissionLost {
                dropped_messages,
            }) => MobileCaptureWriteOutcomeDto::AdmissionLost { dropped_messages },
            Err(_) => MobileCaptureWriteOutcomeDto::Failed,
        }
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
