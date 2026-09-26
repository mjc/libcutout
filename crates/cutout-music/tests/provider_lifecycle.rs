#![cfg_attr(test, allow(clippy::disallowed_macros))]

use cutout_core::{MonotonicTimestamp, WallClockUnixTimestamp};
use cutout_music::callback_epoch::{
    AuthorizationTransactionKind, AuthorizationTransactionMatch, CallbackEpochMatch,
};
use cutout_music::connection::MusicConnectionCallback;
use cutout_music::ids::{EstablishedConnectionId, ProviderSessionId};
use cutout_music::provider_lifecycle::{
    MusicConnectionAttemptAdmission, MusicProviderLifecycle, MusicProviderSuspension,
    MusicProviderWorkState, MusicTransportCompletion, MusicTransportOutcome, MusicTransportOwner,
};
use cutout_music::{
    MusicCapabilities, MusicCommand, MusicItem, MusicMonitorRequest, MusicMonitorResume,
    MusicMonitorStart, MusicObservationOutcome, MusicObservationTiming, MusicPlaybackPosition,
    MusicPlaybackState, MusicProvider, MusicRideEventKind, MusicSnapshot,
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

fn observe_transition(
    lifecycle: &mut MusicProviderLifecycle,
    snapshot: MusicSnapshot,
) -> Option<MusicRideEventKind> {
    let timing = MusicObservationTiming::new(
        WallClockUnixTimestamp::new(snapshot.observed_at().as_milliseconds()),
        0,
    );
    match lifecycle.observe_music(snapshot, timing) {
        MusicObservationOutcome::Accepted(decision) => {
            let transition = decision.history_transition()?;
            let kind = transition.kind();
            let _ = lifecycle.acknowledge_history_transition(transition.id());
            Some(kind)
        }
        MusicObservationOutcome::OutOfOrder => panic!("observation unexpectedly out of order"),
    }
}

const fn provider_transport(provider: ProviderSessionId) -> MusicTransportOwner {
    MusicTransportOwner::Provider(provider)
}

const fn connection_transport(
    provider_generation: ProviderSessionId,
    connection_id: EstablishedConnectionId,
) -> MusicTransportOwner {
    MusicTransportOwner::Connection {
        provider_generation,
        connection_id,
    }
}

fn begin_connection(
    lifecycle: &mut MusicProviderLifecycle,
    now_ms: u64,
) -> cutout_music::provider_lifecycle::MusicConnectionAttemptEffect {
    let MusicConnectionAttemptAdmission::Started(effect) =
        lifecycle.begin_connection_attempt(now_ms)
    else {
        panic!("expected a new connection attempt");
    };
    effect
}

#[test]
fn accepted_skip_transport_correlates_the_next_item_in_the_same_rust_owner() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let provider = lifecycle
        .begin_provider_session()
        .expect("provider session");
    assert_eq!(
        observe_transition(
            &mut lifecycle,
            observation(Some("first"), MusicPlaybackState::Playing, 100,)
        ),
        Some(MusicRideEventKind::ItemChanged)
    );
    let transport = lifecycle
        .begin_transport_effect(provider_transport(provider), MusicCommand::Next, 200)
        .expect("transport");
    assert!(matches!(
        lifecycle.finish_transport(provider, transport.id, MusicTransportOutcome::Accepted),
        MusicTransportCompletion::Finished { .. }
    ));

    assert_eq!(
        observe_transition(
            &mut lifecycle,
            observation(Some("second"), MusicPlaybackState::Playing, 300,)
        ),
        Some(MusicRideEventKind::Skip)
    );
}

#[test]
fn failed_skip_transport_does_not_relabel_a_later_item_change() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let provider = lifecycle
        .begin_provider_session()
        .expect("provider session");
    let _ = observe_transition(
        &mut lifecycle,
        observation(Some("first"), MusicPlaybackState::Playing, 100),
    );
    let transport = lifecycle
        .begin_transport_effect(provider_transport(provider), MusicCommand::Next, 200)
        .expect("transport");
    let _ = lifecycle.finish_transport(provider, transport.id, MusicTransportOutcome::Failed);

    assert_eq!(
        observe_transition(
            &mut lifecycle,
            observation(Some("second"), MusicPlaybackState::Playing, 300,)
        ),
        Some(MusicRideEventKind::ItemChanged)
    );
}

#[test]
fn item_change_before_failed_skip_completion_is_not_recorded_as_a_skip() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let provider = lifecycle
        .begin_provider_session()
        .expect("provider session");
    let _ = observe_transition(
        &mut lifecycle,
        observation(Some("first"), MusicPlaybackState::Playing, 100),
    );
    let transport = lifecycle
        .begin_transport_effect(provider_transport(provider), MusicCommand::Next, 200)
        .expect("transport");
    let timing = MusicObservationTiming::new(WallClockUnixTimestamp::new(300), 0);
    let MusicObservationOutcome::Accepted(provisional) = lifecycle.observe_music(
        observation(Some("second"), MusicPlaybackState::Playing, 300),
        timing,
    ) else {
        panic!("item change must be accepted");
    };
    assert_eq!(provisional.history_transition(), None);

    let _ = lifecycle.finish_transport(provider, transport.id, MusicTransportOutcome::Failed);
    assert_eq!(
        observe_transition(
            &mut lifecycle,
            observation(Some("second"), MusicPlaybackState::Playing, 400),
        ),
        Some(MusicRideEventKind::ItemChanged)
    );
}

