#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]

//! Provider-neutral music playback, privacy, and ride-timeline contracts.
//!
//! Persistence and platform adapters consume this domain model. Shared clock
//! types come from `cutout-core`; this crate does not perform I/O.

pub mod callback_epoch;
pub mod connection;
mod history;
pub mod ids;
mod model;
mod monitor;
mod observation;
pub mod player_request;
pub mod provider_lifecycle;

pub use history::*;
pub use model::*;
pub use monitor::*;
pub use observation::{MusicObservationDecision, MusicObservationOutcome};

#[cfg(test)]
mod tests {
    use super::*;
    use cutout_core::{MonotonicTimestamp, WallClockUnixTimestamp};

    fn snapshot(item: Option<MusicItem>) -> MusicSnapshot {
        snapshot_at(item, 10)
    }

    fn snapshot_at(item: Option<MusicItem>, observed_at: u64) -> MusicSnapshot {
        MusicSnapshot::new(
            MusicProvider::AppleMusic,
            "session",
            MusicPlaybackState::Playing,
            item,
            MusicPlaybackPosition::new(Some(10), Some(100)).expect("valid position"),
            MonotonicTimestamp::new(observed_at),
            MusicCapabilities::new()
                .with(MusicCommand::Previous)
                .with(MusicCommand::Pause)
                .with(MusicCommand::Next)
                .with(MusicCommand::OpenProvider),
        )
        .expect("valid music fixture")
    }

    #[test]
    fn snapshot_rejects_position_after_duration() {
        let error = MusicPlaybackPosition::new(Some(101), Some(100))
            .expect_err("position must not exceed duration");
        assert_eq!(error, MusicValidationError::PositionAfterDuration);
    }

    #[test]
    fn display_text_is_trimmed_and_truncated_on_a_utf8_boundary() {
        let item = MusicItem::new(
            "track-1",
            Some(format!("  {}  ", "é".repeat(MAX_MUSIC_DISPLAY_TEXT_BYTES))),
            None,
        )
        .expect("display metadata does not reject an observation");
        let title = item.title().expect("non-blank title");

        assert_eq!(title.len(), MAX_MUSIC_DISPLAY_TEXT_BYTES);
        assert_eq!(title.chars().count(), MAX_MUSIC_DISPLAY_TEXT_BYTES / 2);
        assert!(!title.starts_with(char::is_whitespace));
        assert!(!title.ends_with(char::is_whitespace));
    }

    #[test]
    fn history_policy_redacts_display_metadata() {
        let item = MusicItem::new(
            "track-1",
            Some("Song".to_owned()),
            Some("Artist".to_owned()),
        )
        .expect("valid item");
        let opaque = MusicRideEvent::from_snapshot(
            &snapshot(Some(item)),
            MusicRideEventKind::Play,
            MonotonicTimestamp::new(10),
            WallClockUnixTimestamp::new(100),
            2,
            MusicHistoryPolicy::OpaqueItem,
        )
        .expect("opaque policy records an event");
        assert_eq!(
            opaque.item_identifier().map(MusicIdentifier::as_str),
            Some("track-1")
        );
        assert_eq!(opaque.title(), None);
        assert_eq!(opaque.artist(), None);
    }

    #[test]
    fn opaque_policy_drops_spotify_local_identifier_metadata() {
        let item = MusicItem::new(
            "spotify:local:Private+Artist:Private+Album:Private+Title:180",
            Some("Private title".to_owned()),
            Some("Private artist".to_owned()),
        )
        .expect("valid local item");
        let snapshot = MusicSnapshot::new(
            MusicProvider::Spotify,
            "session",
            MusicPlaybackState::Playing,
            Some(item),
            MusicPlaybackPosition::default(),
            MonotonicTimestamp::new(10),
            MusicCapabilities::new(),
        )
        .expect("valid music fixture");
        let opaque = MusicRideEvent::from_snapshot(
            &snapshot,
            MusicRideEventKind::Play,
            MonotonicTimestamp::new(10),
            WallClockUnixTimestamp::new(100),
            2,
            MusicHistoryPolicy::OpaqueItem,
        )
        .expect("opaque policy records an event");
        assert_eq!(opaque.item_identifier(), None);
        assert_eq!(opaque.title(), None);
        assert_eq!(opaque.artist(), None);
    }

    #[test]
    fn disabled_history_does_not_create_an_event() {
        assert!(
            MusicRideEvent::from_snapshot(
                &snapshot(None),
                MusicRideEventKind::Play,
                MonotonicTimestamp::new(10),
                WallClockUnixTimestamp::new(100),
                2,
                MusicHistoryPolicy::Disabled,
            )
            .is_none()
        );
    }

