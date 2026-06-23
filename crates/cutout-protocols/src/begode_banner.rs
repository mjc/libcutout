/// Begode/Gotway ASCII banner evidence observed on the shared notify pipe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BegodeBanner<'a> {
    /// Firmware banner returned by the `V` query.
    Firmware {
        /// Recognized firmware family prefix.
        prefix: BegodeFirmwarePrefix,

        /// Complete trimmed banner text.
        banner: &'a str,
    },

    /// Model-name banner returned by the `N` query.
    ModelName(&'a str),

    /// IMU banner used as temperature formula evidence.
    Imu(BegodeImuKind<'a>),
}

/// Result of classifying a raw Begode/Gotway banner candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BegodeBannerParse<'a> {
    /// The bytes were a recognized banner.
    Banner(BegodeBanner<'a>),

    /// No bytes were provided.
    Empty,

    /// The payload has the binary realtime frame prefix, not an ASCII banner.
    BinaryFrame,

    /// The payload contains bytes outside the accepted banner text subset.
    NonAscii,

    /// The payload is ASCII text, but not a recognized Begode/Gotway banner.
    UnknownText,
}

impl<'a> BegodeBannerParse<'a> {
    /// Returns the parsed banner when classification recognized one.
    #[must_use]
    pub const fn banner(self) -> Option<BegodeBanner<'a>> {
        match self {
            Self::Banner(banner) => Some(banner),
            Self::Empty | Self::BinaryFrame | Self::NonAscii | Self::UnknownText => None,
        }
    }
}

/// Recognized Begode/Gotway firmware banner prefix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BegodeFirmwarePrefix {
    /// Stock Begode/Gotway firmware prefix (`GW`).
    Gotway,

    /// `ExtremeBull` firmware prefix (`JN`).
    ExtremeBull,

    /// Freestyl3r custom firmware prefix (`CF`).
    Freestyl3r,

    /// `SmirnoV` custom firmware prefix (`BF`).
    SmirnoV,
}

impl BegodeFirmwarePrefix {
    /// Returns whether this firmware family exposes an authoritative hardware PWM field.
    #[must_use]
    pub const fn uses_hardware_pwm(self) -> bool {
        matches!(self, Self::Freestyl3r | Self::SmirnoV)
    }
}

/// IMU family evidence from a Begode `MPU...` banner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BegodeImuKind<'a> {
    /// MPU6050 native temperature formula evidence.
    Mpu6050,

    /// MPU6500 temperature formula evidence.
    Mpu6500,

    /// Unknown MPU suffix retained for diagnostics.
    Unknown(&'a str),
}

/// Parses a single Begode/Gotway ASCII identity/config banner.
///
/// Returns `None` for binary Begode realtime frames and for text that does not
/// match the recognized banner prefixes.
#[must_use]
pub fn parse_begode_ascii_banner(bytes: &[u8]) -> Option<BegodeBanner<'_>> {
    classify_begode_ascii_banner(bytes).banner()
}

/// Classifies a raw Begode/Gotway ASCII identity/config banner candidate.
///
/// The returned enum keeps the untrusted byte boundary explicit while allowing
/// callers that only care about recognized banners to use
/// [`parse_begode_ascii_banner`].
#[must_use]
pub fn classify_begode_ascii_banner(bytes: &[u8]) -> BegodeBannerParse<'_> {
    if bytes.is_empty() {
        return BegodeBannerParse::Empty;
    }
    if bytes.starts_with(&[0x55, 0xaa]) {
        return BegodeBannerParse::BinaryFrame;
    }
    if !bytes.iter().copied().all(is_banner_ascii) {
        return BegodeBannerParse::NonAscii;
    }

    let Ok(text) = core::str::from_utf8(bytes) else {
        return BegodeBannerParse::NonAscii;
    };
    let text = text.trim();
    if text.is_empty() {
        return BegodeBannerParse::Empty;
    }

    parse_firmware_banner(text)
        .or_else(|| parse_model_name_banner(text))
        .or_else(|| parse_imu_banner(text))
        .map_or(BegodeBannerParse::UnknownText, BegodeBannerParse::Banner)
}

fn parse_firmware_banner(text: &str) -> Option<BegodeBanner<'_>> {
    let prefix = if text.starts_with("GW") {
        BegodeFirmwarePrefix::Gotway
    } else if text.starts_with("JN") {
        BegodeFirmwarePrefix::ExtremeBull
    } else if text.starts_with("CF") {
        BegodeFirmwarePrefix::Freestyl3r
    } else if text.starts_with("BF") {
        BegodeFirmwarePrefix::SmirnoV
    } else {
        return None;
    };

    Some(BegodeBanner::Firmware {
        prefix,
        banner: text,
    })
}

fn parse_model_name_banner(text: &str) -> Option<BegodeBanner<'_>> {
    let model = text
        .strip_prefix("NAME")?
        .trim_start_matches([':', '=', ' ', '\t'])
        .trim();

    (!model.is_empty()).then_some(BegodeBanner::ModelName(model))
}

