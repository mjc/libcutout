use cutout_music::{MusicMonitor, MusicMonitorRequest, MusicMonitorResume, MusicMonitorStart};

#[test]
fn passive_request_starts_without_a_ride_or_authorization() {
    let mut monitor = MusicMonitor::default();
    assert!(monitor.is_scene_active());
    assert_eq!(monitor.take_start(), None);
    monitor.request(MusicMonitorRequest::Observe);
    assert_eq!(monitor.take_start(), Some(MusicMonitorStart::Observe));
    monitor.suspend();
    assert_eq!(monitor.take_start(), None);
    assert_eq!(monitor.resume(), MusicMonitorResume::Restored);
    assert_eq!(monitor.take_start(), Some(MusicMonitorStart::Observe));
    assert_eq!(monitor.resume(), MusicMonitorResume::AlreadyActive);
}

#[test]
fn explicit_authorization_is_consumed_once_and_not_downgraded_by_passive_request() {
    let mut monitor = MusicMonitor::default();
    monitor.request(MusicMonitorRequest::Authorize);
    monitor.request(MusicMonitorRequest::Observe);
    assert_eq!(monitor.take_start(), Some(MusicMonitorStart::Authorize));
    assert_eq!(monitor.take_start(), Some(MusicMonitorStart::Observe));
}

#[test]
fn foreground_recovery_after_authorization_is_passive() {
    let mut monitor = MusicMonitor::default();
    monitor.request(MusicMonitorRequest::Authorize);
    assert_eq!(monitor.take_start(), Some(MusicMonitorStart::Authorize));
    monitor.suspend();
    // A provider callback can save a credential while suspended, but cannot
    // restart observation. Foreground recovery does not need a ride.
    assert_eq!(monitor.take_start(), None);
    assert_eq!(monitor.resume(), MusicMonitorResume::Restored);
    assert_eq!(monitor.take_start(), Some(MusicMonitorStart::Observe));
    // If foreground arrives before the callback, further starts remain passive.
    assert_eq!(monitor.take_start(), Some(MusicMonitorStart::Observe));
}

#[test]
fn suspension_discards_an_unused_authorization_grant() {
    let mut monitor = MusicMonitor::default();
    monitor.request(MusicMonitorRequest::Authorize);
    monitor.suspend();
    assert_eq!(monitor.resume(), MusicMonitorResume::Restored);
    assert_eq!(monitor.take_start(), Some(MusicMonitorStart::Observe));
}

#[test]
fn cancellation_prevents_resume_and_discards_authorization() {
    let mut monitor = MusicMonitor::default();
    monitor.request(MusicMonitorRequest::Authorize);
    monitor.cancel();
    assert_eq!(monitor.take_start(), None);
    monitor.suspend();
    assert_eq!(monitor.resume(), MusicMonitorResume::NoRequest);
    monitor.request(MusicMonitorRequest::Observe);
    assert_eq!(monitor.take_start(), Some(MusicMonitorStart::Observe));
}