#[test]
fn consecutive_disconnect_observations_emit_one_boundary() {
    let mut lifecycle = MusicProviderLifecycle::default();
    assert_eq!(
        observe_transition(
            &mut lifecycle,
            observation(None, MusicPlaybackState::Disconnected, 100,)
        ),
        Some(MusicRideEventKind::ProviderDisconnected)
    );
    assert_eq!(
        observe_transition(
            &mut lifecycle,
            observation(None, MusicPlaybackState::Disconnected, 200,)
        ),
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
    let attempt = begin_connection(&mut lifecycle, 0).attempt_id;
    assert!(lifecycle.connection_established(attempt, 0).is_some());

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
fn authorization_callback_remains_current_on_either_side_of_scene_resume() {
    for callback_before_resume in [true, false] {
        let mut lifecycle = MusicProviderLifecycle::default();
        lifecycle.request_monitor(MusicMonitorRequest::Authorize);
        assert_eq!(
            lifecycle.begin_monitor().map(|effect| effect.start),
            Some(MusicMonitorStart::Authorize),
        );
        let authorization = lifecycle
            .begin_authorization(AuthorizationTransactionKind::Authorizing)
            .expect("authorization");
        let generation = lifecycle
            .begin_provider_session()
            .expect("provider session");

        let suspension = lifecycle.suspend();
        assert!(suspension.observation_gap);
        assert_eq!(
            lifecycle.classify_provider_session(generation),
            CallbackEpochMatch::Stale,
        );

        if callback_before_resume {
            assert_eq!(
                lifecycle.classify_authorization(authorization),
                AuthorizationTransactionMatch::Authorizing,
            );
            assert_eq!(
                lifecycle.finish_authorization(authorization),
                AuthorizationTransactionMatch::Authorizing,
            );
        }

        assert_eq!(lifecycle.resume(), MusicMonitorResume::Restored);
        assert_eq!(
            lifecycle.begin_monitor().map(|effect| effect.start),
            Some(MusicMonitorStart::Observe),
        );

        if !callback_before_resume {
            assert_eq!(
                lifecycle.classify_authorization(authorization),
                AuthorizationTransactionMatch::Authorizing,
            );
            assert_eq!(
                lifecycle.finish_authorization(authorization),
                AuthorizationTransactionMatch::Authorizing,
            );
        }

        assert_eq!(
            lifecycle.classify_authorization(authorization),
            AuthorizationTransactionMatch::Stale,
        );
    }
}

#[test]
fn replaced_provider_work_rejects_stale_generation_and_connection_callbacks() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let old_generation = lifecycle
        .begin_provider_session()
        .expect("provider session");
    let old_connection = begin_connection(&mut lifecycle, 0).attempt_id;

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
    assert_eq!(lifecycle.connection_established(old_connection, 0), None);
}

#[test]
fn transport_has_one_terminal_outcome_across_deadline_and_late_callback() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let generation = lifecycle
        .begin_provider_session()
        .expect("provider session");
    let request = lifecycle
        .begin_transport_effect(provider_transport(generation), MusicCommand::Play, 1_000)
        .expect("request");

    assert_eq!(request.deadline.as_milliseconds(), 11_000);
    assert_eq!(
        lifecycle.begin_transport_effect(provider_transport(generation), MusicCommand::Play, 1_001),
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
        lifecycle.finish_transport(generation, request.id, MusicTransportOutcome::Accepted),
        MusicTransportCompletion::Stale,
    );

    let replacement = lifecycle
        .begin_transport_effect(provider_transport(generation), MusicCommand::Play, 11_001)
        .expect("replacement request");
    assert_eq!(
        lifecycle.retire_provider_session(generation),
        MusicTransportCompletion::Finished {
            request_id: replacement.id,
            outcome: MusicTransportOutcome::Cancelled,
        },
    );
    assert_eq!(
        lifecycle.finish_transport(generation, replacement.id, MusicTransportOutcome::Failed,),
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
        .begin_transport_effect(provider_transport(provider), MusicCommand::Play, 0)
        .expect("first transport");
    assert_eq!(
        lifecycle.cancel_transport(provider, first.id),
        MusicTransportCompletion::Finished {
            request_id: first.id,
            outcome: MusicTransportOutcome::Cancelled,
        },
    );

    let replacement = lifecycle
        .begin_transport_effect(provider_transport(provider), MusicCommand::Play, 1)
        .expect("replacement transport");
    assert_eq!(
        lifecycle.cancel_transport(provider, first.id),
        MusicTransportCompletion::Stale,
    );
    assert_eq!(
        lifecycle.finish_transport(provider, replacement.id, MusicTransportOutcome::Accepted),
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
    let attempt = begin_connection(&mut lifecycle, 0).attempt_id;
    let unproven = EstablishedConnectionId::from_raw(attempt.raw());
    assert_eq!(
        lifecycle.begin_transport_effect(
            connection_transport(provider, unproven),
            MusicCommand::Play,
            0,
        ),
        None,
        "transport cannot be admitted before the connection is established",
    );
    let connection = lifecycle
        .connection_established(attempt, 0)
        .expect("established connection");
    let request = lifecycle
        .begin_transport_effect(
            connection_transport(provider, connection),
            MusicCommand::Play,
            0,
        )
        .expect("transport");
    let disconnected = lifecycle.connection_disconnected_effect(attempt, 1);
    assert_eq!(
        disconnected.transport,
        MusicTransportCompletion::Finished {
            request_id: request.id,
            outcome: MusicTransportOutcome::Cancelled,
        }
    );
    assert_eq!(
        lifecycle
            .connection_disconnected_effect(attempt, 2)
            .callback,
        MusicConnectionCallback::Stale,
    );
}

#[test]
fn connection_end_releases_player_poll_and_owned_transport_for_reconnect() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let provider = lifecycle
        .begin_provider_session()
        .expect("provider session");
    let attempt = begin_connection(&mut lifecycle, 0).attempt_id;
    let connection = lifecycle
        .connection_established(attempt, 0)
        .expect("established connection");
    let poll = lifecycle
        .begin_player_state_request(0)
        .expect("player poll");
    let command = lifecycle
        .begin_transport_effect(
            connection_transport(provider, connection),
            MusicCommand::Play,
            0,
        )
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
            .begin_transport_effect(provider_transport(provider), MusicCommand::Play, 101)
            .is_some()
    );
}

