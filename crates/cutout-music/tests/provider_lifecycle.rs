use cutout_core::MonotonicTimestamp;
use cutout_music::callback_epoch::{
    AuthorizationTransactionKind, AuthorizationTransactionMatch, CallbackEpochMatch,
};
use cutout_music::connection::MusicConnectionCallback;
use cutout_music::provider_lifecycle::{
    MusicProviderLifecycle, MusicProviderSuspension, MusicProviderWorkState,
    MusicTransportCompletion, MusicTransportOutcome,
};
use cutout_music::{
    MusicCapabilities, MusicCommand, MusicItem, MusicMonitorRequest, MusicMonitorResume,
    MusicMonitorStart, MusicObservationOutcome, MusicPlaybackPosition, MusicPlaybackState,
    MusicProvider, MusicRideEventKind, MusicSnapshot,
};

fn observation(
    item: Option<&str>,
    state: MusicPlaybackState,
    observed_at_ms: u64,
) -> MusicSnapshot {
    MusicSnapshot::new(
        MusicProvider::AppleMusic,
        "session",
        state,
        item.map(|identifier| MusicItem::new(identifier, None, None).expect("valid item")),
        MusicPlaybackPosition::default(),
        MonotonicTimestamp::new(observed_at_ms),
        MusicCapabilities::new(),
    )
    .expect("valid observation")
}

fn transition(outcome: MusicObservationOutcome) -> Option<MusicRideEventKind> {
    match outcome {
        MusicObservationOutcome::Accepted(decision) => decision.transition(),
        MusicObservationOutcome::OutOfOrder => panic!("observation unexpectedly out of order"),
    }
}

#[test]
fn accepted_skip_transport_correlates_the_next_item_in_the_same_rust_owner() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let provider = lifecycle
        .begin_provider_session()
        .expect("provider session");
    assert_eq!(
        transition(lifecycle.observe_music(observation(
            Some("first"),
            MusicPlaybackState::Playing,
            100,
        ))),
        Some(MusicRideEventKind::ItemChanged)
    );
    let transport = lifecycle
        .begin_transport_effect(provider, MusicCommand::Next, 200)
        .expect("transport");
    assert!(matches!(
        lifecycle.finish_transport(provider, transport.id, MusicTransportOutcome::Accepted, 200,),
        MusicTransportCompletion::Finished { .. }
    ));

    assert_eq!(
        transition(lifecycle.observe_music(observation(
            Some("second"),
            MusicPlaybackState::Playing,
            300,
        ))),
        Some(MusicRideEventKind::Skip)
    );
}

#[test]
fn failed_skip_transport_does_not_relabel_a_later_item_change() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let provider = lifecycle
        .begin_provider_session()
        .expect("provider session");
    let _ = lifecycle.observe_music(observation(Some("first"), MusicPlaybackState::Playing, 100));
    let transport = lifecycle
        .begin_transport_effect(provider, MusicCommand::Next, 200)
        .expect("transport");
    let _ = lifecycle.finish_transport(provider, transport.id, MusicTransportOutcome::Failed, 200);

    assert_eq!(
        transition(lifecycle.observe_music(observation(
            Some("second"),
            MusicPlaybackState::Playing,
            300,
        ))),
        Some(MusicRideEventKind::ItemChanged)
    );
}

#[test]
fn consecutive_disconnect_observations_emit_one_boundary() {
    let mut lifecycle = MusicProviderLifecycle::default();
    assert_eq!(
        transition(lifecycle.observe_music(observation(
            None,
            MusicPlaybackState::Disconnected,
            100,
        ))),
        Some(MusicRideEventKind::ProviderDisconnected)
    );
    assert_eq!(
        transition(lifecycle.observe_music(observation(
            None,
            MusicPlaybackState::Disconnected,
            200,
        ))),
        None
    );
}

#[test]
fn connection_end_invalidates_command_feedback() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let _provider = lifecycle
        .begin_provider_session()
        .expect("provider session");
    let feedback = lifecycle
        .begin_command_feedback()
        .expect("feedback identity");
    let attempt = lifecycle
        .begin_connection_attempt(0)
        .expect("connection attempt")
        .attempt_id;
    assert_eq!(
        lifecycle.connection_established(attempt, 0),
        MusicConnectionCallback::Accepted
    );

    let ended = lifecycle.connection_disconnected_effect(attempt, 1);

    assert_eq!(ended.callback, MusicConnectionCallback::Accepted);
    assert_eq!(
        lifecycle.classify_command_feedback(feedback),
        CallbackEpochMatch::Stale
    );
}

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
        let authorization = lifecycle
            .begin_authorization(AuthorizationTransactionKind::Authorizing)
            .expect("authorization");
        let generation = lifecycle
            .begin_provider_session()
            .expect("provider session");

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
    let old_generation = lifecycle
        .begin_provider_session()
        .expect("provider session");
    let old_connection = lifecycle
        .begin_connection_attempt(0)
        .expect("attempt")
        .attempt_id;

    let current_generation = lifecycle
        .begin_provider_session()
        .expect("provider session");
    assert_eq!(
        lifecycle.classify_provider_session(old_generation),
        CallbackEpochMatch::Stale,
    );
    assert_eq!(
        lifecycle.classify_provider_session(current_generation),
        CallbackEpochMatch::Current,
    );
    assert_eq!(
        lifecycle.connection_established(old_connection, 0),
        MusicConnectionCallback::Stale,
    );
}

