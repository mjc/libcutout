use arrayvec::ArrayVec;
use thiserror::Error;

use crate::{
    GattChannel, GattFingerprint, MonotonicMillis, ProtocolFamily, VerifiedValue, WriteMode,
};

/// PEVCAP file format magic bytes.
pub const PEVCAP_MAGIC: [u8; 8] = *b"PEVCAP\0\0";

/// Current major PEVCAP format version.
pub const PEVCAP_VERSION_MAJOR: u16 = 1;

/// Current minor PEVCAP format version.
pub const PEVCAP_VERSION_MINOR: u16 = 0;

/// Maximum captured advertisement service UUIDs stored in the PEVCAP header.
pub const PEVCAP_MAX_ADVERTISED_SERVICES: usize = 8;

/// Maximum GATT fingerprints stored in the PEVCAP header.
pub const PEVCAP_MAX_GATT_FINGERPRINTS: usize = 8;

/// Maximum annotations stored in the PEVCAP header.
pub const PEVCAP_MAX_ANNOTATIONS: usize = 8;

/// Current PEVCAP format version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PevcapFormatVersion {
    /// Major version number.
    pub major: u16,

    /// Minor version number.
    pub minor: u16,
}

impl PevcapFormatVersion {
    /// Returns the format version currently produced by Cutout.
    #[must_use]
    pub const fn current() -> Self {
        Self {
            major: PEVCAP_VERSION_MAJOR,
            minor: PEVCAP_VERSION_MINOR,
        }
    }
}

/// Direction of a captured transport record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PevcapDirection {
    /// Data flowed from the peripheral to the host.
    Inbound,

    /// Data flowed from the host to the peripheral.
    Outbound,
}

/// Resolved model and firmware metadata embedded in the capture header.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PevcapResolvedIdentity {
    /// Resolved protocol family, when known.
    pub protocol_family: Option<ProtocolFamily>,

    /// Resolved model name, when known.
    pub model: Option<VerifiedValue<String>>,

    /// Resolved firmware string, when known.
    pub firmware: Option<VerifiedValue<String>>,
}

/// PEVCAP capture header metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PevcapHeader {
    /// Wall-clock start time in Unix milliseconds.
    pub wall_clock_start_unix_ms: u64,

    /// Platform identifier recorded by the capture producer.
    pub platform_id: String,

    /// Maximum transport write length observed at capture time.
    pub write_limit: Option<u16>,

    /// Advertised service UUIDs observed during discovery.
    pub advertised_services: ArrayVec<GattChannel, PEVCAP_MAX_ADVERTISED_SERVICES>,

    /// GATT fingerprints observed during discovery.
    pub gatt_fingerprints: ArrayVec<GattFingerprint, PEVCAP_MAX_GATT_FINGERPRINTS>,

    /// Resolved device identity, when known.
    pub resolved_identity: Option<PevcapResolvedIdentity>,

    /// Version of the Cutout library that produced the capture.
    pub library_version: String,

    /// Registry hash used to resolve the capture.
    pub registry_hash: [u8; 32],

    /// Human annotations attached to the capture.
    pub annotations: ArrayVec<String, PEVCAP_MAX_ANNOTATIONS>,
}

impl PevcapHeader {
    /// Creates a header after validating the bounded metadata fields.
    ///
    /// # Errors
    ///
    /// Returns [`PevcapHeaderError::TooManyAdvertisedServices`] when the
    /// observed service list exceeds the format bound, or similarly for GATT
    /// fingerprints and annotations.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        wall_clock_start_unix_ms: u64,
        platform_id: impl Into<String>,
        write_limit: Option<u16>,
        advertised_services: &[GattChannel],
        gatt_fingerprints: &[GattFingerprint],
        resolved_identity: Option<PevcapResolvedIdentity>,
        library_version: impl Into<String>,
        registry_hash: [u8; 32],
        annotations: &[&str],
    ) -> Result<Self, PevcapHeaderError> {
        Ok(Self {
            wall_clock_start_unix_ms,
            platform_id: platform_id.into(),
            write_limit,
            advertised_services: collect_bounded(
                advertised_services,
                PEVCAP_MAX_ADVERTISED_SERVICES,
                PevcapHeaderField::AdvertisedServices,
            )?,
            gatt_fingerprints: collect_bounded(
                gatt_fingerprints,
                PEVCAP_MAX_GATT_FINGERPRINTS,
                PevcapHeaderField::GattFingerprints,
            )?,
            resolved_identity,
            library_version: library_version.into(),
            registry_hash,
            annotations: collect_bounded_strings(
                annotations,
                PEVCAP_MAX_ANNOTATIONS,
                PevcapHeaderField::Annotations,
            )?,
        })
    }
}

