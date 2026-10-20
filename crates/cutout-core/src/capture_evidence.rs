//! User-selected notes and screenshots, separate from raw Bluetooth evidence.

use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Recording notes are bounded by UTF-8 bytes, not characters.
pub const MAX_CAPTURE_NOTE_BYTES: usize = 1_024;
/// Maximum explicitly selected screenshots per recording.
pub const MAX_CAPTURE_SCREENSHOTS: usize = 4;
/// Maximum encoded JPEG size for one screenshot.
pub const MAX_CAPTURE_SCREENSHOT_BYTES: usize = 5 * 1_024 * 1_024;
/// Bound for the JSON attachment section, including base64 and escaped notes.
pub const MAX_CAPTURE_EVIDENCE_BYTES: usize = 28 * 1_024 * 1_024;

/// One screenshot re-encoded by the native image picker as JPEG.
#[derive(Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureScreenshot {
    /// JPEG bytes; JSON uses base64 rather than an array of integers.
    #[serde(with = "jpeg_base64")]
    pub jpeg: Vec<u8>,
}

impl std::fmt::Debug for CaptureScreenshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaptureScreenshot")
            .field("bytes", &self.jpeg.len())
            .finish()
    }
}

/// Optional, durable user-authored context for one recording.
#[derive(Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureAttachments {
    /// User-entered note, retained exactly without trimming or truncation.
    pub note: String,
    /// Explicitly selected screenshots in display order.
    #[serde(deserialize_with = "bounded_screenshots")]
    pub screenshots: Vec<CaptureScreenshot>,
}

fn bounded_screenshots<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<CaptureScreenshot>, D::Error> {
    struct Screenshots;
    impl<'de> serde::de::Visitor<'de> for Screenshots {
        type Value = Vec<CaptureScreenshot>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("up to four screenshots")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            let mut values = Vec::new();
            while let Some(value) = sequence.next_element()? {
                if values.len() == MAX_CAPTURE_SCREENSHOTS {
                    return Err(serde::de::Error::custom("too many screenshots"));
                }
                values.push(value);
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Screenshots)
}

impl std::fmt::Debug for CaptureAttachments {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaptureAttachments")
            .field("note_bytes", &self.note.len())
            .field("screenshots", &self.screenshots.len())
            .finish()
    }
}

/// Public limits without including any note or image content.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CaptureEvidenceError {
    /// The UTF-8 note exceeds its byte budget.
    #[error("The note must be 1,024 bytes or less.")]
    NoteTooLong,
    /// Too many images were selected.
    #[error("Choose up to four screenshots.")]
    TooManyScreenshots,
    /// One encoded image exceeds its budget.
    #[error("Each screenshot must be 5 MiB or less.")]
    ScreenshotTooLarge,
    /// The native picker did not provide a supported JPEG container.
    #[error("The screenshot could not be read as a JPEG image.")]
    InvalidScreenshot,
}

impl CaptureAttachments {
    /// Checks note and screenshot limits at the portable admission boundary.
    /// # Errors
    /// Returns the violated limit without exposing private content.
    pub fn validate(&self) -> Result<(), CaptureEvidenceError> {
        if self.note.len() > MAX_CAPTURE_NOTE_BYTES {
            return Err(CaptureEvidenceError::NoteTooLong);
        }
        if self.screenshots.len() > MAX_CAPTURE_SCREENSHOTS {
            return Err(CaptureEvidenceError::TooManyScreenshots);
        }
        for screenshot in &self.screenshots {
            if screenshot.jpeg.len() > MAX_CAPTURE_SCREENSHOT_BYTES {
                return Err(CaptureEvidenceError::ScreenshotTooLarge);
            }
            if !screenshot.jpeg.starts_with(&[0xff, 0xd8, 0xff])
                || !screenshot.jpeg.ends_with(&[0xff, 0xd9])
            {
                return Err(CaptureEvidenceError::InvalidScreenshot);
            }
        }
        Ok(())
    }

    /// Whether the user supplied any additional context.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.note.is_empty() && self.screenshots.is_empty()
    }
}

/// Describes the raw PEVCAP prefix and the attached context in one immutable upload.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureEvidenceManifest {
    /// SHA-256 of the unchanged raw PEVCAP bytes.
    pub capture_sha256: String,
    /// Length of the PEVCAP prefix; attachment JSON follows immediately.
    pub capture_bytes: u64,
    /// Exact UTF-8 note length used in the Send disclosure.
    pub note_bytes: u32,
    /// Number of selected screenshots used in the Send disclosure.
    pub screenshots: u32,
}

impl CaptureEvidenceManifest {
    /// Creates the manifest from validated local evidence.
    /// # Errors
    /// Returns the violated note or screenshot limit.
    pub fn new(
        capture_sha256: String,
        capture_bytes: u64,
        evidence: &CaptureAttachments,
    ) -> Result<Self, CaptureEvidenceError> {
        evidence.validate()?;
        Ok(Self {
            capture_sha256,
            capture_bytes,
            note_bytes: u32::try_from(evidence.note.len())
                .map_err(|_| CaptureEvidenceError::NoteTooLong)?,
            screenshots: u32::try_from(evidence.screenshots.len())
                .map_err(|_| CaptureEvidenceError::TooManyScreenshots)?,
        })
    }
}

/// Digests a complete transfer, retaining raw evidence identity in its manifest.
#[must_use]
pub fn capture_evidence_digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

mod jpeg_base64 {
    use super::STANDARD;
    use base64::Engine as _;
    use serde::{Deserialize, Deserializer, Serializer};
    pub(super) fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<u8>, D::Error> {
        let encoded = String::deserialize(deserializer)?;
        if encoded.len() > super::MAX_CAPTURE_SCREENSHOT_BYTES.div_ceil(3) * 4 {
            return Err(serde::de::Error::custom("screenshot exceeds size limit"));
        }
        STANDARD
            .decode(encoded)
            .map_err(|_| serde::de::Error::custom("invalid screenshot encoding"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_digest_retains_standard_sha256_hex_identity() {
        assert_eq!(
            capture_evidence_digest(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            capture_evidence_digest(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn notes_use_utf8_bytes_and_screenshots_are_bounded() {
        let mut evidence = CaptureAttachments {
            note: "é".repeat(512),
            screenshots: vec![],
        };
        assert_eq!(evidence.validate(), Ok(()));
        evidence.note.push('a');
        assert_eq!(evidence.validate(), Err(CaptureEvidenceError::NoteTooLong));
        evidence.note.clear();
        evidence.screenshots = vec![
            CaptureScreenshot {
                jpeg: vec![0xff, 0xd8, 0xff, 0xd9]
            };
            4
        ];
        assert_eq!(evidence.validate(), Ok(()));
        let json = serde_json::to_vec(&evidence).unwrap();
        assert_eq!(
            serde_json::from_slice::<CaptureAttachments>(&json).unwrap(),
            evidence
        );
        evidence.screenshots.push(evidence.screenshots[0].clone());
        assert_eq!(
            evidence.validate(),
            Err(CaptureEvidenceError::TooManyScreenshots)
        );
        evidence.screenshots = vec![CaptureScreenshot {
            jpeg: b"not an image".to_vec(),
        }];
        assert_eq!(
            evidence.validate(),
            Err(CaptureEvidenceError::InvalidScreenshot)
        );
    }
}