#[test]
fn transport_has_one_terminal_outcome_across_deadline_and_late_callback() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let generation = lifecycle
        .begin_provider_session()
        .expect("provider session");
    let request = lifecycle
        .begin_transport_effect(generation, MusicCommand::Play, 1_000)
        .expect("request");

    assert_eq!(request.deadline.as_milliseconds(), 11_000);
    assert_eq!(
        lifecycle.begin_transport_effect(generation, MusicCommand::Play, 1_001),
        None
    );
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
        lifecycle.finish_transport(
            generation,
            request.id,
            MusicTransportOutcome::Accepted,
            11_000
        ),
        MusicTransportCompletion::Stale,
    );

    let replacement = lifecycle
        .begin_transport_effect(generation, MusicCommand::Play, 11_001)
        .expect("replacement request");
    assert_eq!(
        lifecycle.retire_provider_session(generation),
        MusicTransportCompletion::Finished {
            request_id: replacement.id,
            outcome: MusicTransportOutcome::Cancelled,
        },
    );
    assert_eq!(
        lifecycle.finish_transport(
            generation,
            replacement.id,
            MusicTransportOutcome::Failed,
            11_001,
        ),
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
    let provider = lifecycle
        .begin_provider_session()
        .expect("provider session");
    let first = lifecycle
        .begin_transport_effect(provider, MusicCommand::Play, 0)
        .expect("first transport");
    assert_eq!(
        lifecycle.cancel_transport(provider, first.id),
        MusicTransportCompletion::Finished {
            request_id: first.id,
            outcome: MusicTransportOutcome::Cancelled,
        },
    );

    let replacement = lifecycle
        .begin_transport_effect(provider, MusicCommand::Play, 1)
        .expect("replacement transport");
    assert_eq!(
        lifecycle.cancel_transport(provider, first.id),
        MusicTransportCompletion::Stale,
    );
    assert_eq!(
        lifecycle.finish_transport(provider, replacement.id, MusicTransportOutcome::Accepted, 1,),
        MusicTransportCompletion::Finished {
            request_id: replacement.id,
            outcome: MusicTransportOutcome::Accepted,
        },
    );
}

#[test]
fn connection_disconnect_cancels_only_connection_owned_transport() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let provider = lifecycle
        .begin_provider_session()
        .expect("provider session");
    let connection = lifecycle
        .begin_connection_attempt(0)
        .expect("attempt")
        .attempt_id;
    assert_eq!(
        lifecycle.begin_transport_effect_for_connection(
            provider,
            Some(connection),
            MusicCommand::Play,
            0,
        ),
        None,
        "transport cannot be admitted before the connection is established",
    );
    assert_eq!(
        lifecycle.connection_established(connection, 0),
        MusicConnectionCallback::Accepted
    );
    let request = lifecycle
        .begin_transport_effect_for_connection(provider, Some(connection), MusicCommand::Play, 0)
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
fn connection_end_releases_player_poll_and_owned_transport_for_reconnect() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let provider = lifecycle
        .begin_provider_session()
        .expect("provider session");
    let attempt = lifecycle
        .begin_connection_attempt(0)
        .expect("attempt")
        .attempt_id;
    assert_eq!(
        lifecycle.connection_established(attempt, 0),
        MusicConnectionCallback::Accepted
    );
    let poll = lifecycle
        .begin_player_state_request(0)
        .expect("player poll");
    let command = lifecycle
        .begin_transport_effect_for_connection(provider, Some(attempt), MusicCommand::Play, 0)
        .expect("transport");

    let failure = lifecycle.connection_failed_effect(attempt, 100);
    assert_eq!(failure.callback, MusicConnectionCallback::Accepted);
    assert_eq!(
        failure.transport,
        MusicTransportCompletion::Finished {
            request_id: command.id,
            outcome: MusicTransportOutcome::Cancelled,
        }
    );
    assert_eq!(
        lifecycle.complete_player_state_request(poll, 1_000),
        cutout_music::player_request::MusicPlayerRequestCompletion::Stale
    );
    assert!(lifecycle.begin_player_state_request(101).is_some());
    assert!(
        lifecycle
            .begin_transport_effect_for_connection(provider, None, MusicCommand::Play, 101)
            .is_some()
    );
}

#[test]
fn stale_connection_cannot_start_owned_transport() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let provider = lifecycle
        .begin_provider_session()
        .expect("provider session");
    let attempt = lifecycle
        .begin_connection_attempt(0)
        .expect("attempt")
        .attempt_id;
    assert_eq!(
        lifecycle.connection_failed_effect(attempt, 100).callback,
        MusicConnectionCallback::Accepted
    );

    assert_eq!(
        lifecycle.begin_transport_effect_for_connection(
            provider,
            Some(attempt),
            MusicCommand::Play,
            101,
        ),
        None
    );
}

