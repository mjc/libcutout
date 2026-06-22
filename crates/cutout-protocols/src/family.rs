/// Protocol device family used by capture-backed fixtures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceFamily {
    /// NOSFET/Aero/Veteran family.
    NosfetAero,

    /// Begode/Falcon family.
    BegodeFalcon,
}

/// Classification of a notification stream by protocol family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolFamilyClassification {
    /// The bytes identified a known family.
    Known(DeviceFamily),

    /// The bytes were insufficient to make a decision.
    Pending,

    /// The bytes were definitely not a known family.
    Unknown,
}

/// Transport-independent stream family classifier.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProtocolFamilyClassifier;

impl ProtocolFamilyClassifier {
    /// Classifies a prefix of notification bytes by protocol family.
    #[must_use]
    pub fn classify(bytes: &[u8]) -> ProtocolFamilyClassification {
        if matches_prefix(bytes, &[0xdc, 0x5a, 0x5c]) {
            return ProtocolFamilyClassification::Known(DeviceFamily::NosfetAero);
        }
        if matches_prefix(bytes, &[0x55, 0xaa, 0x19, 0xc1]) {
            return ProtocolFamilyClassification::Known(DeviceFamily::BegodeFalcon);
        }
        if complete_or_pending(bytes, &[0xdc, 0x5a, 0x5c])
            || complete_or_pending(bytes, &[0x55, 0xaa, 0x19, 0xc1])
        {
            return ProtocolFamilyClassification::Pending;
        }
        ProtocolFamilyClassification::Unknown
    }
}

fn complete_or_pending(bytes: &[u8], prefix: &[u8]) -> bool {
    if bytes.len() >= prefix.len() {
        return false;
    }
    prefix.starts_with(bytes)
}

fn matches_prefix(bytes: &[u8], prefix: &[u8]) -> bool {
    bytes.len() >= prefix.len() && bytes.starts_with(prefix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifier_distinguishes_partial_and_complete_prefixes() {
        assert_eq!(
            ProtocolFamilyClassifier::classify(&[0xdc, 0x5a]),
            ProtocolFamilyClassification::Pending
        );
        assert_eq!(
            ProtocolFamilyClassifier::classify(&[0xdc, 0x5a, 0x5c]),
            ProtocolFamilyClassification::Known(DeviceFamily::NosfetAero)
        );
        assert_eq!(
            ProtocolFamilyClassifier::classify(&[0x55, 0xaa, 0x19]),
            ProtocolFamilyClassification::Pending
        );
        assert_eq!(
            ProtocolFamilyClassifier::classify(&[0x55, 0xaa, 0x19, 0xc1]),
            ProtocolFamilyClassification::Known(DeviceFamily::BegodeFalcon)
        );
        assert_eq!(
            ProtocolFamilyClassifier::classify(&[0x01, 0x02, 0x03]),
            ProtocolFamilyClassification::Unknown
        );
    }
}