    #[test]
    fn timeline_rejects_old_and_coalesces_duplicate_events() {
        let event = MusicRideEvent::from_snapshot(
            &snapshot(None),
            MusicRideEventKind::Play,
            MonotonicTimestamp::new(10),
            WallClockUnixTimestamp::new(100),
            2,
            MusicHistoryPolicy::OpaqueItem,
        )
        .expect("valid event");
        let mut timeline = MusicTimeline::new();
        assert_eq!(
            timeline.append(event.clone()),
            MusicTimelineOutcome::Recorded
        );
        assert_eq!(timeline.append(event), MusicTimelineOutcome::Duplicate);
        let old_snapshot = snapshot_at(None, 9);
        let old = MusicRideEvent::from_snapshot(
            &old_snapshot,
            MusicRideEventKind::Pause,
            MonotonicTimestamp::new(9),
            WallClockUnixTimestamp::new(99),
            2,
            MusicHistoryPolicy::OpaqueItem,
        )
        .expect("valid event");
        assert_eq!(timeline.append(old), MusicTimelineOutcome::OutOfOrder);
        assert_eq!(timeline.events().len(), 1);
    }

    #[test]
    fn timeline_coalesces_same_transition_with_newer_timing() {
        let first = MusicRideEvent::from_snapshot(
            &snapshot(None),
            MusicRideEventKind::Play,
            MonotonicTimestamp::new(10),
            WallClockUnixTimestamp::new(100),
            2,
            MusicHistoryPolicy::OpaqueItem,
        )
        .expect("valid event");
        let second = MusicRideEvent::from_snapshot(
            &snapshot(None),
            MusicRideEventKind::Play,
            MonotonicTimestamp::new(20),
            WallClockUnixTimestamp::new(110),
            3,
            MusicHistoryPolicy::OpaqueItem,
        )
        .expect("valid event");
        let mut timeline = MusicTimeline::new();
        assert_eq!(timeline.append(first), MusicTimelineOutcome::Recorded);
        assert_eq!(timeline.append(second), MusicTimelineOutcome::Duplicate);
        assert_eq!(timeline.events().len(), 1);
    }

    #[test]
    fn timeline_retains_distinct_skip_occurrences_without_metadata() {
        let first = MusicRideEvent::from_snapshot(
            &snapshot(None),
            MusicRideEventKind::Skip,
            MonotonicTimestamp::new(10),
            WallClockUnixTimestamp::new(100),
            2,
            MusicHistoryPolicy::OpaqueItem,
        )
        .expect("valid event");
        let second = MusicRideEvent::from_snapshot(
            &snapshot(None),
            MusicRideEventKind::Skip,
            MonotonicTimestamp::new(20),
            WallClockUnixTimestamp::new(110),
            2,
            MusicHistoryPolicy::OpaqueItem,
        )
        .expect("valid event");
        let mut timeline = MusicTimeline::new();
        assert_eq!(timeline.append(first), MusicTimelineOutcome::Recorded);
        assert_eq!(timeline.append(second), MusicTimelineOutcome::Recorded);
        assert_eq!(timeline.events().len(), 2);
    }

    #[test]
    fn timeline_retains_distinct_item_changes_after_opaque_redaction() {
        let first = MusicRideEvent::from_snapshot(
            &snapshot(None),
            MusicRideEventKind::ItemChanged,
            MonotonicTimestamp::new(10),
            WallClockUnixTimestamp::new(100),
            2,
            MusicHistoryPolicy::OpaqueItem,
        )
        .expect("valid event");
        let second = MusicRideEvent::from_snapshot(
            &snapshot(None),
            MusicRideEventKind::ItemChanged,
            MonotonicTimestamp::new(20),
            WallClockUnixTimestamp::new(110),
            2,
            MusicHistoryPolicy::OpaqueItem,
        )
        .expect("valid event");
        let mut timeline = MusicTimeline::new();
        assert_eq!(timeline.append(first), MusicTimelineOutcome::Recorded);
        assert_eq!(timeline.append(second), MusicTimelineOutcome::Recorded);
        assert_eq!(timeline.events().len(), 2);
    }

    #[test]
    fn timeline_deduplicates_exact_occurrence_replays() {
        let event = MusicRideEvent::from_snapshot(
            &snapshot(None),
            MusicRideEventKind::ItemChanged,
            MonotonicTimestamp::new(10),
            WallClockUnixTimestamp::new(100),
            2,
            MusicHistoryPolicy::OpaqueItem,
        )
        .expect("valid event");
        let mut timeline = MusicTimeline::new();
        assert_eq!(
            timeline.append(event.clone()),
            MusicTimelineOutcome::Recorded
        );
        assert_eq!(timeline.append(event), MusicTimelineOutcome::Duplicate);
    }

    #[test]
    fn timeline_restore_preserves_duplicate_durable_rows() {
        let event = MusicRideEvent::from_snapshot(
            &snapshot(None),
            MusicRideEventKind::Play,
            MonotonicTimestamp::new(10),
            WallClockUnixTimestamp::new(100),
            2,
            MusicHistoryPolicy::OpaqueItem,
        )
        .expect("valid event");
        let restored = MusicTimeline::from_stored_events(vec![event.clone(), event])
            .expect("durable sequence is authoritative");
        assert_eq!(restored.events().len(), 2);
    }

    #[test]
    fn blank_display_text_is_treated_as_absent() {
        let item = MusicItem::new("track-1", Some("  ".to_owned()), Some(String::new()))
            .expect("blank optional display fields are absent");
        assert_eq!(item.title(), None);
        assert_eq!(item.artist(), None);
    }
}
