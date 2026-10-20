use std::sync::Arc;

use cutout_application::music::{
    observation_admission::{
        Admission, AdmissionError, HistoryReceipt, ObservationQueue, RecordingIdentity,
    },
    provider_lifecycle::ProviderLifecycle,
};
use cutout_core::MonotonicTimestamp;
use cutout_music::{
    MusicCapabilities, MusicCommand, MusicItem, MusicMonitorRequest, MusicPlaybackPosition,
    MusicPlaybackState, MusicProvider, MusicSnapshot,
};

fn snapshot(state: MusicPlaybackState, at_ms: u64) -> MusicSnapshot {
    MusicSnapshot::new(
        MusicProvider::AppleMusic,
        "session",
        state,
        Some(MusicItem::new("track-1", Some("Song".into()), None).expect("valid item")),
        MusicPlaybackPosition::new(None, None).expect("valid empty position"),
        MonotonicTimestamp::new(at_ms),
        MusicCapabilities::new().with(MusicCommand::Pause),
    )
    .expect("valid provider observation")
}

#[test]
fn public_observation_api_retains_transition_until_persisted_receipt() {
    let provider = ProviderLifecycle::new();
    provider.request_monitor(MusicMonitorRequest::Observe);
    let baseline = provider
        .observe_for_ride(
            Some("ride-1".into()),
            snapshot(MusicPlaybackState::Paused, 0),
            1_700_000_000_000,
            2,
        )
        .expect("baseline classified")
        .expect("initial item observation accepted");
    let baseline_transition = baseline
        .history_transition()
        .expect("initial item transition")
        .id();
    assert_eq!(
        provider.acknowledge_history_transition(baseline_transition),
        cutout_music::MusicHistoryTransitionAcknowledgement::Acknowledged
    );
    let decision = provider
        .observe_for_ride(
            Some("ride-1".into()),
            snapshot(MusicPlaybackState::Playing, 1),
            1_700_000_000_001,
            2,
        )
        .expect("observation classified")
        .expect("new playback advances history");
    let transition = decision
        .history_transition()
        .cloned()
        .expect("play transition retained");

    let queue = Arc::new(ObservationQueue::default());
    let request = queue.admit(Some(17));
    let Admission::Admitted(lease) = request.admission() else {
        panic!("observation admitted");
    };
    lease
        .classify(
            Some(RecordingIdentity {
                ride_id: "ride-1".into(),
                generation: 4,
            }),
            || Ok::<_, AdmissionError>((decision, Some(transition.clone()))),
        )
        .expect("FIFO head owns classification");
    let (recording, claimed) = lease.claim_write().expect("exact transition claimed once");
    assert_eq!(recording.ride_id, "ride-1");
    assert_eq!(claimed, transition);
    lease.history_receipt(HistoryReceipt::Recorded);

    let stop = queue.begin_stop().expect("Stop covers admitted history");
    request
        .settle()
        .expect("durable receipt settles required work");
    assert_eq!(stop.wait_result(), Ok(()));
}

#[tokio::test]
async fn non_durable_history_outcome_fails_stop_and_requires_explicit_retry() {
    let provider = ProviderLifecycle::new();
    let baseline = provider
        .observe_for_ride(
            Some("ride-1".into()),
            snapshot(MusicPlaybackState::Paused, 0),
            1_700_000_000_000,
            2,
        )
        .expect("baseline classified")
        .expect("initial item observation accepted");
    let baseline_transition = baseline
        .history_transition()
        .expect("initial item transition")
        .id();
    assert_eq!(
        provider.acknowledge_history_transition(baseline_transition),
        cutout_music::MusicHistoryTransitionAcknowledgement::Acknowledged
    );
    let decision = provider
        .observe_for_ride(
            Some("ride-1".into()),
            snapshot(MusicPlaybackState::Playing, 1),
            1_700_000_000_001,
            2,
        )
        .expect("observation classified")
        .expect("play transition produced");
    let transition = decision
        .history_transition()
        .cloned()
        .expect("play transition retained");
    let queue = Arc::new(ObservationQueue::default());
    let request = queue.admit(Some(17));
    let Admission::Admitted(lease) = request.admission() else {
        panic!("observation admitted");
    };
    lease
        .classify(
            Some(RecordingIdentity {
                ride_id: "ride-1".into(),
                generation: 4,
            }),
            || Ok::<_, AdmissionError>(((), Some(transition.clone()))),
        )
        .expect("recordable observation classified");
    let (recording, claimed) = lease.claim_write().expect("history write claimed");
    assert_eq!(recording.ride_id, "ride-1");
    assert_eq!(claimed.id(), transition.id());
    let stop = queue.begin_stop().expect("Stop fence");
    // A rejected persistence outcome never acknowledges a history write.
    lease.history_receipt(HistoryReceipt::OutOfOrder);
    assert_eq!(request.settle(), Err(AdmissionError::Incomplete));
    let stop_result = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        tokio::task::spawn_blocking(move || stop.wait_result()),
    )
    .await
    .expect("Stop fence completes within the test bound")
    .expect("Stop fence waiter does not panic");
    assert_eq!(stop_result, Err(AdmissionError::Incomplete));
    request.release();
    assert_eq!(
        queue.begin_stop().expect("explicit retry").wait_result(),
        Ok(())
    );
}

#[test]
fn stop_fence_covers_only_its_admission_prefix() {
    let queue = Arc::new(ObservationQueue::default());
    let first = queue.admit(None);
    let stop = queue.begin_stop().expect("fence at current prefix");
    let later = queue.admit(None);
    let Admission::Admitted(first_lease) = first.admission() else {
        panic!("first observation admitted");
    };
    let Admission::Admitted(later_lease) = later.admission() else {
        panic!("later observation admitted");
    };
    first_lease
        .classify(None, || Ok::<_, AdmissionError>(((), None)))
        .expect("prefix classified");
    first.settle().expect("prefix settled");
    assert_eq!(stop.wait_result(), Ok(()));
    drop(stop);
    assert_eq!(
        later_lease.state(),
        cutout_application::music::observation_admission::LeaseState::Ready
    );
    later.release();
}

#[test]
fn a_foreign_queue_cannot_claim_or_publish_an_admitted_lease() {
    let owner = Arc::new(ObservationQueue::default());
    let foreign = Arc::new(ObservationQueue::default());
    let request = owner.admit(Some(8));
    let Admission::Admitted(lease) = request.admission() else {
        panic!("observation admitted");
    };
    assert!(owner.owns(&lease));
    assert!(!foreign.owns(&lease));
    assert_eq!(
        foreign.with_ready(&lease, || ()),
        Err(AdmissionError::Stale)
    );
}
