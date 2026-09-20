//! Bounded current-session raw settings evidence, separate from semantic readback.

use bytes::Bytes;
use cutout_core::MonotonicTimestamp;

use crate::{DeviceConnectionSnapshot, VeteranBmsPageEvidence, VeteranFrame};

/// Protocol-qualified identity of one retained read-only page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawSettingsPageKey {
    /// Veteran/LeaperKim/NOSFET page selector, including undecoded selectors.
    Veteran {
        /// Selector observed in the validated frame.
        selector: u8,
    },
}

/// Latest validated frame for one page in the current connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawSettingsPage {
    /// Protocol and page identity; clients do not parse the frame to identify it.
    pub key: RawSettingsPageKey,
    /// Original host receipt time of the notification completing the frame.
    pub observed_at: MonotonicTimestamp,
    /// Entire validated frame, including header, undecoded bytes and CRC trailer.
    pub bytes: Bytes,
}

/// Retained pages paired with the selected connection and exact model/profile identity.
///
/// Reading this snapshot neither refreshes observations nor confirms a command.
/// Pages are discarded when the selected session ends; this is not a disk cache.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawSettingsSnapshot {
    /// Current connection and selected model identity used to choose the control profile.
    pub session: DeviceConnectionSnapshot,
    /// The latest validated settings page (selector 8), if observed.
    pub pages: Vec<RawSettingsPage>,
}

/// Bounded retention for the current connection's settings page.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct RawSettingsPages {
    pages: Vec<RawSettingsPage>,
}

impl RawSettingsPages {
    pub(crate) fn pages(&self) -> &[RawSettingsPage] {
        &self.pages
    }