#[test]
fn stale_connection_cannot_start_owned_transport() {
    let mut lifecycle = MusicProviderLifecycle::default();
    let provider = lifecycle
        .begin_provider_session()
        .expect("provider session");
    let attempt = begin_connection(&mut lifecycle, 0).attempt_id;
    assert_eq!(
        lifecycle.connection_failed_effect(attempt, 100).callback,
        MusicConnectionCallback::Accepted
    );

    assert_eq!(
        lifecycle.begin_transport_effect(
            connection_transport(provider, EstablishedConnectionId::from_raw(attempt.raw())),
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
    let attempt = begin_connection(&mut lifecycle, 0).attempt_id;

    assert_eq!(lifecycle.connection_established(attempt, 10_000), None);
    assert_eq!(
        lifecycle.connection_failed_effect(attempt, 10_000).callback,
        MusicConnectionCallback::Stale,
    );

    let replacement = begin_connection(&mut lifecycle, 10_000).attempt_id;
    assert_eq!(
        lifecycle.begin_transport_effect(
            connection_transport(
                provider,
                EstablishedConnectionId::from_raw(replacement.raw()),
            ),
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
    let first = begin_connection(&mut lifecycle, 0).attempt_id;
    let connection = lifecycle
        .connection_established(first, 0)
        .expect("established connection");
    let request = lifecycle
        .begin_transport_effect(
            connection_transport(provider, connection),
            MusicCommand::Play,
            9_000,
        )
        .expect("transport");

    let replacement = begin_connection(&mut lifecycle, 10_000).attempt_id;
    assert_ne!(first, replacement);
    assert_eq!(
        lifecycle.finish_transport(provider, request.id, MusicTransportOutcome::Accepted,),
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
        .begin_transport_effect(provider_transport(apple), MusicCommand::Play, 0)
        .expect("apple command");

    let spotify = lifecycle
        .begin_provider_session()
        .expect("provider session");
    assert_ne!(apple, spotify);
    assert_eq!(
        lifecycle.finish_transport(apple, command.id, MusicTransportOutcome::Accepted),
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
        let attempt = begin_connection(&mut lifecycle, now_ms).attempt_id;
        assert_eq!(
            lifecycle
                .connection_failed_effect(attempt, now_ms + 100)
                .callback,
            MusicConnectionCallback::Accepted,
        );
    }

    assert_eq!(
        lifecycle.begin_connection_attempt(100_000),
        MusicConnectionAttemptAdmission::Exhausted
    );
}

#[test]
fn lost_connection_callbacks_exhaust_recovery_and_stop_monitor_polling() {
    let mut lifecycle = MusicProviderLifecycle::default();
    lifecycle.request_monitor(MusicMonitorRequest::Observe);
    let monitor = lifecycle.begin_monitor().expect("monitor");
    let _provider = lifecycle
        .begin_provider_session()
        .expect("provider session");

    for now_ms in [0, 10_000, 20_000] {
        assert!(matches!(
            lifecycle.begin_connection_attempt(now_ms),
            MusicConnectionAttemptAdmission::Started(_)
        ));
    }
    assert_eq!(
        lifecycle.begin_connection_attempt(30_000),
        MusicConnectionAttemptAdmission::Exhausted
    );
    assert_eq!(
        lifecycle.next_monitor_poll(
            monitor.generation,
            MusicProviderWorkState::CredentialsAvailable,
            30_000,
        ),
        None
    );
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
