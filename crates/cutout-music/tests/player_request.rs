use cutout_music::player_request::{
    MusicPlayerRequest, MusicPlayerRequestCompletion, MusicPlayerRequestExpiration,
};

#[test]
fn lost_callback_times_out_and_late_callback_cannot_clear_retry() {
    let mut request = MusicPlayerRequest::default();
    let first = request.begin(0).expect("first request");
    assert_eq!(request.begin(9_999), None);
    let retry = request.begin(10_000).expect("lost callback timed out");
    assert_ne!(first, retry);
    assert_eq!(
        request.complete(first, 10_000),
        MusicPlayerRequestCompletion::Stale
    );
    assert_eq!(request.begin(10_001), None);
    assert_eq!(
        request.complete(retry, 10_001),
        MusicPlayerRequestCompletion::Accepted
    );
    assert!(request.begin(10_002).is_some());
}

#[test]
fn normal_completion_allows_next_poll_and_reset_invalidates_old_callback() {
    let mut request = MusicPlayerRequest::default();
    let first = request.begin(100).expect("first request");
    assert_eq!(
        request.complete(first, 101),
        MusicPlayerRequestCompletion::Accepted
    );
    assert_eq!(
        request.complete(first, 102),
        MusicPlayerRequestCompletion::Stale
    );
    let second = request.begin(101).expect("next poll");
    request.reset();
    let third = request.begin(102).expect("new connection");
    assert_ne!(second, third);
    assert_eq!(
        request.complete(second, 103),
        MusicPlayerRequestCompletion::Stale
    );
    assert_eq!(
        request.complete(third, 104),
        MusicPlayerRequestCompletion::Accepted
    );
}

#[test]
fn superseded_poll_releases_pending_slot_for_immediate_repoll() {
    let mut request = MusicPlayerRequest::default();
    let poll = request.begin(100).expect("initial poll");
    let poll_revision = request.observation_revision();
    request.mark_observed(101);

    assert_eq!(
        request.complete_if_current(poll, poll_revision, 102),
        MusicPlayerRequestCompletion::Stale
    );
    assert!(request.begin(102).is_some());
}

#[test]
fn backwards_clock_does_not_expire_an_outstanding_request() {
    let mut request = MusicPlayerRequest::default();
    assert!(request.begin(100).is_some());
    assert_eq!(request.begin(0), None);
    assert_eq!(request.begin(10_099), None);
    assert!(request.begin(10_100).is_some());
}

#[test]
fn callback_at_request_deadline_is_stale() {
    let mut request = MusicPlayerRequest::default();
    let id = request.begin(1_000).expect("request");
    assert_eq!(
        request.complete(id, 11_000),
        MusicPlayerRequestCompletion::Stale
    );
    assert!(request.begin(11_001).is_some());
}

#[test]
fn deadline_distinguishes_pending_expired_and_replaced_requests() {
    let mut request = MusicPlayerRequest::default();
    let first = request.begin(1_000).expect("request");

    assert_eq!(
        request.expire(first, 10_999),
        MusicPlayerRequestExpiration::Pending
    );
    assert_eq!(
        request.expire(first, 11_000),
        MusicPlayerRequestExpiration::Expired
    );
    assert_eq!(
        request.expire(first, 11_001),
        MusicPlayerRequestExpiration::Stale
    );

    let replacement = request.begin(11_001).expect("replacement");
    assert_eq!(
        request.expire(first, 21_001),
        MusicPlayerRequestExpiration::Stale
    );
    assert_eq!(
        request.complete(replacement, 11_002),
        MusicPlayerRequestCompletion::Accepted
    );
}