    pub(crate) fn observe_veteran(
        &mut self,
        frame: &VeteranFrame,
        observed_at: MonotonicTimestamp,
    ) {
        let Some(evidence) = VeteranBmsPageEvidence::from_frame(frame) else {
            return;
        };
        let selector = evidence.selector.get();
        let position = self
            .pages
            .binary_search_by_key(&selector, |page| match page.key {
                RawSettingsPageKey::Veteran { selector } => selector,
            });
        if let Ok(index) = position
            && self
                .pages
                .get(index)
                .is_some_and(|page| page.observed_at > observed_at)
        {
            return;
        }
        let page = RawSettingsPage {
            key: RawSettingsPageKey::Veteran { selector },
            observed_at,
            bytes: Bytes::copy_from_slice(frame.as_slice()),
        };
        match position {
            Ok(index) => {
                if let Some(existing) = self.pages.get_mut(index) {
                    *existing = page;
                }
            }
            Err(index) => self.pages.insert(index, page),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DeviceConnectionSession, DeviceDetectionEvent, ReadOnlyNotificationDecoder,
        VETERAN_DATA_CHANNEL, VeteranFrameParseResult, VeteranFrameReassembler,
        VeteranNotificationDecoder,
    };
    use cutout_core::{
        ConnectionAttemptToken, MonotonicMillisDto, ProtocolFamily, SessionInputDto,
        SettingCommandStatus, SettingCompletionStrategy, SettingId, SettingSubmissionOutcome,
        SettingTransportStatus,
    };

    // Independent hardware capture, never round-tripped through the command encoder.
    fn captured_frames() -> Vec<Vec<u8>> {
        let mut assembler = VeteranFrameReassembler::default();
        let mut frames = Vec::new();
        for line in include_str!("../fixtures/nosfet-aero/nf2557-2026-06-21-powered-on-long.hex")
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
        {
            for offset in (0..line.len()).step_by(2) {
                let byte = u8::from_str_radix(&line[offset..offset + 2], 16).unwrap();
                if let Ok(VeteranFrameParseResult::Complete(frame)) =
                    assembler.feed_byte_result(byte)
                {
                    frames.push(frame.as_slice().to_vec());
                }
            }
        }
        frames
    }

    fn feed(decoder: &mut VeteranNotificationDecoder, bytes: &[u8], at: u64) {
        decoder.handle_notification(
            ProtocolFamily::VeteranLeaperkimNosfet,
            VETERAN_DATA_CHANNEL,
            bytes,
            MonotonicTimestamp::new(at),
            &mut Vec::new(),
        );
    }

    #[test]
    fn raw_settings_retains_exact_captured_pages_and_original_times() {
        let mut decoder = VeteranNotificationDecoder::default();
        let mut expected = std::collections::BTreeMap::new();
        for (index, frame) in captured_frames().into_iter().enumerate() {
            let at = index as u64 + 1;
            // Fragmented notifications must cache only after the full validated frame.
            for chunk in frame.chunks(7) {
                feed(&mut decoder, chunk, at);
            }
            if frame.get(46) == Some(&8) {
                expected.insert(frame[46], (at, frame));
            }
        }
        assert!(expected.contains_key(&8));
        assert_eq!(decoder.raw_settings_pages().len(), expected.len());
        for page in decoder.raw_settings_pages() {
            let RawSettingsPageKey::Veteran { selector } = page.key;
            let (at, bytes) = &expected[&selector];
            assert_eq!(page.observed_at.get(), *at);
            assert_eq!(page.bytes.as_ref(), bytes);
        }
        let before = decoder.raw_settings_pages().to_vec();
        for frame in captured_frames() {
            feed(&mut decoder, &frame, 0);
        }
        assert_eq!(decoder.raw_settings_pages(), before);
        decoder.reset();
        assert!(decoder.raw_settings_pages().is_empty());
    }

    #[test]
    fn raw_settings_same_timestamp_keeps_last_frame_in_notification() {
        let frames = captured_frames();
        let first = frames
            .iter()
            .find(|frame| frame.get(46) == Some(&8))
            .unwrap();
        let last = frames
            .iter()
            .find(|frame| frame.get(46) == Some(&8) && *frame != first)
            .unwrap();
        let mut decoder = VeteranNotificationDecoder::default();
        feed(
            &mut decoder,
            &[first.as_slice(), last.as_slice()].concat(),
            55,
        );
        assert_eq!(decoder.raw_settings_pages().len(), 1);
        let retained = &decoder.raw_settings_pages()[0];
        assert_eq!(retained.bytes.as_ref(), last);
        assert_eq!(retained.observed_at.get(), 55);
    }

    #[test]
    fn raw_settings_bounds_unknown_selectors_and_rejects_bad_crc() {
        let mut decoder = VeteranNotificationDecoder::default();
        let template = captured_frames()
            .into_iter()
            .find(|frame| frame.get(46) == Some(&8))
            .unwrap();
        for at in [10, 20] {
            for selector in 0..=u8::MAX {
                // Mutation is only for the exhaustive bound/unknown-selector check.
                let mut frame = template.clone();
                frame[46] = selector;
                let crc_offset = frame.len() - 4;
                let crc = crc32fast::hash(&frame[..crc_offset]);
                frame[crc_offset..].copy_from_slice(&crc.to_be_bytes());
                feed(&mut decoder, &frame, at);
            }
        }
        assert_eq!(decoder.raw_settings_pages().len(), 1);
        assert_eq!(decoder.raw_settings_pages()[0].observed_at.get(), 20);
        assert!(decoder.raw_settings_pages()[0].bytes.len() <= crate::MAX_VETERAN_FRAME_LEN);
        let before = decoder.raw_settings_pages().to_vec();
        let mut bad_crc = template;
        bad_crc[50] ^= 1;
        feed(&mut decoder, &bad_crc, 30);
        assert_eq!(decoder.raw_settings_pages(), before);
    }

    fn connected(frame: &[u8]) -> (DeviceConnectionSession, ConnectionAttemptToken) {
        let mut owner = DeviceConnectionSession::default();
        owner.begin_attempt("NF2557".into(), MonotonicTimestamp::new(0));
        let token = owner.snapshot().connection.token.unwrap();
        owner.session_state_mut().connection.connected(&token);
        let _ =
            owner.observe_for_attempt(&token, DeviceDetectionEvent::Notification { bytes: frame });
        owner.resolve(&token, false, MonotonicTimestamp::new(1));
        assert_eq!(
            owner.snapshot().identity.unwrap().model,
            Some(&crate::NOSFET_AERO_REGISTRY_ENTRY)
        );
        owner
            .ingest(
                &token,
                &SessionInputDto::LinkUp {
                    monotonic_ms: MonotonicMillisDto { milliseconds: 1 },
                    max_write_len: None,
                },
            )
            .unwrap();
        (owner, token)
    }

    fn ingest(
        owner: &mut DeviceConnectionSession,
        token: &ConnectionAttemptToken,
        bytes: &[u8],
        at: u64,
    ) {
        owner
            .ingest(
                token,
                &SessionInputDto::Notification {
                    channel: VETERAN_DATA_CHANNEL.as_bytes(),
                    bytes: bytes.to_vec(),
                    monotonic_ms: MonotonicMillisDto { milliseconds: at },
                },
            )
            .unwrap();
    }

    #[test]
    fn raw_settings_snapshot_preserves_sparse_semantics_without_confirmation_and_resets() {
        let frames = captured_frames();
        let page8 = frames
            .iter()
            .find(|frame| frame.get(46) == Some(&8))
            .unwrap();
        let page2 = frames
            .iter()
            .find(|frame| frame.get(46) == Some(&2))
            .unwrap();
        let (mut owner, token) = connected(page8);
        ingest(&mut owner, &token, page8, 100);
        let initial = owner
            .settings_snapshot()
            .settings
            .into_iter()
            .find(|setting| setting.id == SettingId::PedalHardness)
            .unwrap();
        let requested = initial.current.unwrap().value;
        let request_id = owner.session_state_mut().settings.submission(
            SettingId::PedalHardness,
            requested,
            SettingSubmissionOutcome::Accepted,
            SettingCompletionStrategy::MatchingReadback,
            MonotonicTimestamp::new(101),
        );
        owner.session_state_mut().settings.transport(
            SettingId::PedalHardness,
            request_id,
            SettingTransportStatus::Submitted,
            MonotonicTimestamp::new(102),
        );
        let pending = owner.settings_snapshot();
        let snapshot = owner.raw_settings_snapshot();
        assert_eq!(snapshot.session.connection.token.as_ref(), Some(&token));
        assert_eq!(
            snapshot.session.identity.unwrap().model,
            Some(&crate::NOSFET_AERO_REGISTRY_ENTRY)
        );
        assert_eq!(owner.raw_settings_snapshot(), snapshot);
        assert_eq!(
            owner.settings_snapshot(),
            pending,
            "cache access is read-only"
        );

        ingest(&mut owner, &token, page8, 90);
        ingest(&mut owner, &token, page2, 110);
        let retained = owner
            .settings_snapshot()
            .settings
            .into_iter()
            .find(|setting| setting.id == SettingId::PedalHardness)
            .unwrap();
        assert_eq!(retained.current, initial.current);
        assert_eq!(
            retained.status,
            SettingCommandStatus::WaitingForConfirmation
        );
        let pages = owner.raw_settings_snapshot().pages;
        assert_eq!(pages.len(), 1);
        let retained8 = pages
            .iter()
            .find(|page| page.key == RawSettingsPageKey::Veteran { selector: 8 })
            .unwrap();
        assert_eq!(retained8.bytes.as_ref(), page8);
        assert_eq!(retained8.observed_at.get(), 100);

        owner.begin_attempt("other-device".into(), MonotonicTimestamp::new(200));
        assert!(owner.raw_settings_snapshot().pages.is_empty());
        assert!(
            owner
                .ingest(
                    &token,
                    &SessionInputDto::Tick {
                        monotonic_ms: MonotonicMillisDto { milliseconds: 201 }
                    }
                )
                .is_none()
        );
        let (mut owner, token) = connected(page8);
        ingest(&mut owner, &token, page8, 100);
        owner.link_down(&token);
        assert!(owner.raw_settings_snapshot().pages.is_empty());
        owner.disconnect();
        assert!(owner.raw_settings_snapshot().pages.is_empty());
    }
}
