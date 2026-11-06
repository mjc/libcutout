#![cfg_attr(test, allow(clippy::disallowed_macros))]

use cutout_application::ride_activity::{
    ActivityIdentity, ActivityObservation, ActivityWork, ActivityWorkAdmission, ActivityWorkQueue,
    LifecycleAdmission, LifecycleRequest, ProjectionState, RidePhase, TerminalEffect,
};

// Opaque identities need equality and routing, not a Debug or Default implementation.
#[derive(Clone, Eq, PartialEq)]
struct Identity(&'static str);
impl ActivityIdentity for Identity {
    fn platform_identifier(&self) -> &str {
        self.0
    }
}

fn active() -> ActivityObservation<Identity> {
    ActivityObservation {
        identity: Some(Identity("wheel-a")),
        phase: RidePhase::Active,
        projection: ProjectionState::Active,
        last_telemetry_at_ms: Some(100),
    }
}

#[test]
fn terminal_identity_survives_coalesced_presentations_and_stale_completions() {
    let queue = ActivityWorkQueue::<Identity, &'static str>::default();
    assert!(queue.accept_request(1));
    assert!(matches!(
        queue.enqueue_presentation(1),
        ActivityWorkAdmission::Run { .. }
    ));
    assert!(queue.accept_request(2));
    assert!(matches!(
        queue.enqueue_terminal(
            2,
            TerminalEffect {
                identity: Identity("wheel-a"),
                reason: "user-stop"
            },
            2_000
        ),
        ActivityWorkAdmission::Queued { .. }
    ));
    for id in 3..100 {
        assert!(queue.accept_request(id));
        assert!(matches!(
            queue.enqueue_presentation(id),
            ActivityWorkAdmission::Queued { .. }
        ));
    }
    assert_eq!(queue.snapshot().pending_terminal_request_id, Some(2));
    assert_eq!(queue.snapshot().pending_presentation_request_id, Some(99));
    assert!(queue.finish_work(0).is_none());
    let Some(ActivityWork::Terminal {
        request_id, effect, ..
    }) = queue.finish_work(1)
    else {
        panic!("terminal obligation must run first")
    };
    assert_eq!(request_id, 2);
    assert!(effect.identity == Identity("wheel-a"));
    assert_eq!(effect.reason, "user-stop");
    assert!(matches!(
        queue.finish_work(2),
        Some(ActivityWork::Presentation { request_id: 99 })
    ));
    assert!(queue.finish_work(2).is_none());
    assert!(queue.finish_work(99).is_none());
    assert_eq!(queue.snapshot().in_flight_request_id, None);
}

#[test]
fn transport_fence_rejects_old_telemetry_and_background_still_owes_flush() {
    let queue = ActivityWorkQueue::<Identity, ()>::default();
    assert!(queue.accept_presentation_request(1, "wheel-a".into(), active(), 100));
    assert_eq!(
        queue.accept_lifecycle_request(
            2,
            LifecycleRequest::TransportDisconnected { at_ms: 101 },
            active()
        ),
        LifecycleAdmission::Reduce
    );
    assert!(!queue.accept_presentation_request(3, "wheel-a".into(), active(), 100));
    assert_eq!(
        queue.accept_lifecycle_request(5, LifecycleRequest::Foreground, active()),
        LifecycleAdmission::Reduce
    );
    assert_eq!(
        queue.accept_lifecycle_request(4, LifecycleRequest::Background, active()),
        LifecycleAdmission::FlushOnly
    );
    assert_eq!(
        queue.accept_lifecycle_request(4, LifecycleRequest::Background, active()),
        LifecycleAdmission::Rejected
    );
}
