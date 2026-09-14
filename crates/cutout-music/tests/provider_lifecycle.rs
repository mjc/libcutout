use cutout_music::callback_epoch::{
    AuthorizationTransactionKind, AuthorizationTransactionMatch, CallbackEpochMatch,
};
use cutout_music::connection::MusicConnectionCallback;
use cutout_music::provider_lifecycle::{
    MusicProviderLifecycle, MusicProviderSuspension, MusicProviderWorkState,
    MusicTransportCompletion, MusicTransportOutcome,
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
            lifecycle.begin_monitor().map(|effect| effect.start),
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
            lifecycle.begin_monitor().map(|effect| effect.start),
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
    let request = lifecycle
        .begin_transport_effect(generation, 1_000)
        .expect("request");

    assert_eq!(request.deadline_ms, 11_000);
    assert_eq!(lifecycle.begin_transport_effect(generation, 1_001), None);
    assert_eq!(
        lifecycle.expire_transport(generation, 10_999),
        MusicTransportCompletion::Pending,
    );
    assert_eq!(
        lifecycle.expire_transport(generation, 11_000),
        MusicTransportCompletion::Finished {
            request_id: request.id,
            outcome: MusicTransportOutcome::TimedOut,
        },
    );
    assert_eq!(
        lifecycle.finish_transport(generation, request.id, MusicTransportOutcome::Accepted),
        MusicTransportCompletion::Stale,
    );

    let replacement = lifecycle
        .begin_transport_effect(generation, 11_001)
        .expect("replacement request");
    assert_eq!(
        lifecycle.retire_provider_session(generation),
        MusicTransportCompletion::Finished {
            request_id: replacement.id,
            outcome: MusicTransportOutcome::Cancelled,
        },
    );
    assert_eq!(
        lifecycle.finish_transport(generation, replacement.id, MusicTransportOutcome::Failed),
        MusicTransportCompletion::Stale,
    );
}
#[test]
fn monitor_effect_ids_are_rust_owned_and_invalidated_by_scene_changes() {
    let mut lifecycle = MusicProviderLifecycle::default();
    lifecycle.request_monitor(MusicMonitorRequest::Observe);
    let first = lifecycle.begin_monitor().expect("first monitor");
    assert_eq!(first.start, MusicMonitorStart::Observe);
    assert_eq!(
        lifecycle.classify_monitor(first.generation),
        CallbackEpochMatch::Current,
    );

    let replacement = lifecycle.begin_monitor().expect("replacement monitor");
    assert_ne!(first.generation, replacement.generation);
    assert_eq!(
        lifecycle.classify_monitor(first.generation),
        CallbackEpochMatch::Stale,
    );
    assert_eq!(
        lifecycle.classify_monitor(replacement.generation),
        CallbackEpochMatch::Current,
    );

    lifecycle.suspend();
    assert_eq!(
        lifecycle.classify_monitor(replacement.generation),
        CallbackEpochMatch::Stale,
    );
}

#[test]
fn transport_cancellation_is_scoped_to_the_rust_request_and_provider_generation() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let provider = lifecycle.begin_provider_session();
    let first = lifecycle
        .begin_transport_effect(provider, 0)
        .expect("first transport");
    assert_eq!(
        lifecycle.cancel_transport(provider, first.id),
        MusicTransportCompletion::Finished {
            request_id: first.id,
            outcome: MusicTransportOutcome::Cancelled,
        },
    );

    let replacement = lifecycle
        .begin_transport_effect(provider, 1)
        .expect("replacement transport");
    assert_eq!(
        lifecycle.cancel_transport(provider, first.id),
        MusicTransportCompletion::Stale,
    );
    assert_eq!(
        lifecycle.finish_transport(provider, replacement.id, MusicTransportOutcome::Accepted,),
        MusicTransportCompletion::Finished {
            request_id: replacement.id,
            outcome: MusicTransportOutcome::Accepted,
        },
    );
}

#[test]
fn connection_disconnect_cancels_only_connection_owned_transport() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let provider = lifecycle.begin_provider_session();
    let connection = lifecycle.begin_connection_attempt(0).expect("attempt");
    let request = lifecycle
        .begin_transport_effect_for_connection(provider, Some(connection), 0)
        .expect("transport");
    assert_eq!(
        lifecycle.cancel_transport_for_connection(provider, connection),
        MusicTransportCompletion::Finished {
            request_id: request.id,
            outcome: MusicTransportOutcome::Cancelled,
        }
    );
    assert_eq!(
        lifecycle.cancel_transport_for_connection(provider, connection),
        MusicTransportCompletion::Stale
    );
}