/// PEVCAP header validation error.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PevcapHeaderError {
    /// A bounded header list exceeded the allowed capacity.
    #[error("{field:?} has {len} items but the format only allows {max}")]
    TooManyItems {
        /// Header field that exceeded its bound.
        field: PevcapHeaderField,

        /// Number of items observed.
        len: usize,

        /// Maximum allowed items.
        max: usize,
    },
}

/// Bounded header field identifier for error reporting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PevcapHeaderField {
    /// Advertised services observed during discovery.
    AdvertisedServices,

    /// GATT fingerprints observed during discovery.
    GattFingerprints,

    /// Free-form capture annotations.
    Annotations,
}

fn collect_bounded<T: Clone, const N: usize>(
    items: &[T],
    max: usize,
    field: PevcapHeaderField,
) -> Result<ArrayVec<T, N>, PevcapHeaderError> {
    if items.len() > max {
        return Err(PevcapHeaderError::TooManyItems {
            field,
            len: items.len(),
            max,
        });
    }

    let mut collected = ArrayVec::new();
    for item in items {
        let pushed = collected.try_push(item.clone());
        debug_assert!(pushed.is_ok());
    }
    Ok(collected)
}

fn collect_bounded_strings<const N: usize>(
    items: &[&str],
    max: usize,
    field: PevcapHeaderField,
) -> Result<ArrayVec<String, N>, PevcapHeaderError> {
    if items.len() > max {
        return Err(PevcapHeaderError::TooManyItems {
            field,
            len: items.len(),
            max,
        });
    }

    let mut collected = ArrayVec::new();
    for item in items {
        let pushed = collected.try_push((*item).to_owned());
        debug_assert!(pushed.is_ok());
    }
    Ok(collected)
}

/// Owned capture record for PEVCAP files.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PevcapRecord {
    /// Relative monotonic timestamp in milliseconds.
    pub monotonic_ms: MonotonicMillis,

    /// Transport direction for this record.
    pub direction: PevcapDirection,

    /// Characteristic that produced or consumed the bytes.
    pub characteristic: GattChannel,

    /// Optional service UUID associated with the record.
    pub service: Option<GattChannel>,

    /// Write mode, when the record is an outbound write.
    pub write_mode: Option<WriteMode>,

    /// Exact bytes captured for the record.
    pub bytes: Vec<u8>,
}

impl PevcapRecord {
    /// Creates an outbound write record.
    #[must_use]
    pub fn outbound_write(
        monotonic_ms: MonotonicMillis,
        characteristic: GattChannel,
        write_mode: WriteMode,
        bytes: Vec<u8>,
    ) -> Self {
        Self {
            monotonic_ms,
            direction: PevcapDirection::Outbound,
            characteristic,
            service: None,
            write_mode: Some(write_mode),
            bytes,
        }
    }

    /// Creates an inbound notification record.
    #[must_use]
    pub fn inbound_notification(
        monotonic_ms: MonotonicMillis,
        characteristic: GattChannel,
        service: GattChannel,
        bytes: Vec<u8>,
    ) -> Self {
        Self {
            monotonic_ms,
            direction: PevcapDirection::Inbound,
            characteristic,
            service: Some(service),
            write_mode: None,
            bytes,
        }
    }
}

/// Versioned PEVCAP capture envelope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PevcapCapture {
    /// File format version.
    pub version: PevcapFormatVersion,

    /// Header metadata for the capture.
    pub header: PevcapHeader,

    /// Ordered transport records.
    pub records: Vec<PevcapRecord>,
}

