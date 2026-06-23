use arrayvec::ArrayString;
use cutout_protocols::{
    IdentityConfidence, IdentityEvidence, ProtocolFamilyClassification, ProtocolFamilyClassifier,
    StagedIdentityInput, StagedIdentityResolution, identify_known_model, parse_model_banner,
};

use crate::{ConnectionSummary, SessionBridgeReport};

const MAX_BANNER_MODEL_LEN: usize = 64;

/// Staged identity resolution surfaced by a BTLE bridge run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeIdentityResolution {
    /// Resolved manufacturer, when model confidence was reached.
    pub manufacturer: Option<&'static str>,

    /// Resolved model, when model confidence was reached.
    pub model: Option<&'static str>,

    /// Confidence reported by staged identification.
    pub confidence: IdentityConfidence,

    /// Evidence that contributed to the decision.
    pub evidence: IdentityEvidence,
}

pub(crate) struct IdentityContext<'a> {
    advertised_name: Option<&'a str>,
    summary: &'a ConnectionSummary,
}

impl<'a> IdentityContext<'a> {
    pub(crate) fn new(summary: &'a ConnectionSummary) -> Self {
        Self {
            advertised_name: summary.observation.name.as_deref(),
            summary,
        }
    }

    fn gatt_fingerprints(&self) -> impl Clone + Iterator<Item = cutout_core::GattFingerprint> + '_ {
        self.summary.iter_gatt_fingerprints()
    }
}

#[derive(Default)]
pub(crate) struct IdentityState {
    stream_family: StreamFamilyEvidence,
    banner_model: Option<BannerModel>,
}

impl IdentityState {
    pub(crate) fn observe(&mut self, bytes: &[u8]) {
        self.stream_family.observe(bytes);
        self.banner_model = parse_model_banner(bytes)
            .and_then(|banner| BannerModel::new(banner.model))
            .or_else(|| self.banner_model.take());
    }

    fn banner_model(&self) -> Option<&str> {
        self.banner_model.as_ref().map(BannerModel::as_str)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StreamFamilyEvidence(ProtocolFamilyClassification);

impl Default for StreamFamilyEvidence {
    fn default() -> Self {
        Self(ProtocolFamilyClassification::Pending)
    }
}

impl StreamFamilyEvidence {
    fn observe(&mut self, bytes: &[u8]) {
        self.0 = (!self.is_known_supported())
            .then(|| ProtocolFamilyClassifier::classify(bytes))
            .filter(|classification| *classification != ProtocolFamilyClassification::Unknown)
            .unwrap_or(self.0);
    }

    const fn is_known_supported(self) -> bool {
        matches!(
            self.0,
            ProtocolFamilyClassification::Known(
                cutout_protocols::DeviceFamily::BegodeFalcon
                    | cutout_protocols::DeviceFamily::NosfetAero
            )
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BannerModel(ArrayString<MAX_BANNER_MODEL_LEN>);

impl BannerModel {
    fn new(model: &str) -> Option<Self> {
        ArrayString::from(model).ok().map(Self)
    }

    fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

pub(crate) fn update_identity_report(
    report: &mut SessionBridgeReport,
    context: &IdentityContext<'_>,
    state: &IdentityState,
) {
    let input = StagedIdentityInput {
        advertised_name: context.advertised_name,
        gatt: context.gatt_fingerprints(),
        stream_family: state.stream_family.0,
        banner_model: state.banner_model(),
    };
    let resolution = identify_known_model(&input);
    report.identity = bridge_identity_resolution(resolution);
}

fn bridge_identity_resolution(
    resolution: StagedIdentityResolution,
) -> Option<BridgeIdentityResolution> {
    (resolution.confidence != IdentityConfidence::NoMatch).then(|| {
        let model = resolution.model;
        BridgeIdentityResolution {
            manufacturer: model.map(|entry| entry.manufacturer),
            model: model.map(|entry| entry.model),
            confidence: resolution.confidence,
            evidence: resolution.evidence,
        }
    })
}

#[cfg(test)]
mod tests {
    use btleplug::api::CharPropFlags;
    use uuid::Uuid;

    use crate::{
        AdvertisedServices, CharacteristicSummary, ConnectionSummary, ManufacturerDataSummaries,
        PeripheralObservation, ServiceSummary,
    };

    use super::{BannerModel, IdentityContext, IdentityState, MAX_BANNER_MODEL_LEN};

    #[test]
    fn banner_model_is_bounded_inline_storage() {
        assert_eq!(
            BannerModel::new("Falcon").expect("short model").as_str(),
            "Falcon"
        );
        assert!(BannerModel::new(&"x".repeat(MAX_BANNER_MODEL_LEN + 1)).is_none());
    }

    #[test]
    fn identity_state_ignores_non_banner_untrusted_bytes() {
        let mut state = IdentityState::default();

        state.observe(&[0x55, 0xaa, 0x20, 0x20]);
        assert_eq!(state.banner_model(), None);

        state.observe(b"NAME=Falcon");
        assert_eq!(state.banner_model(), Some("Falcon"));

        state.observe(&[0x55, 0xaa, 0x20, 0x20]);
        assert_eq!(state.banner_model(), Some("Falcon"));
    }

    #[test]
    fn identity_context_preserves_advertised_name_and_gatt_roles() {
        let summary = ConnectionSummary {
            observation: PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                name: Some("Falcon".to_owned()),
                rssi: Some(-42),
                advertised_services: AdvertisedServices::new(),
                manufacturer_data: ManufacturerDataSummaries::new(),
            },
            services: vec![ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![CharacteristicSummary {
                    uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::WRITE_WITHOUT_RESPONSE | CharPropFlags::NOTIFY,
                }]
                .into(),
            }]
            .into(),
        };

        let context = IdentityContext::new(&summary);
        let gatt = context.gatt_fingerprints().collect::<Vec<_>>();

        assert_eq!(context.advertised_name, Some("Falcon"));
        assert_eq!(gatt.len(), 1);
        assert_eq!(
            gatt[0].service.as_uuid(),
            Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb)
        );
        assert!(gatt[0].roles.supports_write_without_response());
        assert!(gatt[0].roles.supports_notify());
    }
}