#[test]
fn newer_push_revision_rejects_an_older_player_poll() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let request = lifecycle.begin_player_state_request(0).expect("poll");
    let revision = lifecycle.player_state_observation_revision();
    lifecycle.mark_player_state_observed(1);
    assert_eq!(
        lifecycle.complete_player_state_request_if_current(request, revision),
        cutout_music::player_request::MusicPlayerRequestCompletion::Stale
    );
}

#[test]
fn provider_change_cancels_transport_and_rejects_its_late_completion() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let apple = lifecycle.begin_provider_session();
    let command = lifecycle
        .begin_transport_effect(apple, 0)
        .expect("apple command");

    let spotify = lifecycle.begin_provider_session();
    assert_ne!(apple, spotify);
    assert_eq!(
        lifecycle.finish_transport(apple, command.id, MusicTransportOutcome::Accepted),
        MusicTransportCompletion::Stale,
    );
}

#[test]
fn spotify_connection_retries_do_not_reset_when_the_sdk_object_is_recreated() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let provider = lifecycle.begin_provider_session();

    for now_ms in [0, 2_100, 4_200] {
        assert_eq!(
            lifecycle.classify_provider_session(provider),
            CallbackEpochMatch::Current,
        );
        let attempt = lifecycle
            .begin_connection_attempt(now_ms)
            .expect("bounded attempt");
        assert_eq!(
            lifecycle.connection_failed(attempt, now_ms + 100),
            MusicConnectionCallback::Accepted,
        );
    }

    assert_eq!(lifecycle.begin_connection_attempt(100_000), None);
}

#[test]
fn rust_effects_own_deadlines_and_monitor_continuation() {
    let mut lifecycle = MusicProviderLifecycle::default();
    lifecycle.request_monitor(MusicMonitorRequest::Observe);
    let monitor = lifecycle.begin_monitor().expect("monitor");
    assert_eq!(
        lifecycle
            .next_monitor_poll(monitor.generation, MusicProviderWorkState::Active, 500)
            .expect("poll")
            .deadline_ms,
        1_500,
    );
    assert_eq!(
        lifecycle.next_monitor_poll(
            monitor.generation,
            MusicProviderWorkState::RequiresUserAction,
            500,
        ),
        None,
    );

    let provider = lifecycle.begin_provider_session();
    let authorization =
        lifecycle.begin_authorization_effect(AuthorizationTransactionKind::Authorizing, 1_000);
    assert_eq!(authorization.deadline_ms, u64::MAX);
    let renewal =
        lifecycle.begin_authorization_effect(AuthorizationTransactionKind::Renewing, 1_000);
    assert_eq!(renewal.deadline_ms, 21_000);
    let artwork = lifecycle
        .begin_artwork_effect(provider, 2_000)
        .expect("artwork");
    assert_eq!(artwork.deadline_ms, 7_000);
    assert_eq!(
        lifecycle.complete_artwork_request(provider, artwork.id),
        cutout_music::player_request::MusicPlayerRequestCompletion::Accepted,
    );
    let retry = lifecycle
        .begin_artwork_retry_effect(provider, 7_000)
        .expect("retry");
    assert_eq!(retry.deadline_ms, 8_000);
    assert_eq!(
        lifecycle.complete_artwork_retry(provider, retry.id),
        CallbackEpochMatch::Current,
    );
}

#[test]
fn command_feedback_identity_rejects_older_completion_and_dismissal() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let first = lifecycle.begin_command_feedback();
    let second = lifecycle.begin_command_feedback();

    assert_eq!(
        lifecycle.classify_command_feedback(first),
        CallbackEpochMatch::Stale,
    );
    assert_eq!(
        lifecycle.dismiss_command_feedback(first),
        CallbackEpochMatch::Stale,
    );
    assert_eq!(
        lifecycle.classify_command_feedback(second),
        CallbackEpochMatch::Current,
    );
    assert_eq!(
        lifecycle.dismiss_command_feedback(second),
        CallbackEpochMatch::Current,
    );
}