#[test]
fn an_expired_connection_attempt_rejects_late_callbacks_and_retires_owned_transport() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let provider = lifecycle
        .begin_provider_session()
        .expect("provider session");
    let attempt = lifecycle
        .begin_connection_attempt(0)
        .expect("attempt")
        .attempt_id;

    assert_eq!(
        lifecycle.connection_established(attempt, 10_000),
        MusicConnectionCallback::Stale,
    );
    assert_eq!(
        lifecycle.connection_failed_effect(attempt, 10_000).callback,
        MusicConnectionCallback::Stale,
    );

    let replacement = lifecycle
        .begin_connection_attempt(10_000)
        .expect("replacement attempt")
        .attempt_id;
    assert_eq!(
        lifecycle.begin_transport_effect_for_connection(
            provider,
            Some(replacement),
            MusicCommand::Play,
            10_000,
        ),
        None,
        "a replacement attempt must remain unconnected",
    );
}

#[test]
fn replacing_a_timed_out_attempt_cancels_its_transport_before_the_new_attempt() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let provider = lifecycle
        .begin_provider_session()
        .expect("provider session");
    let first = lifecycle
        .begin_connection_attempt(0)
        .expect("first attempt")
        .attempt_id;
    assert_eq!(
        lifecycle.connection_established(first, 0),
        MusicConnectionCallback::Accepted
    );
    let request = lifecycle
        .begin_transport_effect_for_connection(provider, Some(first), MusicCommand::Play, 9_000)
        .expect("transport");

    let replacement = lifecycle
        .begin_connection_attempt(10_000)
        .expect("replacement attempt")
        .attempt_id;
    assert_ne!(first, replacement);
    assert_eq!(
        lifecycle.finish_transport(
            provider,
            request.id,
            MusicTransportOutcome::Accepted,
            10_000,
        ),
        MusicTransportCompletion::Stale,
        "the old connection cannot complete after replacement",
    );
}

#[test]
fn newer_push_revision_rejects_an_older_player_poll() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let request = lifecycle.begin_player_state_request(0).expect("poll");
    let revision = lifecycle.player_state_observation_revision();
    lifecycle.mark_player_state_observed(1);
    assert_eq!(
        lifecycle.complete_player_state_request_if_current(request, revision, 1_000),
        cutout_music::player_request::MusicPlayerRequestCompletion::Stale
    );
}

#[test]
fn provider_change_cancels_transport_and_rejects_its_late_completion() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let apple = lifecycle
        .begin_provider_session()
        .expect("provider session");
    let command = lifecycle
        .begin_transport_effect(apple, MusicCommand::Play, 0)
        .expect("apple command");

    let spotify = lifecycle
        .begin_provider_session()
        .expect("provider session");
    assert_ne!(apple, spotify);
    assert_eq!(
        lifecycle.finish_transport(apple, command.id, MusicTransportOutcome::Accepted, 0),
        MusicTransportCompletion::Stale,
    );
}

#[test]
fn spotify_connection_retries_do_not_reset_when_the_sdk_object_is_recreated() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let provider = lifecycle
        .begin_provider_session()
        .expect("provider session");

    for now_ms in [0, 2_100, 4_200] {
        assert_eq!(
            lifecycle.classify_provider_session(provider),
            CallbackEpochMatch::Current,
        );
        let attempt = lifecycle
            .begin_connection_attempt(now_ms)
            .expect("bounded attempt")
            .attempt_id;
        assert_eq!(
            lifecycle
                .connection_failed_effect(attempt, now_ms + 100)
                .callback,
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
            .deadline
            .as_milliseconds(),
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

    let provider = lifecycle
        .begin_provider_session()
        .expect("provider session");
    let authorization = lifecycle
        .begin_authorization_effect(AuthorizationTransactionKind::Authorizing, 1_000)
        .expect("authorization effect");
    assert_eq!(authorization.deadline.as_milliseconds(), u64::MAX);
    let renewal = lifecycle
        .begin_authorization_effect(AuthorizationTransactionKind::Renewing, 1_000)
        .expect("renewal effect");
    assert_eq!(renewal.deadline.as_milliseconds(), 21_000);
    let artwork = lifecycle
        .begin_artwork_effect(provider, 2_000)
        .expect("artwork");
    assert_eq!(artwork.deadline.as_milliseconds(), 7_000);
    assert_eq!(
        lifecycle.complete_artwork_request(provider, artwork.id),
        cutout_music::player_request::MusicPlayerRequestCompletion::Accepted,
    );
    let retry = lifecycle
        .begin_artwork_retry_effect(provider, 7_000)
        .expect("retry");
    assert_eq!(retry.deadline.as_milliseconds(), 8_000);
    assert_eq!(
        lifecycle.complete_artwork_retry(provider, retry.id),
        CallbackEpochMatch::Current,
    );
}

#[test]
fn command_feedback_identity_rejects_older_completion_and_dismissal() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let first = lifecycle.begin_command_feedback().expect("first feedback");
    let second = lifecycle.begin_command_feedback().expect("second feedback");

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