impl PevcapCapture {
    /// Creates a capture envelope using the current format version.
    #[must_use]
    pub fn new(header: PevcapHeader, records: Vec<PevcapRecord>) -> Self {
        Self {
            version: PevcapFormatVersion::current(),
            header,
            records,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VerificationStatus;

    #[test]
    fn pevcap_current_version_and_magic_are_stable() {
        assert_eq!(PEVCAP_MAGIC, *b"PEVCAP\0\0");
        assert_eq!(
            PevcapFormatVersion::current(),
            PevcapFormatVersion { major: 1, minor: 0 }
        );
    }

    #[test]
    fn pevcap_header_preserves_metadata_and_bounded_evidence() {
        let service = GattChannel::from_bytes([0x11; 16]);
        let fingerprint = GattFingerprint {
            service,
            characteristic: GattChannel::from_bytes([0x22; 16]),
            roles: crate::GattRoles::empty()
                .with_read()
                .with_write()
                .with_notify(),
            verification: VerificationStatus::HardwareVerified,
        };
        let header = PevcapHeader::new(
            1_725_000_000_000,
            "darwin",
            Some(185),
            &[service],
            &[fingerprint],
            Some(PevcapResolvedIdentity {
                protocol_family: Some(ProtocolFamily::VeteranLeaperkimNosfet),
                model: Some(VerifiedValue {
                    value: "NOSFET Aero".to_owned(),
                    verification: VerificationStatus::HardwareVerified,
                }),
                firmware: Some(VerifiedValue {
                    value: "3.8.12".to_owned(),
                    verification: VerificationStatus::SourceAndHardwareVerified,
                }),
            }),
            "0.1.0",
            [0xAB; 32],
            &["capture", "demo"],
        )
        .expect("header should validate");

        assert_eq!(header.wall_clock_start_unix_ms, 1_725_000_000_000);
        assert_eq!(header.platform_id, "darwin");
        assert_eq!(header.write_limit, Some(185));
        assert_eq!(header.advertised_services.as_slice(), &[service]);
        assert_eq!(header.gatt_fingerprints.as_slice(), &[fingerprint]);
        assert_eq!(
            header
                .resolved_identity
                .as_ref()
                .map(|resolved| resolved.model.as_ref().map(|model| model.value.as_str())),
            Some(Some("NOSFET Aero"))
        );
        assert_eq!(
            header
                .resolved_identity
                .as_ref()
                .and_then(|resolved| resolved.model.as_ref().map(|model| model.verification)),
            Some(VerificationStatus::HardwareVerified)
        );
        assert_eq!(
            header
                .resolved_identity
                .as_ref()
                .and_then(|resolved| resolved
                    .firmware
                    .as_ref()
                    .map(|firmware| firmware.verification)),
            Some(VerificationStatus::SourceAndHardwareVerified)
        );
        assert_eq!(header.library_version, "0.1.0");
        assert_eq!(header.registry_hash, [0xAB; 32]);
        assert_eq!(
            header.annotations.as_slice(),
            &["capture".to_owned(), "demo".to_owned()]
        );
    }

    #[test]
    fn pevcap_header_rejects_oversized_annotations() {
        let annotations = vec!["note"; PEVCAP_MAX_ANNOTATIONS + 1];
        let error = PevcapHeader::new(
            0,
            "linux",
            None,
            &[],
            &[],
            None,
            "0.1.0",
            [0x00; 32],
            &annotations,
        )
        .expect_err("header should reject oversized annotations");

        assert_eq!(
            error,
            PevcapHeaderError::TooManyItems {
                field: PevcapHeaderField::Annotations,
                len: PEVCAP_MAX_ANNOTATIONS + 1,
                max: PEVCAP_MAX_ANNOTATIONS,
            }
        );
    }

    #[test]
    fn pevcap_records_preserve_direction_modes_and_bytes() {
        let characteristic = GattChannel::from_bytes([0x33; 16]);
        let service = GattChannel::from_bytes([0x44; 16]);
        let write = PevcapRecord::outbound_write(
            7,
            characteristic,
            WriteMode::WithoutResponse,
            vec![0x01, 0x23, 0xab],
        );
        let notification = PevcapRecord::inbound_notification(
            9,
            characteristic,
            service,
            vec![0xde, 0xad, 0xbe, 0xef],
        );

        assert_eq!(write.direction, PevcapDirection::Outbound);
        assert_eq!(write.characteristic, characteristic);
        assert_eq!(write.service, None);
        assert_eq!(write.write_mode, Some(WriteMode::WithoutResponse));
        assert_eq!(write.bytes, vec![0x01, 0x23, 0xab]);
        assert_eq!(notification.direction, PevcapDirection::Inbound);
        assert_eq!(notification.characteristic, characteristic);
        assert_eq!(notification.service, Some(service));
        assert_eq!(notification.write_mode, None);
        assert_eq!(notification.bytes, vec![0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn pevcap_capture_wraps_header_and_records() {
        let header = PevcapHeader::new(
            1,
            "darwin",
            Some(185),
            &[],
            &[],
            None,
            "0.1.0",
            [0x11; 32],
            &[],
        )
        .expect("header should validate");
        let records = vec![PevcapRecord::outbound_write(
            1,
            GattChannel::from_bytes([0x55; 16]),
            WriteMode::WithResponse,
            vec![0x10],
        )];

        let capture = PevcapCapture::new(header.clone(), records.clone());

        assert_eq!(capture.version, PevcapFormatVersion::current());
        assert_eq!(capture.header, header);
        assert_eq!(capture.records, records);
    }
}
