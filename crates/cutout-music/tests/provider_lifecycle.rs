use cutout_music::callback_epoch::{
    AuthorizationTransactionKind, AuthorizationTransactionMatch, CallbackEpochMatch,
};
use cutout_music::connection::MusicConnectionCallback;
use cutout_music::provider_lifecycle::{
    MusicProviderLifecycle, MusicProviderSuspension, MusicTransportCompletion,
    MusicTransportOutcome,
};
use cutout_music::{MusicMonitorRequest, MusicMonitorResume, MusicMonitorStart, MusicProvider};

#[test]
fn apple_music_and_spotify_share_monitoring_and_observation_gap_policy() {
    let mut providers = [
        (MusicProvider::AppleMusic, MusicProviderLifecycle::default()),
        (MusicProvider::Spotify, MusicProviderLifecycle::default()),
    ];

    for (provider, lifecycle) in &mut providers {
        lifecycle.request_monitor(MusicMonitorRequest::Authorize);
        assert_eq!(
            lifecycle.take_monitor_start(),
            Some(MusicMonitorStart::Authorize),
            "{provider:?}",
        );
        let authorization =
            lifecycle.begin_authorization(AuthorizationTransactionKind::Authorizing);
        let generation = lifecycle.begin_provider_session();

        assert_eq!(
            lifecycle.suspend(),
            MusicProviderSuspension {
                observation_gap: true,
                cancelled_transport_request_id: None,
            },
            "{provider:?}",
        );
        assert_eq!(
            lifecycle.classify_provider_session(generation),
            CallbackEpochMatch::Stale,
            "{provider:?}",
        );
        assert_eq!(
            lifecycle.finish_authorization(authorization),
            AuthorizationTransactionMatch::Authorizing,
            "{provider:?}",
        );
        assert_eq!(
            lifecycle.resume(),
            MusicMonitorResume::Restored,
            "{provider:?}",
        );
        assert_eq!(
            lifecycle.take_monitor_start(),
            Some(MusicMonitorStart::Observe),
            "{provider:?}",
        );

        let _ = lifecycle.cancel_monitor();
        lifecycle.suspend();
        assert_eq!(
            lifecycle.resume(),
            MusicMonitorResume::NoRequest,
            "{provider:?}",
        );
    }
}

#[test]
fn replaced_provider_work_rejects_stale_generation_and_connection_callbacks() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let old_generation = lifecycle.begin_provider_session();
    let old_connection = lifecycle.begin_connection_attempt(0).expect("attempt");

    let current_generation = lifecycle.begin_provider_session();
    assert_eq!(
        lifecycle.classify_provider_session(old_generation),
        CallbackEpochMatch::Stale,
    );
    assert_eq!(
        lifecycle.classify_provider_session(current_generation),
        CallbackEpochMatch::Current,
    );
    assert_eq!(
        lifecycle.connection_established(old_connection),
        MusicConnectionCallback::Stale,
    );
}

#[test]
fn transport_has_one_terminal_outcome_across_deadline_and_late_callback() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let generation = lifecycle.begin_provider_session();
    let request = lifecycle.begin_transport(1_000).expect("request");

    assert_eq!(lifecycle.begin_transport(1_001), None);
    assert_eq!(
        lifecycle.expire_transport(10_999),
        MusicTransportCompletion::Pending,
    );
    assert_eq!(
        lifecycle.expire_transport(11_000),
        MusicTransportCompletion::Finished {
            request_id: request,
            outcome: MusicTransportOutcome::TimedOut,
        },
    );
    assert_eq!(
        lifecycle.finish_transport(request, MusicTransportOutcome::Accepted),
        MusicTransportCompletion::Stale,
    );

    let replacement = lifecycle
        .begin_transport(11_001)
        .expect("replacement request");
    assert_eq!(
        lifecycle.retire_provider_session(generation),
        MusicTransportCompletion::Finished {
            request_id: replacement,
            outcome: MusicTransportOutcome::Cancelled,
        },
    );
    assert_eq!(
        lifecycle.finish_transport(replacement, MusicTransportOutcome::Failed),
        MusicTransportCompletion::Stale,
    );
}
