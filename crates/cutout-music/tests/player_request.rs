use cutout_music::player_request::MusicPlayerRequest;

#[test]
fn lost_callback_times_out_and_late_callback_cannot_clear_retry() {
    let mut request = MusicPlayerRequest::default();
    let first = request.begin(0).expect("first request");
    assert_eq!(request.begin(9_999), None);
    let retry = request.begin(10_000).expect("lost callback timed out");
    assert_ne!(first, retry);
    assert!(!request.complete(first));
    assert_eq!(request.begin(10_001), None);
    assert!(request.complete(retry));
    assert!(request.begin(10_002).is_some());
}

#[test]
fn normal_completion_allows_next_poll_and_reset_invalidates_old_callback() {
    let mut request = MusicPlayerRequest::default();
    let first = request.begin(100).expect("first request");
    assert!(request.complete(first));
    assert!(!request.complete(first));
    let second = request.begin(101).expect("next poll");
    request.reset();
    let third = request.begin(102).expect("new connection");
    assert_ne!(second, third);
    assert!(!request.complete(second));
    assert!(request.complete(third));
}

#[test]
fn backwards_clock_does_not_expire_an_outstanding_request() {
    let mut request = MusicPlayerRequest::default();
    assert!(request.begin(100).is_some());
    assert_eq!(request.begin(0), None);
    assert_eq!(request.begin(10_099), None);
    assert!(request.begin(10_100).is_some());
}