fn parse_imu_banner(text: &str) -> Option<BegodeBanner<'_>> {
    let suffix = text.strip_prefix("MPU")?.trim();
    if suffix.is_empty() {
        return None;
    }

    Some(BegodeBanner::Imu(match suffix {
        "6050" => BegodeImuKind::Mpu6050,
        "6500" => BegodeImuKind::Mpu6500,
        _ => BegodeImuKind::Unknown(suffix),
    }))
}

const fn is_banner_ascii(byte: u8) -> bool {
    matches!(byte, b'\n' | b'\r' | b'\t' | 0x20..=0x7e)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use crate::{
        BegodeBanner, BegodeBannerParse, BegodeFirmwarePrefix, BegodeImuKind,
        classify_begode_ascii_banner, parse_begode_ascii_banner,
    };

    #[test]
    fn classifies_untrusted_banner_boundary_without_allocating_decisions() {
        assert_eq!(classify_begode_ascii_banner(b""), BegodeBannerParse::Empty);
        assert_eq!(
            classify_begode_ascii_banner(&[0x55, 0xaa, 0x20, 0x20]),
            BegodeBannerParse::BinaryFrame
        );
        assert_eq!(
            classify_begode_ascii_banner(b"NAME=Falcon\x00"),
            BegodeBannerParse::NonAscii
        );
        assert_eq!(
            classify_begode_ascii_banner(b"hello but not a banner"),
            BegodeBannerParse::UnknownText
        );
        assert_eq!(
            classify_begode_ascii_banner(b"NAME=Falcon"),
            BegodeBannerParse::Banner(BegodeBanner::ModelName("Falcon"))
        );
    }

    #[test]
    fn parses_name_banner_with_equals_or_colon_separator() {
        assert_eq!(
            parse_begode_ascii_banner(b"\r\nNAME=Falcon\n"),
            Some(BegodeBanner::ModelName("Falcon"))
        );
        assert_eq!(
            parse_begode_ascii_banner(b"NAME:Master"),
            Some(BegodeBanner::ModelName("Master"))
        );
    }

    #[test]
    fn parses_known_firmware_banner_prefixes() {
        let gotway = parse_begode_ascii_banner(b"GW2-MASTER-1.42");
        let extreme_bull = parse_begode_ascii_banner(b"JN FALCON 1.0");
        let freestyl3r = parse_begode_ascii_banner(b"CF 5.0");
        let smirnov = parse_begode_ascii_banner(b"BF V5.3 CFW");

        assert_eq!(
            gotway,
            Some(BegodeBanner::Firmware {
                prefix: BegodeFirmwarePrefix::Gotway,
                banner: "GW2-MASTER-1.42",
            })
        );
        assert_eq!(
            extreme_bull,
            Some(BegodeBanner::Firmware {
                prefix: BegodeFirmwarePrefix::ExtremeBull,
                banner: "JN FALCON 1.0",
            })
        );
        assert_eq!(
            freestyl3r,
            Some(BegodeBanner::Firmware {
                prefix: BegodeFirmwarePrefix::Freestyl3r,
                banner: "CF 5.0",
            })
        );
        assert_eq!(
            smirnov,
            Some(BegodeBanner::Firmware {
                prefix: BegodeFirmwarePrefix::SmirnoV,
                banner: "BF V5.3 CFW",
            })
        );
    }

    #[test]
    fn firmware_prefix_declares_whether_hardware_pwm_is_authoritative() {
        assert!(!BegodeFirmwarePrefix::Gotway.uses_hardware_pwm());
        assert!(!BegodeFirmwarePrefix::ExtremeBull.uses_hardware_pwm());
        assert!(BegodeFirmwarePrefix::Freestyl3r.uses_hardware_pwm());
        assert!(BegodeFirmwarePrefix::SmirnoV.uses_hardware_pwm());
    }

    #[test]
    fn parses_mpu_banner_as_temperature_formula_evidence() {
        assert_eq!(
            parse_begode_ascii_banner(b"MPU6050"),
            Some(BegodeBanner::Imu(BegodeImuKind::Mpu6050))
        );
        assert_eq!(
            parse_begode_ascii_banner(b"MPU6500"),
            Some(BegodeBanner::Imu(BegodeImuKind::Mpu6500))
        );
        assert_eq!(
            parse_begode_ascii_banner(b"MPU9250"),
            Some(BegodeBanner::Imu(BegodeImuKind::Unknown("9250")))
        );
    }

    #[test]
    fn binary_begode_frames_are_not_ascii_banners() {
        assert_eq!(parse_begode_ascii_banner(&[0x55, 0xaa, 0, 0]), None);
    }

    proptest! {
        #[test]
        fn any_non_printable_non_whitespace_byte_rejects_as_banner(
            prefix in proptest::collection::vec(0x20_u8..=0x7e, 0..8),
            bad in 0_u8..=0x1f,
            suffix in proptest::collection::vec(0x20_u8..=0x7e, 0..8),
        ) {
            prop_assume!(!matches!(bad, b'\n' | b'\r' | b'\t'));

            let mut bytes = prefix;
            bytes.push(bad);
            bytes.extend(suffix);

            prop_assert_eq!(parse_begode_ascii_banner(&bytes), None);
        }
    }
}
