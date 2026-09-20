//! Read-only projection of retained raw pages; native clients never parse wire bytes.

use cutout_protocols::{RawSettingsPageKey, RawSettingsSnapshot};

use crate::{CutoutSessionStateHandle, MobileDeviceSessionSnapshotDto};

/// Protocol-qualified page identity assigned by Rust.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRawSettingsPageKeyDto {
    /// Veteran/LeaperKim/NOSFET selector, including undecoded pages.
    Veteran {
        /// Selector observed in the validated frame.
        selector: u8,
    },
}

/// Last complete validated frame for one page.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRawSettingsPageDto {
    /// Rust-decoded protocol and page identity.
    pub key: MobileRawSettingsPageKeyDto,
    /// Original monotonic receipt time, never refreshed by reading the cache.
    pub observed_at_ms: u64,
    /// Exact frame, including undecoded bytes and CRC trailer.
    pub bytes: Vec<u8>,
}

/// Bounded current-session pages and the connection/model that owns them.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRawSettingsSnapshotDto {
    /// Selected connection and exact model identity used to choose its control profile.
    pub session: MobileDeviceSessionSnapshotDto,
    /// Latest pages in selector order. Empty after disconnect or a new attempt.
    pub pages: Vec<MobileRawSettingsPageDto>,
}

impl From<RawSettingsSnapshot> for MobileRawSettingsSnapshotDto {
    fn from(value: RawSettingsSnapshot) -> Self {
        Self {
            session: value.session.into(),
            pages: value
                .pages
                .into_iter()
                .map(|page| MobileRawSettingsPageDto {
                    key: match page.key {
                        RawSettingsPageKey::Veteran { selector } => {
                            MobileRawSettingsPageKeyDto::Veteran { selector }
                        }
                    },
                    observed_at_ms: page.observed_at.get(),
                    bytes: page.bytes.to_vec(),
                })
                .collect(),
        }
    }
}

#[uniffi::export]
impl CutoutSessionStateHandle {
    /// Reads retained evidence without sending requests or confirming settings.
    pub fn raw_settings_snapshot(&self) -> MobileRawSettingsSnapshotDto {
        self.lock_inner().raw_settings_snapshot().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MobileMonotonicMillisDto, MobileSessionInputDto, MobileSessionInputKindDto};
    use cutout_protocols::{
        VETERAN_DATA_CHANNEL, VeteranFrameParseResult, VeteranFrameReassembler,
    };

    #[test]
    fn raw_settings_ffi_reads_captured_bytes_identity_time_and_session_reset() {
        let mut assembler = VeteranFrameReassembler::default();
        let mut captured = None;
        'capture: for line in include_str!(
            "../../cutout-protocols/fixtures/nosfet-aero/nf2557-2026-06-21-powered-on-long.hex"
        )
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        {
            for offset in (0..line.len()).step_by(2) {
                let byte = u8::from_str_radix(&line[offset..offset + 2], 16).unwrap();
                if let Ok(VeteranFrameParseResult::Complete(frame)) =
                    assembler.feed_byte_result(byte)
                    && frame.as_slice().get(46) == Some(&8)
                {
                    captured = Some(frame.as_slice().to_vec());
                    break 'capture;
                }
            }
        }
        let frame = captured.expect("independent captured page 8");
        let handle = CutoutSessionStateHandle::new();
        assert!(handle.raw_settings_snapshot().pages.is_empty());
        let token = handle
            .begin_connection_attempt("NF2557".into(), 0)
            .token
            .unwrap();
        handle.connection_link_established(token.clone());
        handle.observe_connection_notification(token.clone(), frame.clone());
        let selected = handle.resolve_device_session(token.clone(), false, 1);
        assert_eq!(
            selected.identity.as_ref().unwrap().model.as_deref(),
            Some("NOSFET Aero")
        );
        handle
            .ingest_device_session(
                token.clone(),
                MobileSessionInputDto {
                    kind: MobileSessionInputKindDto::LinkUp,
                    monotonic_ms: MobileMonotonicMillisDto { milliseconds: 1 },
                    max_write_len: None,
                    channel: Vec::new(),
                    bytes: Vec::new(),
                },
            )
            .unwrap();
        handle
            .ingest_device_session(
                token.clone(),
                MobileSessionInputDto {
                    kind: MobileSessionInputKindDto::Notification,
                    channel: VETERAN_DATA_CHANNEL.as_bytes().to_vec(),
                    bytes: frame.clone(),
                    monotonic_ms: MobileMonotonicMillisDto { milliseconds: 42 },
                    max_write_len: None,
                },
            )
            .unwrap();
        let settings = handle.settings_snapshot();
        let snapshot = handle.raw_settings_snapshot();
        assert_eq!(snapshot.session, selected);
        assert_eq!(
            snapshot.pages,
            vec![MobileRawSettingsPageDto {
                key: MobileRawSettingsPageKeyDto::Veteran { selector: 8 },
                observed_at_ms: 42,
                bytes: frame,
            }]
        );
        assert_eq!(handle.raw_settings_snapshot(), snapshot);
        assert_eq!(handle.settings_snapshot(), settings);
        handle.connection_link_down(token);
        assert!(handle.raw_settings_snapshot().pages.is_empty());
        let replacement = handle.begin_connection_attempt("NF2557".into(), 100);
        assert_eq!(
            handle.raw_settings_snapshot().session.connection,
            replacement
        );
        assert!(handle.raw_settings_snapshot().pages.is_empty());
    }
}
