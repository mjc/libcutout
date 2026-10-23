use crate::pevcap_limits::{LimitExceeded, PevcapLimits, PevcapUsage, check_limit};
use crate::recording::RideMotionObservation;
use cutout_core::{
    PevcapEncoding, PevcapEvent, PevcapPhoneLocation, PevcapReader, PevcapRecord, PhoneControlMode,
    PhoneDriveLimits,
};
use cutout_music::{
    MusicEventTiming, MusicHistoryPolicy, MusicHistoryState, MusicProvider, MusicRideEvent,
    MusicRideEventKind, MusicTimeline, MusicTimelineOutcome, MusicValidationError,
};
use cutout_ride_maps::{
    AverageSpeedMillimetresPerSecond, Coordinate, ElapsedTimeContinuation, ElapsedTimeMilliseconds,
    ElapsedTimeStateParts, ElapsedTimeSummary, LocationAdmission, LocationSample, LocationSource,
    MAX_LIVE_ROUTE_POINTS, MonotonicMilliseconds, MovingTimeAvailability, MovingTimeContinuation,
    MovingTimeObservation, MovingTimeSource, MovingTimeSourceStateParts, MovingTimeStateParts,
    MovingTimeSummary, RideAppPresence, RideAppPresenceOrder, RideEvent, RideInactivityEligibility,
    RideInactivityEvidence, RideLifecycleState, RideMapPoint, RideMapRecorder, RideMapSegmentId,
    RidePointCount, RidePointSequence, RideRecoveryDecision, RideSegmentStartReason, RideSummary,
    RouteCameraBounds, RouteCameraRegion, RouteDisplayBudget, RouteDisplayPoint,
    RouteEndpointMetadata, RoutePrivacyPolicy, RouteProjectionAccumulator,
    RouteSegmentDisplayMetadata, RouteTelemetryState, RouteViewport, TransitionError,
    VehicleIdentity, WallClockUnixMilliseconds, count_segment_runs, route_admission,
    route_camera_region, route_segment_display_metadata,
};
use hex::encode as hex_encode;
use rusqlite::types::Value;
use rusqlite::{Connection, ErrorCode, OptionalExtension, params, params_from_iter};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};
use thiserror::Error;
use uuid::Uuid;

mod capture_data;
mod capture_history;
mod capture_settings;
pub use capture_history::{PevcapCaptureCursor, PevcapCapturePage, StoredPevcapCapture};
pub(crate) use capture_settings::CaptureTransportReadPrefix;
mod live_capture_history;
pub use live_capture_history::{
    LiveCaptureHistoryCursor, LiveCaptureHistoryEntry, LiveCaptureHistoryPage,
};
mod live_capture;
pub use live_capture::{
    LIVE_CAPTURE_EVENT_LIMIT_BYTES, LIVE_CAPTURE_HEADER_LIMIT_BYTES,
    LIVE_CAPTURE_TOTAL_LIMIT_BYTES, LiveCaptureEvent, LiveCaptureEventKind, LiveCaptureId,
    LiveCaptureIntegrity, LiveCaptureLocationAdmission, LiveCaptureLocationObservation,
    LiveCaptureLocationValidation, LiveCaptureSnapshot, LiveCaptureState,
};
mod music_privacy;
mod recorded_capture;
pub use recorded_capture::RecordedCapture;
mod capture_submission;
pub use capture_submission::*;
mod charging;
pub use charging::*;
mod device_drivetrain;
mod device_vehicle;
mod migrations;
mod normalization;
#[cfg(test)]
pub(crate) use normalization::pause_legacy_archive_for_test;
mod phone_control;
mod ride_inactivity;
mod ride_write;
mod service;
mod worker;
#[cfg(test)]
pub(crate) use migrations::{CURRENT_SCHEMA_VERSION, create_current_schema};
use migrations::{migrate, table_exists, verify_current_schema};
use ride_write::{
    LocationWriteDecision, LocationWriteMode, RideWriteState, RideWriteStateParts,
    wall_clock_now_milliseconds,
};

const COMMAND_QUEUE_CAPACITY: usize = 64;
pub(crate) const DATABASE_PAGE_SIZE_BYTES: u32 = 16 * 1024;
const MAX_QUERY_LIMIT: u32 = 500;
/// Default number of rides loaded for one history page.
pub const DEFAULT_HISTORY_PAGE_LIMIT: u32 = 50;
/// Default number of surrounding routes shown in history context.
pub const DEFAULT_HISTORY_CONTEXT_ROUTES: u32 = 8;
/// Default per-route display budget for history context.
pub const DEFAULT_HISTORY_CONTEXT_POINTS_PER_ROUTE: u32 = 512;
/// Default aggregate display budget for history context.
pub const DEFAULT_HISTORY_CONTEXT_TOTAL_POINTS: u32 = 4_096;
/// Recent-history window used by the mobile map filter.
pub const DEFAULT_HISTORY_RECENT_WINDOW_MILLISECONDS: u64 = 30 * 24 * 60 * 60 * 1_000;
const MAX_HISTORY_CONTEXT_ROUTES: usize = 8;
const MAX_HISTORY_CONTEXT_POINTS: usize = 4_096;
const PEVCAP_LOCATION_BATCH_SIZE: usize = 256;
const MAX_MANAGED_PEVCAP_DIRECTORY_ATTEMPTS: usize = 8;
const DEFAULT_ROUTE_PROJECTION_TIMEOUT: Duration = Duration::from_secs(2);

#[cfg(test)]
mod pevcap_confirmation_hooks {
    use std::{cell::RefCell, thread::LocalKey};

    type Hook = RefCell<Option<Box<dyn FnOnce()>>>;

    thread_local! {
        pub(super) static BEFORE_BEGIN: Hook = const { RefCell::new(None) };
        pub(super) static BEFORE_STORE: Hook = const { RefCell::new(None) };
        pub(super) static AFTER_ABORT: Hook = const { RefCell::new(None) };
    }

    pub(super) fn run(key: &'static LocalKey<Hook>) {
        if let Some(hook) = key.with(|slot| slot.borrow_mut().take()) {
            hook();
        }
    }
}

#[derive(Clone, Copy)]
struct PevcapRoutePoint {
    sample: LocationSample,
    segment_id: RideMapSegmentId,
    start_reason: RideSegmentStartReason,
    telemetry_state: RouteTelemetryState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SegmentStartReasonInput {
    Infer,
    Recorded(RideSegmentStartReason),
}

#[derive(Clone, Copy)]
struct PreviousRidePoint {
    sequence: i64,
    segment_id: RideMapSegmentId,
    sample: LocationSample,
}

const MAX_STORED_TEXT_CHARS: usize = 512;
const MAX_VERIFIED_CONNECTION_LIFECYCLE_MUTATIONS: usize = 3;
/// Origin of a canonical ride record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideSource {
    /// A ride recorded from live platform location updates.
    Live,
    /// A ride projected from an imported PEVCAP artifact.
    PevcapImport,
}

impl RideSource {
    const fn as_db(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::PevcapImport => "pevcap_import",
        }
    }
}

/// Stable identifier for one canonical ride.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RideId(Uuid);

impl RideId {
    /// Creates a new random ride identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Returns the UUID representation.
    #[must_use]
    pub const fn uuid(self) -> Uuid {
        self.0
    }

    /// Creates an identifier from a UUID returned by the mobile boundary.
    #[must_use]
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }
}

impl Default for RideId {
    fn default() -> Self {
        Self::new()
    }
}

/// One raw BMS voltage observation stored independently of ride recording.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BmsVoltageSampleRecord {
    device_identity: String,
    session_identifier: String,
    event_sequence: u64,
    monotonic_milliseconds: u64,
    wall_clock_milliseconds: u64,
    observation_index: u16,
    pack_index: Option<u16>,
    pack_observation_index: Option<u16>,
    millivolts: i32,
}

impl BmsVoltageSampleRecord {
    /// Creates a validated raw BMS voltage sample.
    ///
    /// # Errors
    ///
    /// Returns an error when either identity is empty or exceeds the storage bound.
    pub fn new(
        device_identity: &str,
        session_identifier: &str,
        event_sequence: u64,
        monotonic_milliseconds: u64,
        wall_clock_milliseconds: u64,
        observation_index: u16,
        millivolts: i32,
    ) -> Result<Self, StorageError> {
        Ok(Self {
            device_identity: normalize_stored_text(device_identity, "BMS device identity")?,
            session_identifier: normalize_stored_text(session_identifier, "BMS session identity")?,
            event_sequence,
            monotonic_milliseconds,
            wall_clock_milliseconds,
            observation_index,
            pack_index: None,
            pack_observation_index: None,
            millivolts,
        })
    }

    /// Attaches the protocol-assigned pack identity when it is known.
    #[must_use]
    pub const fn with_pack_identity(
        mut self,
        pack_index: Option<u16>,
        pack_observation_index: Option<u16>,
    ) -> Self {
        self.pack_index = pack_index;
        self.pack_observation_index = pack_observation_index;
        self
    }
}

/// Mandatory upper bound for a growing database query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueryLimit(u32);

impl QueryLimit {
    /// Validates a non-zero query limit no larger than the service maximum.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::InvalidQueryLimit`] when the value is outside the supported range.
    pub fn new(value: u32) -> Result<Self, StorageError> {
        (1..=MAX_QUERY_LIMIT)
            .contains(&value)
            .then_some(Self(value))
            .ok_or(StorageError::InvalidQueryLimit(value))
    }

    const fn get(self) -> u32 {
        self.0
    }
}

/// Rust-owned bounds for one history overview context projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HistoryContextBudget {
    history_page_limit: QueryLimit,
    max_routes: usize,
    per_route_budget: RouteDisplayBudget,
    total_point_budget: usize,
}

impl HistoryContextBudget {
    /// Creates bounded history-page, route-count, and point budgets.
    ///
    /// `history_page_limit` bounds the records loaded for the context query. `max_routes` bounds
    /// the context routes returned after selected-ride exclusion. The point budgets apply after
    /// viewport filtering and are enforced by the Rust projection worker.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::InvalidHistoryContextBudget`] when any bound is outside the
    /// supported range.
    pub fn new(
        history_page_limit: u32,
        max_routes: u32,
        per_route_points: u32,
        total_point_budget: u32,
    ) -> Result<Self, StorageError> {
        let history_page_limit = QueryLimit::new(history_page_limit)?;
        let max_routes =
            usize::try_from(max_routes).map_err(|_| StorageError::InvalidHistoryContextBudget {
                field: "max routes",
                value: max_routes,
            })?;
        if !(1..=MAX_HISTORY_CONTEXT_ROUTES).contains(&max_routes) {
            return Err(StorageError::InvalidHistoryContextBudget {
                field: "max routes",
                value: u32::try_from(max_routes).unwrap_or(u32::MAX),
            });
        }
        let per_route_budget = usize::try_from(per_route_points)
            .ok()
            .and_then(RouteDisplayBudget::new)
            .ok_or(StorageError::InvalidHistoryContextBudget {
                field: "per-route point budget",
                value: per_route_points,
            })?;
        let total_point_budget = usize::try_from(total_point_budget)
            .ok()
            .filter(|value| (1..=MAX_HISTORY_CONTEXT_POINTS).contains(value))
            .ok_or(StorageError::InvalidHistoryContextBudget {
                field: "aggregate point budget",
                value: total_point_budget,
            })?;
        Ok(Self {
            history_page_limit,
            max_routes,
            per_route_budget,
            total_point_budget,
        })
    }

    /// Returns the bounded history page size.
    #[must_use]
    pub const fn history_page_limit(self) -> QueryLimit {
        self.history_page_limit
    }

    /// Returns the maximum number of context routes.
    #[must_use]
    pub const fn max_routes(self) -> usize {
        self.max_routes
    }

    /// Returns the per-route display budget.
    #[must_use]
    pub const fn per_route_budget(self) -> RouteDisplayBudget {
        self.per_route_budget
    }

    /// Returns the aggregate display-point budget.
    #[must_use]
    pub const fn total_point_budget(self) -> usize {
        self.total_point_budget
    }
}

/// Stable cursor for descending ride-history pagination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RideCursor {
    created_at_ms: u64,
    ride_id: RideId,
}

impl RideCursor {
    /// Restores a cursor returned by a previous query page.
    #[must_use]
    pub const fn new(created_at_milliseconds: u64, ride_id: RideId) -> Self {
        Self {
            created_at_ms: created_at_milliseconds,
            ride_id,
        }
    }

    /// Returns the creation-time component.
    #[must_use]
    pub const fn created_at_milliseconds(self) -> u64 {
        self.created_at_ms
    }

    /// Returns the ride-identifier component.
    #[must_use]
    pub const fn ride_id(self) -> RideId {
        self.ride_id
    }
}

/// Distance policy for bounded ride-history queries.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RideHistoryDistanceFilter {
    /// Include every saved ride, regardless of its recorded distance.
    #[default]
    AllRides,
    /// Include rides whose persisted distance is strictly greater than 500 feet.
    OverFiveHundredFeet,
}

/// Rust-owned filters for bounded ride-history queries.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RideHistoryQuery {
    created_after_ms: Option<WallClockUnixMilliseconds>,
    vehicle_identity: Option<VehicleIdentity>,
    search_text: Option<String>,
    distance: RideHistoryDistanceFilter,
}

/// A vehicle identity used by at least one visible ride, with its persisted display name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RideHistoryVehicleOption {
    platform_identifier: String,
    display_name: Option<String>,
}

impl RideHistoryVehicleOption {
    /// Returns the stable platform-local identity.
    #[must_use]
    pub fn platform_identifier(&self) -> &str {
        &self.platform_identifier
    }

    /// Returns the persisted display name, when one has been saved for this identity.
    #[must_use]
    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }
}

impl RideHistoryQuery {
    /// Creates a query from optional date, vehicle-identity, search, and distance filters.
    #[must_use]
    pub fn new(
        created_after_milliseconds: Option<u64>,
        vehicle_identity: Option<&str>,
        search_text: Option<&str>,
        distance: RideHistoryDistanceFilter,
    ) -> Self {
        Self {
            created_after_ms: created_after_milliseconds.map(WallClockUnixMilliseconds::new),
            vehicle_identity: vehicle_identity.and_then(VehicleIdentity::new),
            search_text: search_text
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
            distance,
        }
    }

    /// Returns the inclusive lower creation-time bound.
    #[must_use]
    pub const fn created_after_milliseconds(&self) -> Option<u64> {
        match self.created_after_ms {
            Some(value) => Some(value.as_u64()),
            None => None,
        }
    }

    /// Returns the stable platform identity filter.
    #[must_use]
    pub const fn created_after_timestamp(&self) -> Option<WallClockUnixMilliseconds> {
        self.created_after_ms
    }

    /// Returns the typed stable platform identity filter.
    #[must_use]
    pub fn vehicle_identity(&self) -> Option<&VehicleIdentity> {
        self.vehicle_identity.as_ref()
    }

    /// Returns the normalized user search text.
    #[must_use]
    pub fn search_text(&self) -> Option<&str> {
        self.search_text.as_deref()
    }

    /// Returns the minimum-distance policy for this history query.
    #[must_use]
    pub const fn distance_filter(&self) -> RideHistoryDistanceFilter {
        self.distance
    }
}

/// Bounded ride-history projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RideRecord {
    id: RideId,
    source: RideSource,
    state: RideLifecycleState,
    created_at_ms: u64,
    monotonic_created_at_ms: Option<u64>,
    monotonic_last_event_ms: Option<u64>,
    paused_at_ms: Option<u64>,
    paused_duration_ms: u64,
    completed_duration_ms: u64,
    last_recorded_point_duration_ms: Option<u64>,
    moving_time_checkpoint: MovingTimeCheckpoint,
    last_recorded_point_moving_time: MovingTimeSummary,
    last_recorded_point_elapsed_time: ElapsedTimeSummary,
    updated_at_ms: u64,
    duration_ms: u64,
    summary: RideSummary,
    segment_count: u64,
    background_gap_count: u64,
    candidate_vehicle: Option<String>,
    associated_vehicle: Option<String>,
    candidate_vehicle_name: Option<String>,
    associated_vehicle_name: Option<String>,
    associated_at_ms: Option<u64>,
    last_telemetry_at_ms: Option<u64>,
    last_location_observation: Option<LocationObservationClocks>,
    motion_observation: Option<RideMotionObservation>,
    stationary_location_observation: Option<LocationSample>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LocationObservationClocks {
    monotonic_ms: u64,
    wall_clock_ms: u64,
}

impl RideRecord {
    /// The durable moving-time continuation and its commit revision.
    #[must_use]
    pub const fn moving_time_checkpoint(&self) -> MovingTimeCheckpoint {
        self.moving_time_checkpoint
    }

    /// Moving-time prefix captured at the latest accepted live route point.
    #[must_use]
    pub const fn last_recorded_point_moving_time(&self) -> MovingTimeSummary {
        self.last_recorded_point_moving_time
    }

    /// Returns the elapsed-time summary at a monotonic instant without writing.
    #[must_use]
    pub fn elapsed_time_summary_at(&self, as_of: MonotonicMilliseconds) -> ElapsedTimeSummary {
        self.moving_time_checkpoint.elapsed_time_at(as_of)
    }

    /// Returns the committed elapsed-time summary without projecting a later instant.
    #[must_use]
    pub const fn elapsed_time(&self) -> ElapsedTimeSummary {
        self.moving_time_checkpoint
            .elapsed_time_continuation()
            .summary()
    }

    /// Returns the elapsed-time summary captured at the latest accepted live route point.
    #[must_use]
    pub const fn last_recorded_point_elapsed_time(&self) -> ElapsedTimeSummary {
        self.last_recorded_point_elapsed_time
    }

    /// Projects the current committed continuation to the caller's monotonic as-of time.
    #[must_use]
    pub fn moving_time_summary_at(&self, as_of: MonotonicMilliseconds) -> MovingTimeSummary {
        self.moving_time_checkpoint.projected_at(as_of)
    }

    /// Duration at the latest accepted live route point, if one has been recorded.
    #[must_use]
    pub const fn last_recorded_point_duration_milliseconds(
        &self,
    ) -> Option<cutout_ride_maps::RideDurationMilliseconds> {
        match self.last_recorded_point_duration_ms {
            Some(milliseconds) => Some(cutout_ride_maps::RideDurationMilliseconds::new(
                milliseconds,
            )),
            None => None,
        }
    }

    /// Latest durable wheel-motion evidence.
    #[must_use]
    pub const fn motion_observation(&self) -> Option<RideMotionObservation> {
        self.motion_observation
    }

    /// Most recent parked-motion boundary, retained even after subsequent riding telemetry.
    #[must_use]
    pub const fn last_stationary_motion_at_milliseconds(&self) -> Option<u64> {
        match self.motion_observation {
            Some(motion) => motion.last_stationary_at_milliseconds,
            None => None,
        }
    }

    /// Latest suppressed stationary GPS observation, separate from route geometry.
    #[must_use]
    pub const fn stationary_location_observation(&self) -> Option<LocationSample> {
        self.stationary_location_observation
    }

    /// Returns the ride identifier.
    #[must_use]
    pub const fn id(&self) -> RideId {
        self.id
    }

    /// Returns the persisted lifecycle state.
    #[must_use]
    pub const fn state(&self) -> RideLifecycleState {
        self.state
    }

    /// Returns the ride origin.
    #[must_use]
    pub const fn source(&self) -> RideSource {
        self.source
    }

    /// Returns the creation time in Unix milliseconds.
    #[must_use]
    pub const fn created_at_milliseconds(&self) -> u64 {
        self.created_at_ms
    }

    /// Returns the monotonic timestamp captured when recording began, when available.
    #[must_use]
    pub const fn monotonic_created_at_milliseconds(&self) -> Option<u64> {
        self.monotonic_created_at_ms
    }

    /// Returns the latest persisted lifecycle monotonic timestamp.
    #[must_use]
    pub const fn monotonic_last_event_milliseconds(&self) -> Option<u64> {
        self.monotonic_last_event_ms
    }

    /// Returns the current pause start timestamp, when paused.
    #[must_use]
    pub const fn paused_at_milliseconds(&self) -> Option<u64> {
        self.paused_at_ms
    }

    /// Returns accumulated paused duration.
    #[must_use]
    pub const fn paused_duration_milliseconds(&self) -> u64 {
        self.paused_duration_ms
    }

    /// Returns the terminal active duration.
    #[must_use]
    pub const fn completed_duration_milliseconds(&self) -> u64 {
        self.completed_duration_ms
    }

    /// Returns the last durable update time in Unix milliseconds.
    #[must_use]
    pub const fn updated_at_milliseconds(&self) -> u64 {
        self.updated_at_ms
    }

    /// Returns the Rust recording duration, excluding paused and interrupted intervals.
    #[must_use]
    pub const fn duration_milliseconds(&self) -> u64 {
        self.duration_ms
    }

    /// Returns the Rust-derived summary.
    #[must_use]
    pub const fn summary(&self) -> RideSummary {
        self.summary
    }

    /// Returns the Rust-derived average speed when the ride has distance and elapsed time.
    #[must_use]
    pub fn average_speed_millimetres_per_second(&self) -> Option<u64> {
        self.summary
            .average_speed_millimetres_per_second(self.duration_ms)
            .map(AverageSpeedMillimetresPerSecond::as_u64)
    }

    /// Returns the number of persisted route segments.
    #[must_use]
    pub const fn segment_count(&self) -> u64 {
        self.segment_count
    }

    /// Returns the number of nonempty segments that begin after a background gap.
    #[must_use]
    pub const fn background_gap_count(&self) -> u64 {
        self.background_gap_count
    }

    /// Returns the candidate vehicle identity snapshotted at ride start.
    #[must_use]
    pub fn candidate_vehicle(&self) -> Option<&str> {
        self.candidate_vehicle.as_deref()
    }

    /// Returns the confirmed vehicle identity, when association succeeded.
    #[must_use]
    pub fn associated_vehicle(&self) -> Option<&str> {
        self.associated_vehicle.as_deref()
    }

    /// Returns the persisted display name for the candidate vehicle, when available.
    #[must_use]
    pub fn candidate_vehicle_name(&self) -> Option<&str> {
        self.candidate_vehicle_name.as_deref()
    }

    /// Returns the persisted display name for the associated vehicle, when available.
    #[must_use]
    pub fn associated_vehicle_name(&self) -> Option<&str> {
        self.associated_vehicle_name.as_deref()
    }

    /// Returns the monotonic association timestamp.
    #[must_use]
    pub const fn associated_at_milliseconds(&self) -> Option<u64> {
        self.associated_at_ms
    }

    /// Returns the newest confirmed telemetry timestamp.
    #[must_use]
    pub const fn last_telemetry_at_milliseconds(&self) -> Option<u64> {
        self.last_telemetry_at_ms
    }

    /// Latest validated GPS observation time, distinct from lifecycle timing.
    #[must_use]
    pub const fn last_location_observed_monotonic_milliseconds(&self) -> Option<u64> {
        match self.last_location_observation {
            Some(clocks) => Some(clocks.monotonic_ms),
            None => None,
        }
    }

    /// Original source wall clock corresponding to the latest validated GPS observation.
    #[must_use]
    pub const fn last_location_observed_wall_clock_milliseconds(&self) -> Option<u64> {
        match self.last_location_observation {
            Some(clocks) => Some(clocks.wall_clock_ms),
            None => None,
        }
    }
}

/// One bounded page of ride-history projections.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RidePage {
    rides: Vec<RideRecord>,
    next_cursor: Option<RideCursor>,
}

impl RidePage {
    /// Returns the page records in stable newest-first order.
    #[must_use]
    pub fn rides(&self) -> &[RideRecord] {
        &self.rides
    }

    /// Returns the cursor for the next page, when more records exist.
    #[must_use]
    pub const fn next_cursor(&self) -> Option<RideCursor> {
        self.next_cursor
    }
}

/// One bounded contextual route from a history overview projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryContextRoute {
    ride_id: RideId,
    projection: RoutePointProjection,
}

impl HistoryContextRoute {
    /// Returns the ride represented by this contextual route.
    #[must_use]
    pub const fn ride_id(&self) -> RideId {
        self.ride_id
    }

    /// Returns the bounded route projection.
    #[must_use]
    pub const fn projection(&self) -> &RoutePointProjection {
        &self.projection
    }
}

/// Bounded context routes for a history overview.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryContextProjection {
    routes: Vec<HistoryContextRoute>,
    source_history_route_count: u64,
    context_route_count: u64,
    total_display_point_count: u64,
    routes_omitted_by_budget: bool,
    history_page_has_more: bool,
}

impl HistoryContextProjection {
    /// Returns contextual routes in the same newest-first order as the history page.
    #[must_use]
    pub fn routes(&self) -> &[HistoryContextRoute] {
        &self.routes
    }

    /// Returns the number of rides loaded from the bounded history page.
    #[must_use]
    pub const fn source_history_route_count(&self) -> u64 {
        self.source_history_route_count
    }

    /// Returns the number of eligible context rides after selected-ride exclusion.
    #[must_use]
    pub const fn context_route_count(&self) -> u64 {
        self.context_route_count
    }

    /// Returns the number of display points emitted across contextual routes.
    #[must_use]
    pub const fn total_display_point_count(&self) -> u64 {
        self.total_display_point_count
    }

    /// Returns whether the point or route budget omitted eligible context data.
    #[must_use]
    pub const fn routes_omitted_by_budget(&self) -> bool {
        self.routes_omitted_by_budget
    }

    /// Returns whether more history exists after the loaded bounded page.
    #[must_use]
    pub const fn history_page_has_more(&self) -> bool {
        self.history_page_has_more
    }
}

/// Stable cursor for ascending route-point pagination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoutePointCursor(u64);

impl RoutePointCursor {
    /// Restores a cursor returned by a previous query page.
    #[must_use]
    pub const fn new(sequence: u64) -> Self {
        Self(sequence)
    }

    /// Returns the sequence component.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.0
    }
}

/// One canonical route point with its stable ride sequence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoutePoint {
    sequence: u64,
    segment_id: u64,
    start_reason: RideSegmentStartReason,
    sample: LocationSample,
    telemetry_state: RouteTelemetryState,
}

impl RoutePoint {
    /// Returns the stable zero-based sequence within the ride.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }

    /// Returns the Rust-owned segment identity within the ride.
    #[must_use]
    pub const fn segment_id(self) -> u64 {
        self.segment_id
    }

    /// Returns why this point's segment began.
    #[must_use]
    pub const fn start_reason(self) -> RideSegmentStartReason {
        self.start_reason
    }

    /// Returns the admitted canonical sample.
    #[must_use]
    pub const fn sample(self) -> LocationSample {
        self.sample
    }

    /// Returns the Rust-owned telemetry provenance.
    #[must_use]
    pub const fn telemetry_state(self) -> RouteTelemetryState {
        self.telemetry_state
    }
}

/// One bounded page of canonical route points.
#[derive(Clone, Debug, PartialEq)]
pub struct RoutePointPage {
    points: Vec<RoutePoint>,
    next_cursor: Option<RoutePointCursor>,
}

/// One bounded snapshot of the durable state required to restore a mobile ride-map core.
#[derive(Clone, Debug, PartialEq)]
pub struct RideRestoreSnapshot {
    last_connected_device: Option<String>,
    ride: Option<RideRecord>,
    route_points: Vec<RoutePoint>,
    music_history_policy: Option<MusicHistoryPolicy>,
    music_history_available: bool,
    inactivity_evidence: Option<RideInactivityEvidence>,
}

impl RideRestoreSnapshot {
    /// Returns the last connected platform identity, when one was persisted.
    #[must_use]
    pub fn last_connected_device(&self) -> Option<&str> {
        self.last_connected_device.as_deref()
    }

    /// Returns the recoverable ride, when one exists.
    #[must_use]
    pub const fn ride(&self) -> Option<&RideRecord> {
        self.ride.as_ref()
    }

    /// Returns the bounded canonical route tail for the recoverable ride.
    #[must_use]
    pub fn route_points(&self) -> &[RoutePoint] {
        &self.route_points
    }

    /// Returns the durable music-history policy when it could be read.
    #[must_use]
    pub const fn music_history_policy(&self) -> Option<MusicHistoryPolicy> {
        self.music_history_policy
    }

    /// Returns whether the music history query succeeded for the recoverable ride.
    #[must_use]
    pub const fn music_history_available(&self) -> bool {
        self.music_history_available
    }

    /// Returns app/device inactivity evidence for the recoverable ride, when persisted.
    #[must_use]
    pub const fn inactivity_evidence(&self) -> Option<&RideInactivityEvidence> {
        self.inactivity_evidence.as_ref()
    }
}

impl RoutePointPage {
    /// Returns points in stable ascending sequence order.
    #[must_use]
    pub fn points(&self) -> &[RoutePoint] {
        &self.points
    }

    /// Returns the cursor for the next page, when more points exist.
    #[must_use]
    pub const fn next_cursor(&self) -> Option<RoutePointCursor> {
        self.next_cursor
    }
}

/// Bounded startup state produced by Rust after recovery.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BootstrapSnapshot {
    recovered_rides: Arc<[RideId]>,
}

impl BootstrapSnapshot {
    /// Returns rides changed from recording to interrupted during acquisition.
    #[must_use]
    pub fn recovered_rides(&self) -> &[RideId] {
        &self.recovered_rides
    }
}

/// Parsed `SQLite` version components.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SqliteVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

impl SqliteVersion {
    /// Returns the major version.
    #[must_use]
    pub const fn major(self) -> u32 {
        self.major
    }

    /// Returns the minor version.
    #[must_use]
    pub const fn minor(self) -> u32 {
        self.minor
    }

    /// Returns the patch version.
    #[must_use]
    pub const fn patch(self) -> u32 {
        self.patch
    }

    fn parse(value: &str) -> Result<Self, StorageError> {
        let mut parts = value.split('.');
        let major = parse_version_part(parts.next(), value)?;
        let minor = parse_version_part(parts.next(), value)?;
        let patch = parse_version_part(parts.next(), value)?;
        if parts.next().is_some() {
            return Err(StorageError::InvalidSqliteVersion(value.to_owned()));
        }
        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

fn parse_version_part(value: Option<&str>, whole: &str) -> Result<u32, StorageError> {
    value
        .ok_or_else(|| StorageError::InvalidSqliteVersion(whole.to_owned()))?
        .parse()
        .map_err(|_| StorageError::InvalidSqliteVersion(whole.to_owned()))
}

/// `SQLite` capabilities observed from the opened connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SqliteCapabilities {
    version: SqliteVersion,
    rtree: bool,
    fts5: bool,
}

/// Persisted voltage-sag model for one stable device identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VoltageSagModelRecord {
    /// Schema version of the model payload.
    pub schema_version: u16,
    /// Effective resistance in milliohms.
    pub effective_resistance_milliohms: u32,
    /// Number of observations contributing to the model.
    pub observations: u16,
    /// Whether the model was verified against hardware.
    pub hardware_verified: bool,
    /// Last learning wall-clock timestamp in Unix milliseconds.
    pub last_learned_wall_clock_milliseconds: u64,
}

/// Persisted phone-generated alarm preferences for one stable device identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhoneAlarmPreferencesRecord {
    enabled: bool,
    pwm_duty_percent: u8,
}

/// Saved phone input mode and current limits for one stable device identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhoneDriveSettingsRecord {
    mode: PhoneControlMode,
    limits: PhoneDriveLimits,
}

impl PhoneDriveSettingsRecord {
    /// Creates one atomic device-scoped phone-drive configuration.
    #[must_use]
    pub const fn new(mode: PhoneControlMode, limits: PhoneDriveLimits) -> Self {
        Self { mode, limits }
    }

    /// Returns the saved input layout. `Disabled` never activates the session.
    #[must_use]
    pub const fn mode(self) -> PhoneControlMode {
        self.mode
    }

    /// Returns the checked throttle and brake current limits.
    #[must_use]
    pub const fn limits(self) -> PhoneDriveLimits {
        self.limits
    }
}

impl Default for PhoneDriveSettingsRecord {
    fn default() -> Self {
        Self::new(PhoneControlMode::Disabled, PhoneDriveLimits::default())
    }
}

impl PhoneAlarmPreferencesRecord {
    /// Creates validated preferences expressed as consumed PWM duty.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::InvalidStoredValue`] unless duty is in `1..=100`.
    pub fn new(enabled: bool, pwm_duty_percent: u8) -> Result<Self, StorageError> {
        if !(1..=100).contains(&pwm_duty_percent) {
            return Err(StorageError::InvalidStoredValue {
                field: "phone alarm PWM duty percent",
                value: pwm_duty_percent.to_string(),
            });
        }
        Ok(Self {
            enabled,
            pwm_duty_percent,
        })
    }

    /// Returns whether the device may produce phone-generated alarms.
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    /// Returns the alarm threshold as consumed PWM duty percent.
    #[must_use]
    pub const fn pwm_duty_percent(self) -> u8 {
        self.pwm_duty_percent
    }
}

/// Durable PEVCAP import outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PevcapImportOutcome {
    /// The artifact contained route locations and produced a canonical ride plus managed capture.
    RideAndCapture,
    /// The artifact contained no route locations and produced only a managed capture.
    CaptureOnly,
    /// The artifact digest already has a committed import receipt.
    AlreadyImported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NewPevcapImportOutcome {
    RideAndCapture,
    CaptureOnly,
}

impl TryFrom<PevcapImportOutcome> for NewPevcapImportOutcome {
    type Error = StorageError;

    fn try_from(outcome: PevcapImportOutcome) -> Result<Self, Self::Error> {
        match outcome {
            PevcapImportOutcome::RideAndCapture => Ok(Self::RideAndCapture),
            PevcapImportOutcome::CaptureOnly => Ok(Self::CaptureOnly),
            PevcapImportOutcome::AlreadyImported => Err(StorageError::PevcapPreviewChanged),
        }
    }
}

impl NewPevcapImportOutcome {
    const fn as_public(self) -> PevcapImportOutcome {
        match self {
            Self::RideAndCapture => PevcapImportOutcome::RideAndCapture,
            Self::CaptureOnly => PevcapImportOutcome::CaptureOnly,
        }
    }

    const fn as_db(self) -> &'static str {
        match self {
            Self::RideAndCapture => "ride_and_capture",
            Self::CaptureOnly => "capture_only",
        }
    }
}

/// Non-fatal warning discovered during PEVCAP preflight.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PevcapImportWarning {
    /// No phone locations were present, so confirmation will not create an empty ride.
    NoRouteLocations,
}

/// Immutable PEVCAP preflight result that must be confirmed explicitly.
#[derive(Clone, Debug, PartialEq)]
pub struct PevcapImportPreview {
    source_path: PathBuf,
    encoding: PevcapEncoding,
    artifact_digest: String,
    artifact_size: u64,
    record_count: u64,
    location_count: u64,
    duration_milliseconds: u64,
    outcome: PevcapImportOutcome,
    warnings: Arc<[PevcapImportWarning]>,
}

fn normalize_stored_text(value: &str, field: &'static str) -> Result<String, StorageError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > MAX_STORED_TEXT_CHARS {
        return Err(StorageError::InvalidStoredValue {
            field,
            value: value.chars().take(MAX_STORED_TEXT_CHARS).collect(),
        });
    }
    Ok(value.to_owned())
}

fn normalize_optional_device_name(
    value: Option<&str>,
    platform_identifier: &str,
) -> Result<Option<String>, StorageError> {
    let Some(value) = value else { return Ok(None) };
    let value = value.trim();
    if value.is_empty() || value == platform_identifier {
        return Ok(None);
    }
    normalize_stored_text(value, "display name").map(Some)
}

/// Canonicalizes a display name for one platform-local device identity.
///
/// Empty and identifier-equal names are absent; all other names are trimmed and bounded.
///
/// # Errors
///
/// Returns [`StorageError`] when the platform identifier or display name exceeds the stored text
/// bound.
pub fn normalize_device_display_name(
    platform_identifier: &str,
    display_name: &str,
) -> Result<Option<String>, StorageError> {
    let platform_identifier = normalize_stored_text(platform_identifier, "platform identifier")?;
    normalize_optional_device_name(Some(display_name), &platform_identifier)
}

fn normalize_optional_stored_text(
    value: Option<&str>,
    field: &'static str,
) -> Result<Option<String>, StorageError> {
    value
        .map(|value| normalize_stored_text(value, field))
        .transpose()
}

impl PevcapImportPreview {
    /// Returns the canonical source path reviewed by preflight.
    #[must_use]
    pub fn source_path(&self) -> &Path {
        &self.source_path
    }

    /// Returns the encoding used to replay and decode the artifact.
    #[must_use]
    pub const fn encoding(&self) -> PevcapEncoding {
        self.encoding
    }

    /// Returns the lowercase SHA-256 artifact digest.
    #[must_use]
    pub fn artifact_digest(&self) -> &str {
        &self.artifact_digest
    }

    /// Returns the source artifact size in bytes.
    #[must_use]
    pub const fn artifact_size(&self) -> u64 {
        self.artifact_size
    }

    /// Returns the number of bounded transport records.
    #[must_use]
    pub const fn record_count(&self) -> u64 {
        self.record_count
    }

    /// Returns the number of records carrying phone locations.
    #[must_use]
    pub const fn location_count(&self) -> u64 {
        self.location_count
    }

    /// Returns the capture duration between its earliest and latest monotonic timestamps.
    #[must_use]
    pub const fn duration_milliseconds(&self) -> u64 {
        self.duration_milliseconds
    }

    /// Returns the durable outcome that confirmation will produce.
    #[must_use]
    pub const fn outcome(&self) -> PevcapImportOutcome {
        self.outcome
    }

    /// Returns non-fatal warnings the confirmation UI should present.
    #[must_use]
    pub fn warnings(&self) -> &[PevcapImportWarning] {
        &self.warnings
    }
    #[cfg(test)]
    #[allow(
        clippy::too_many_arguments,
        reason = "test-only malformed preview fixture"
    )]
    pub(crate) fn from_parts(
        source_path: PathBuf,
        encoding: PevcapEncoding,
        artifact_digest: String,
        artifact_size: u64,
        record_count: u64,
        location_count: u64,
        duration_milliseconds: u64,
        outcome: PevcapImportOutcome,
        warnings: Vec<PevcapImportWarning>,
    ) -> Self {
        Self {
            source_path,
            encoding,
            artifact_digest,
            artifact_size,
            record_count,
            location_count,
            duration_milliseconds,
            outcome,
            warnings: Arc::from(warnings),
        }
    }
}

/// Durable result of one PEVCAP artifact import.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PevcapImportReceipt {
    /// Recording provenance when publication came from a completed local writer.
    pub recording: Option<RecordedCapture>,
    /// Canonical ride created for the artifact, when route locations were present.
    pub ride_id: Option<RideId>,
    /// SHA-256 digest of the source artifact, as lowercase hexadecimal.
    pub artifact_digest: String,
    /// Whether confirmation produced a ride and capture or only a capture.
    pub outcome: PevcapImportOutcome,
    /// Number of transport records read from the artifact.
    pub record_count: u64,
    /// Number of phone-location samples admitted to the ride.
    pub location_count: u64,
    /// Whether this digest was already imported.
    pub duplicate: bool,
}

/// Stable identifier for a stored trail definition.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TrailId(Uuid);

impl TrailId {
    /// Creates a new trail identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Returns the UUID representation.
    #[must_use]
    pub const fn uuid(self) -> Uuid {
        self.0
    }

    /// Creates an identifier from a UUID returned by the mobile boundary.
    #[must_use]
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }
}

impl Default for TrailId {
    fn default() -> Self {
        Self::new()
    }
}

/// Stable key for the one selected-device row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SelectedDeviceKey(Uuid);

impl SelectedDeviceKey {
    pub(crate) const VALUE: Self = Self(Uuid::from_u128(1));

    pub(crate) fn blob(self) -> Vec<u8> {
        self.0.as_bytes().to_vec()
    }
}

/// Stable key for the one ride-session marker row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RideSessionMarkerKey(Uuid);

impl RideSessionMarkerKey {
    const VALUE: Self = Self(Uuid::from_u128(2));

    fn blob(self) -> Vec<u8> {
        self.0.as_bytes().to_vec()
    }
}

/// Stable key for the one last-connected-device row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LastConnectedDeviceKey(Uuid);

impl LastConnectedDeviceKey {
    pub(crate) const VALUE: Self = Self(Uuid::from_u128(3));

    pub(crate) fn blob(self) -> Vec<u8> {
        self.0.as_bytes().to_vec()
    }
}

/// Validated `SQLite` row identifier used only to connect a spatial table to its R*Tree row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SpatialRowId(i64);

impl SpatialRowId {
    fn from_sqlite(value: i64) -> Result<Self, StorageError> {
        if (1..=i64::from(i32::MAX)).contains(&value) {
            Ok(Self(value))
        } else {
            Err(StorageError::InvalidStoredValue {
                field: "spatial row identifier",
                value: value.to_string(),
            })
        }
    }

    const fn get(self) -> i64 {
        self.0
    }
}

/// One canonical trail segment stored in the spatial index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TrailSegment {
    /// Owning trail identifier.
    pub trail_id: TrailId,
    /// Stable sequence within the trail.
    pub sequence: u32,
    /// Segment start coordinate.
    pub start: Coordinate,
    /// Segment end coordinate.
    pub end: Coordinate,
}

/// One charging/food point stored in the spatial index.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MapPoint {
    /// Stable point identifier.
    pub id: MapPointId,
    /// User-visible name.
    pub name: String,
    /// Point coordinate.
    pub coordinate: Coordinate,
}

/// Stable UUID for a stored map point.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MapPointId(Uuid);

impl Default for MapPointId {
    fn default() -> Self {
        Self::new()
    }
}

impl MapPointId {
    /// Creates an identifier for a newly stored point.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Restores an identifier returned by a previous query page.
    #[must_use]
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    /// Returns the UUID used at the mobile boundary.
    #[must_use]
    pub const fn uuid(self) -> Uuid {
        self.0
    }
}

/// Validated fixed-point WGS84 query bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeoBounds {
    minimum_latitude: i32,
    maximum_latitude: i32,
    minimum_longitude: i32,
    maximum_longitude: i32,
}

impl GeoBounds {
    /// Validates geographic bounds. A minimum longitude greater than the maximum denotes an
    /// antimeridian-crossing viewport.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::InvalidGeographicBounds`] for non-finite, out-of-range, or reversed
    /// latitude bounds.
    pub fn new(
        minimum_latitude_degrees: f64,
        maximum_latitude_degrees: f64,
        minimum_longitude_degrees: f64,
        maximum_longitude_degrees: f64,
    ) -> Result<Self, StorageError> {
        let minimum = Coordinate::from_degrees(minimum_latitude_degrees, minimum_longitude_degrees)
            .map_err(|_| StorageError::InvalidGeographicBounds)?;
        let maximum = Coordinate::from_degrees(maximum_latitude_degrees, maximum_longitude_degrees)
            .map_err(|_| StorageError::InvalidGeographicBounds)?;
        if minimum.latitude().as_i32() > maximum.latitude().as_i32() {
            return Err(StorageError::InvalidGeographicBounds);
        }
        Ok(Self {
            minimum_latitude: minimum.latitude().as_i32(),
            maximum_latitude: maximum.latitude().as_i32(),
            minimum_longitude: minimum.longitude().as_i32(),
            maximum_longitude: maximum.longitude().as_i32(),
        })
    }

    const fn crosses_antimeridian(self) -> bool {
        self.minimum_longitude > self.maximum_longitude
    }
}

/// Stable cursor for trail-segment pagination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TrailSegmentCursor {
    trail_id: TrailId,
    sequence: u32,
}

impl TrailSegmentCursor {
    /// Restores a cursor returned by a previous query page.
    #[must_use]
    pub const fn new(trail_id: TrailId, sequence: u32) -> Self {
        Self { trail_id, sequence }
    }

    /// Returns the trail identifier component.
    #[must_use]
    pub const fn trail_id(self) -> TrailId {
        self.trail_id
    }

    /// Returns the segment sequence component.
    #[must_use]
    pub const fn sequence(self) -> u32 {
        self.sequence
    }
}

/// One bounded page of trail-segment projections.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrailSegmentPage {
    segments: Vec<TrailSegment>,
    next_cursor: Option<TrailSegmentCursor>,
}

impl TrailSegmentPage {
    /// Returns the page in stable trail-identifier/sequence order.
    #[must_use]
    pub fn segments(&self) -> &[TrailSegment] {
        &self.segments
    }

    /// Returns the next stable page cursor, when more candidates exist.
    #[must_use]
    pub const fn next_cursor(&self) -> Option<TrailSegmentCursor> {
        self.next_cursor
    }
}

/// Stable cursor for map-point pagination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MapPointCursor(MapPointId);

impl MapPointCursor {
    /// Restores a cursor returned by a previous query page.
    #[must_use]
    pub const fn new(id: MapPointId) -> Self {
        Self(id)
    }

    /// Returns the point identifier component.
    #[must_use]
    pub const fn id(self) -> MapPointId {
        self.0
    }
}

/// One bounded page of map-point projections.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MapPointPage {
    points: Vec<MapPoint>,
    next_cursor: Option<MapPointCursor>,
}

impl MapPointPage {
    /// Returns the page in stable identifier order.
    #[must_use]
    pub fn points(&self) -> &[MapPoint] {
        &self.points
    }

    /// Returns the next stable page cursor, when more candidates exist.
    #[must_use]
    pub const fn next_cursor(&self) -> Option<MapPointCursor> {
        self.next_cursor
    }
}

impl SqliteCapabilities {
    /// Returns the runtime `SQLite` version.
    #[must_use]
    pub const fn sqlite_version(&self) -> SqliteVersion {
        self.version
    }

    /// Returns whether the built-in R*Tree module is available.
    #[must_use]
    pub const fn has_rtree(&self) -> bool {
        self.rtree
    }

    /// Returns whether FTS5 is available.
    #[must_use]
    pub const fn has_fts5(&self) -> bool {
        self.fts5
    }
}

/// Errors from the Rust-owned ride database service.
#[derive(Debug, Error)]
pub enum StorageError {
    /// A charging field or server snapshot is outside its supported contract.
    #[error("invalid charging input")]
    ChargingInvalidInput,
    /// Charging revisions, state or operation identities do not match.
    #[error("charging state changed")]
    ChargingConflict,
    /// A directory pin cannot be edited or withdrawn by this installation.
    #[error("charging spot belongs to another contributor")]
    ChargingNotOwned,
    /// Community mutations are fenced by a pending or completed account deletion.
    #[error("community account deletion prevents contributions")]
    CommunityAccountUnavailable,
    /// The durable community queue reached its bound.
    #[error("charging outbox is full")]
    ChargingOutboxFull,
    /// A recording identity or byte digest already belongs to different provenance.
    #[error("capture identity conflicts with retained provenance")]
    CaptureIdentityConflict,
    /// A live-capture header, event or timestamp is outside its supported bound.
    #[error("invalid live-capture input: {0}")]
    LiveCaptureInputInvalid(&'static str),
    /// A live capture reached its bounded durable byte or sequence limit.
    #[error("live capture reached its persistence limit")]
    LiveCaptureLimitExceeded,
    /// The live capture is not active and cannot accept this lifecycle operation.
    #[error("live capture is not active")]
    LiveCaptureNotActive,
    /// The database path could not be used.
    #[error("invalid database path")]
    InvalidPath,
    /// The database is already owned by another path in this process.
    #[error("a different database is already open in this process")]
    AlreadyOpenForDifferentPath,
    /// `SQLite` reported an error.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Filesystem setup failed.
    #[error("database filesystem error: {0}")]
    Io(#[from] std::io::Error),
    /// Migration found a schema newer than this build supports.
    #[error("unsupported database schema version {0}")]
    UnsupportedSchemaVersion(i64),
    /// The file is `SQLite` but is not Cutout's application database.
    #[error("invalid Cutout database identity")]
    InvalidDatabaseIdentity,
    /// `SQLite` reported that the file failed its quick integrity check.
    #[error("database integrity check failed: {0}")]
    IntegrityCheckFailed(String),
    /// A growing query was not bounded by a supported limit.
    #[error("invalid query limit {0}; expected 1..={MAX_QUERY_LIMIT}")]
    InvalidQueryLimit(u32),
    /// A capture-history cursor has an invalid digest or unrepresentable import timestamp.
    #[error("invalid PEVCAP capture history cursor")]
    InvalidPevcapCaptureCursor,
    /// A history context budget exceeded one of the Rust-owned limits.
    #[error("invalid history context budget for {field}: {value}")]
    InvalidHistoryContextBudget {
        /// Human-readable budget component.
        field: &'static str,
        /// Rejected caller value.
        value: u32,
    },
    /// Geographic bounds were non-finite, out of range, or had reversed latitudes.
    #[error("invalid geographic bounds")]
    InvalidGeographicBounds,
    /// A persisted value could not be decoded.
    #[error("invalid stored {field}: {value}")]
    InvalidStoredValue {
        /// Field that failed decoding.
        field: &'static str,
        /// Bounded stored value for diagnostics.
        value: String,
    },
    /// `SQLite` reported a malformed version string.
    #[error("invalid SQLite version {0}")]
    InvalidSqliteVersion(String),
    /// The requested ride does not exist.
    #[error("ride was not found")]
    NotFound,
    /// A live ride changed while an inactivity-based settlement was pending.
    #[error("ride inactivity evidence or lifecycle changed before settlement")]
    RideInactivityConflict,
    /// The bounded music timeline cannot accept another event.
    #[error("ride music timeline is full")]
    MusicTimelineFull,
    /// A music event reused a sequence for different data.
    #[error("music event sequence {sequence} conflicts with stored data")]
    MusicSequenceConflict {
        /// Conflicting sequence number.
        sequence: i64,
    },
    /// A music event's monotonic timestamp moved backwards.
    #[error("music event sequence {sequence} is out of order")]
    MusicEventOutOfOrder {
        /// Out-of-order sequence number.
        sequence: i64,
    },
    /// A music event sequence skipped one or more durable rows.
    #[error("music event sequence {sequence} expected {expected}")]
    MusicSequenceGap {
        /// Sequence supplied by the caller.
        sequence: i64,
        /// Next contiguous sequence required by storage.
        expected: i64,
    },
    /// A music event attempted to use a different policy than the stored ride policy.
    #[error("music history policy conflicts with stored policy")]
    MusicPolicyConflict,
    /// The requested lifecycle transition is invalid.
    #[error("invalid ride lifecycle transition: {0}")]
    Transition(#[from] TransitionError),
    /// The ride cannot accept a point in its current state.
    #[error("ride is not accepting location samples in state {0:?}")]
    InvalidRideState(RideLifecycleState),
    /// The bounded command queue is full.
    #[error("ride database command queue is full")]
    QueueFull,
    /// A verified-connection admission plan exceeded its lifecycle mutation limit.
    #[error("verified-connection admission has {actual} lifecycle mutations; maximum is {maximum}")]
    TooManyConnectionAdmissionMutations {
        /// Number of submitted lifecycle mutations.
        actual: usize,
        /// Maximum supported mutation count.
        maximum: usize,
    },
    /// A verified-connection admission plan has an inconsistent created-ride target.
    #[error("invalid verified-connection admission plan")]
    InvalidConnectionAdmissionPlan,
    /// The worker stopped before accepting a command.
    #[error("ride database worker stopped")]
    WorkerStopped,
    /// The worker response channel was dropped before the command's outcome was observed.
    ///
    /// The command may already have committed. Callers must treat this as an unknown write
    /// outcome, reacquire the service, and reconcile durable state before retrying; never retry
    /// the same sample blindly.
    #[error("ride database response was dropped")]
    ResponseDropped,
    /// A second worker could not be started.
    #[error("could not start ride database worker: {0}")]
    WorkerStart(String),
    /// PEVCAP decoding failed after the artifact was opened.
    #[error("PEVCAP import failed: {0}")]
    PevcapImport(String),
    /// A PEVCAP artifact exceeded a bounded preflight resource limit.
    #[error("PEVCAP {resource} limit exceeded: {actual} > {limit}")]
    PevcapLimitExceeded {
        /// Bounded resource name.
        resource: &'static str,
        /// Maximum accepted value.
        limit: u64,
        /// Observed value.
        actual: u64,
    },
    /// A published original-byte capture could not be found.
    #[error("PEVCAP capture was not found")]
    PevcapCaptureNotFound,
    /// Stored capture bytes failed export integrity verification.
    #[error("PEVCAP capture export integrity check failed: {0}")]
    PevcapCaptureIntegrity(String),
    /// A capture digest was not canonical lowercase SHA-256 hex.
    #[error("invalid PEVCAP capture digest")]
    InvalidPevcapCaptureDigest,
    /// The source artifact changed after the user reviewed its preflight result.
    #[error("PEVCAP artifact changed after preflight")]
    PevcapPreviewChanged,
    /// Another confirmation for the same artifact is already being committed.
    #[error("PEVCAP artifact import is already in progress")]
    PevcapImportInProgress,
    /// The host wall clock is earlier than the Unix epoch.
    #[error("system wall clock is earlier than the Unix epoch")]
    SystemClock(#[from] std::time::SystemTimeError),
    /// The requested spatial `SQLite` extension is unavailable.
    #[error("SQLite R*Tree capability is unavailable")]
    SpatialCapabilityUnavailable,
    /// The spatial schema could not be initialized.
    #[error("SQLite spatial schema initialization failed: {0}")]
    SpatialSchemaInitialization(String),
    /// A cancellable route projection was cancelled before completion.
    #[error("route projection cancelled")]
    Cancelled,
    /// A durable route projection exceeded its caller-provided deadline.
    #[error("route projection deadline exceeded")]
    DeadlineExceeded,
}

enum SpatialSchemaState {
    Uninitialized,
    Ready,
    Unavailable,
    Failed(String),
}

/// Opaque synchronous handle to the one process-owned ride database worker.
#[derive(Clone)]
pub struct RideDatabase {
    sender: SyncSender<Command>,
    service_id: Uuid,
    bootstrap: BootstrapSnapshot,
    path: Arc<PathBuf>,
    diagnostic_recording_enabled: Arc<AtomicBool>,
    diagnostic_recording_available: Arc<AtomicBool>,
    worker_alive: Arc<AtomicBool>,
    maintenance_demand: Arc<AtomicU64>,
    pub(crate) marker_writes: Arc<Mutex<crate::ride_session_marker::MarkerWriteState>>,
}

impl std::fmt::Debug for RideDatabase {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RideDatabase")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

/// A location write accepted by the bounded database queue but not necessarily committed yet.
///
/// The ticket is deliberately polled rather than awaited so a platform location callback never
/// has to wait for `SQLite` while holding its recording-state lock.
#[derive(Debug)]
pub struct PendingLocationWrite {
    response: Receiver<Result<LocationWriteResult, StorageError>>,
    consumed: bool,
}

/// Queue behavior for one ride-location checkpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocationQueueMode {
    /// Return [`StorageError::QueueFull`] when the bounded worker queue is full.
    Bounded,
    /// Wait for bounded worker capacity; use only from a background worker.
    WaitForCapacity,
}

/// Domain evidence committed with an accepted ride-location write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RideLocationCheckpoint {
    moving_time_observation: Option<MovingTimeObservation>,
    latest_vehicle_moving_time_observation: Option<MovingTimeObservation>,
    inactivity_evidence: Option<RideInactivityEvidence>,
}

impl RideLocationCheckpoint {
    /// Creates a checkpoint from the current Core-owned moving-time and inactivity snapshots.
    #[must_use]
    pub const fn new(
        moving_time_observation: Option<MovingTimeObservation>,
        latest_vehicle_moving_time_observation: Option<MovingTimeObservation>,
        inactivity_evidence: RideInactivityEvidence,
    ) -> Self {
        Self {
            moving_time_observation,
            latest_vehicle_moving_time_observation,
            inactivity_evidence: Some(inactivity_evidence),
        }
    }

    const fn without_inactivity_evidence(
        moving_time_observation: Option<MovingTimeObservation>,
        latest_vehicle_moving_time_observation: Option<MovingTimeObservation>,
    ) -> Self {
        Self {
            moving_time_observation,
            latest_vehicle_moving_time_observation,
            inactivity_evidence: None,
        }
    }
}

/// A BMS batch accepted by the bounded database queue but not necessarily committed yet.
#[must_use = "poll or wait for the durable BMS batch result"]
#[derive(Debug)]
pub struct PendingBmsVoltageWrite {
    response: Receiver<Result<(), StorageError>>,
    consumed: bool,
}

/// Pollable barrier that completes after every earlier storage command has settled.
#[must_use = "poll or wait for the ride checkpoint to complete"]
#[derive(Debug)]
pub struct PendingRideCheckpoint {
    response: Receiver<Result<(), StorageError>>,
    consumed: bool,
}

/// Pollable durable receipt for one opaque marker replacement or removal.
#[must_use = "poll or wait for the durable marker mutation"]
#[derive(Debug)]
pub struct PendingRideSessionMarkerWrite {
    response: Receiver<Result<(), StorageError>>,
    consumed: bool,
}

/// Clock-only GPS evidence tied to one unchanged durable route point.
#[derive(Clone, Copy, Debug)]
pub struct RideLocationObservationCheckpoint {
    /// Ride that owns the observation.
    pub ride_id: RideId,
    /// Last durable point whose native location values must still match.
    pub expected_point_sequence: u64,
    /// Validated same-location observation with original source clocks.
    pub observation: LocationSample,
}

/// Durable stationary GPS evidence that does not add geometry or distance.
#[derive(Clone, Copy, Debug)]
pub struct RideStationaryLocationCheckpoint {
    /// Owning logical ride.
    pub ride_id: RideId,
    /// Route point count captured before this observation.
    pub expected_point_count: u64,
    /// Validated live observation with its original clocks.
    pub observation: LocationSample,
}

/// Durable outcome of a clock-only location checkpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideLocationObservationCheckpointOutcome {
    /// New observation clocks were committed without adding a route point.
    Applied,
    /// The exact observation clock was already durable.
    AlreadyObserved,
    /// The ride, point, native values, or clock order no longer match the checkpoint.
    PointChanged,
}

/// A live music transition accepted by the bounded worker but not yet committed.
#[must_use = "poll or wait for the durable music event result"]
#[derive(Debug)]
pub struct PendingMusicEventWrite {
    response: Receiver<Result<MusicTimelineRecordResult, StorageError>>,
    consumed: bool,
}

/// A music-history query accepted by the bounded worker but not yet completed.
#[must_use = "poll or wait for the music history result"]
#[derive(Debug)]
pub struct PendingMusicHistoryRead {
    response: Receiver<Result<MusicHistory, StorageError>>,
    consumed: bool,
}

/// A music-history mutation accepted by the bounded worker but not yet committed.
#[must_use = "poll or wait for the durable music-history mutation result"]
#[derive(Debug)]
pub struct PendingMusicHistoryWrite {
    response: Receiver<Result<(), StorageError>>,
    consumed: bool,
}

/// A live ride creation accepted by the bounded database queue but not yet committed.
#[must_use = "poll or wait for the durable live ride creation result"]
#[derive(Debug)]
pub struct PendingLiveRideCreation {
    response: Receiver<Result<CommittedRideMovingTimeCheckpoint, StorageError>>,
    consumed: bool,
}

/// A restore snapshot accepted by the bounded worker but not necessarily loaded yet.
#[must_use = "poll or wait for the durable ride restore snapshot"]
#[derive(Debug)]
pub struct PendingRideRestoreSnapshot {
    response: Receiver<Result<RideRestoreSnapshot, StorageError>>,
    consumed: bool,
}

/// Lifecycle transition accepted by the bounded SQLite worker but not yet committed.
#[must_use = "poll or wait for the durable lifecycle transition result"]
#[derive(Debug)]
pub struct PendingRideLifecycleTransition {
    response: Receiver<Result<CommittedRideMovingTimeCheckpoint, StorageError>>,
    consumed: bool,
}

/// Lifecycle mutation included in an ordered verified-connection admission.
#[derive(Clone, Debug)]
pub enum VerifiedConnectionLifecycleMutation {
    /// Applies one timestamped transition to an existing ride.
    Transition {
        /// Ride being transitioned.
        ride_id: RideId,
        /// Requested lifecycle event.
        event: RideEvent,
        /// Wall-clock event timestamp.
        occurred_at_ms: u64,
        /// Monotonic event timestamp, when available.
        monotonic_at_ms: Option<u64>,
    },
    /// Creates and starts a new live ride.
    StartLive {
        /// Wall-clock creation timestamp.
        created_at_ms: u64,
        /// Monotonic creation timestamp.
        monotonic_created_at_ms: u64,
        /// Candidate vehicle identity for the new ride.
        candidate_vehicle: Option<String>,
        /// Retention chosen for this new ride, committed with its creation.
        music_history_policy: MusicHistoryPolicy,
    },
}

/// Ride targeted by verified-connection metadata in the same transaction.
#[derive(Clone, Copy, Debug)]
pub enum VerifiedConnectionRideTarget {
    /// An existing ride.
    Existing(RideId),
    /// The live ride created by this admission's lifecycle plan.
    Created,
}

/// Complete metadata snapshot written with verified-connection admission.
#[derive(Clone, Debug)]
pub struct VerifiedConnectionRideMetadata {
    /// Existing ride or ride created by the automatic lifecycle plan.
    pub target: VerifiedConnectionRideTarget,
    /// Candidate vehicle identity, or `None` to clear it.
    pub candidate_vehicle: Option<String>,
    /// Associated vehicle identity, or `None` to clear it.
    pub associated_vehicle: Option<String>,
    /// Monotonic association timestamp.
    pub associated_at_ms: Option<u64>,
    /// Monotonic last-telemetry timestamp.
    pub last_telemetry_at_ms: Option<u64>,
    /// Monotonic source boundary for a newly verified vehicle connection generation.
    pub moving_time_source_boundary_at_ms: Option<u64>,
}

/// Durable result of one ordered verified-connection admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedConnectionAdmissionOutcome {
    /// Whether the verified identity matched the currently selected device.
    pub selected_device_matches: bool,
    /// Ride created by the automatic lifecycle plan, if any.
    pub created_ride_id: Option<RideId>,
    /// Checkpoint committed for the final/current ride, when this admission changed one.
    pub committed_ride_checkpoint: Option<CommittedRideMovingTimeCheckpoint>,
}

/// Verified connection admission accepted by the bounded worker but not necessarily committed.
#[must_use = "poll or wait for the verified connection admission result"]
#[derive(Debug)]
pub struct PendingVerifiedConnectionAdmission {
    response: Receiver<Result<VerifiedConnectionAdmissionOutcome, StorageError>>,
    consumed: bool,
}

/// Durable outcome for one asynchronously appended location.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocationWriteResult {
    admission: LocationAdmission,
    sequence: Option<u64>,
    moving_time_checkpoint: MovingTimeCheckpoint,
}

/// Versioned durable moving-time continuation returned with a recording commit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MovingTimeCheckpoint {
    revision: u64,
    continuation: MovingTimeContinuation,
    elapsed_time: ElapsedTimeContinuation,
}

/// Moving-time checkpoint committed with a ride lifecycle or creation transaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommittedRideMovingTimeCheckpoint {
    ride_id: RideId,
    lifecycle: RideLifecycleState,
    checkpoint: MovingTimeCheckpoint,
}

impl CommittedRideMovingTimeCheckpoint {
    /// Returns the ride whose transaction committed this checkpoint.
    #[must_use]
    pub const fn ride_id(self) -> RideId {
        self.ride_id
    }

    /// Returns the lifecycle state committed with this checkpoint.
    #[must_use]
    pub const fn lifecycle(self) -> RideLifecycleState {
        self.lifecycle
    }

    /// Returns the moving-time checkpoint committed with the ride.
    #[must_use]
    pub const fn checkpoint(self) -> MovingTimeCheckpoint {
        self.checkpoint
    }
}

impl MovingTimeCheckpoint {
    /// Returns the monotonically increasing durable commit revision.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }

    /// Returns the continuation snapshot committed at `revision`.
    #[must_use]
    pub const fn continuation(self) -> MovingTimeContinuation {
        self.continuation
    }

    /// Returns the elapsed-time continuation committed at the same revision.
    #[must_use]
    pub const fn elapsed_time_continuation(self) -> ElapsedTimeContinuation {
        self.elapsed_time
    }

    /// Projects the committed continuation to an as-of monotonic instant without writing.
    #[must_use]
    pub fn projected_at(self, as_of: MonotonicMilliseconds) -> MovingTimeSummary {
        self.continuation.projected_at(as_of)
    }

    /// Projects elapsed time at a monotonic instant without writing.
    #[must_use]
    pub fn elapsed_time_at(self, as_of: MonotonicMilliseconds) -> ElapsedTimeSummary {
        self.elapsed_time.projected_at(as_of)
    }
}

/// One bounded, privacy-classified projection of a durable ride route.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutePointProjection {
    points: Vec<RouteDisplayPoint>,
    camera_region: Option<RouteCameraRegion>,
    canonical_camera_region: Option<RouteCameraRegion>,
    source_revision: u64,
    source_point_count: u64,
    source_segment_count: u64,
    candidate_point_count: u64,
    candidate_segment_count: u64,
    displayed_segment_count: u64,
    background_gap_count: u64,
    endpoint_metadata: RouteEndpointMetadata,
    segments: Vec<RouteSegmentDisplayMetadata>,
}

struct ProjectedRouteCandidates {
    points: Vec<RouteDisplayPoint>,
    camera_region: Option<RouteCameraRegion>,
    displayed_segment_count: u64,
    segments: Vec<RouteSegmentDisplayMetadata>,
}

#[derive(Clone, Copy)]
struct RouteSegmentProjectionMetadata {
    start_reason: RideSegmentStartReason,
    point_count: u64,
}

impl RoutePointProjection {
    /// Returns the bounded display points in canonical sequence order.
    #[must_use]
    pub fn points(&self) -> &[RouteDisplayPoint] {
        &self.points
    }

    /// Returns the Rust-computed camera region for the bounded display points.
    #[must_use]
    pub const fn camera_region(&self) -> Option<RouteCameraRegion> {
        self.camera_region
    }

    /// Returns the camera region for all canonical route points after privacy projection.
    #[must_use]
    pub const fn canonical_camera_region(&self) -> Option<RouteCameraRegion> {
        self.canonical_camera_region
    }

    /// Returns the Rust-owned source revision used to produce this projection.
    #[must_use]
    pub const fn source_revision(&self) -> u64 {
        self.source_revision
    }
    /// Returns the complete durable point count before LOD or viewport filtering.
    #[must_use]
    pub const fn source_point_count(&self) -> u64 {
        self.source_point_count
    }

    /// Returns the complete durable segment count before viewport filtering or display LOD.
    #[must_use]
    pub const fn source_segment_count(&self) -> u64 {
        self.source_segment_count
    }

    /// Returns the point count inside the requested viewport before display LOD.
    #[must_use]
    pub const fn candidate_point_count(&self) -> u64 {
        self.candidate_point_count
    }

    /// Returns the segment count represented by points inside the requested viewport.
    #[must_use]
    pub const fn candidate_segment_count(&self) -> u64 {
        self.candidate_segment_count
    }

    /// Returns the segment count represented by the bounded display points.
    #[must_use]
    pub const fn displayed_segment_count(&self) -> u64 {
        self.displayed_segment_count
    }

    /// Returns the complete durable count of segments started by a background location gap.
    #[must_use]
    pub const fn background_gap_count(&self) -> u64 {
        self.background_gap_count
    }

    /// Returns canonical endpoint sequences and viewport visibility.
    #[must_use]
    pub const fn endpoint_metadata(&self) -> RouteEndpointMetadata {
        self.endpoint_metadata
    }

    /// Returns bounded metadata for the visible projected segment runs.
    #[must_use]
    pub fn segments(&self) -> &[RouteSegmentDisplayMetadata] {
        &self.segments
    }
}
/// Cooperative cancellation token for a durable route projection.
#[derive(Clone, Default)]
pub struct RouteProjectionCancellation {
    cancelled: Arc<AtomicBool>,
    deadline: Option<Instant>,
    interrupt: Arc<Mutex<Option<Arc<rusqlite::InterruptHandle>>>>,
}

impl std::fmt::Debug for RouteProjectionCancellation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RouteProjectionCancellation")
            .field("cancelled", &self.is_cancelled())
            .field("deadline", &self.deadline)
            .finish_non_exhaustive()
    }
}

impl RouteProjectionCancellation {
    /// Creates an active cancellation token.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a cancellation token with a Rust-owned deadline.
    #[must_use]
    pub fn with_deadline(deadline: Instant) -> Self {
        Self {
            deadline: Some(deadline),
            ..Self::default()
        }
    }

    /// Requests cancellation.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.interrupt();
    }

    /// Returns whether cancellation was requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    fn is_expired(&self) -> bool {
        self.deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
    }

    fn abort_error(&self) -> Option<StorageError> {
        if self.is_expired() {
            Some(StorageError::DeadlineExceeded)
        } else if self.is_cancelled() {
            Some(StorageError::Cancelled)
        } else {
            None
        }
    }

    fn install_interrupt(&self, handle: rusqlite::InterruptHandle) {
        if let Ok(mut interrupt) = self.interrupt.lock() {
            *interrupt = Some(Arc::new(handle));
        }
    }

    fn clear_interrupt(&self) {
        if let Ok(mut interrupt) = self.interrupt.lock() {
            *interrupt = None;
        }
    }

    fn interrupt(&self) {
        if let Ok(interrupt) = self.interrupt.lock()
            && let Some(handle) = interrupt.as_ref()
        {
            handle.interrupt();
        }
    }
}

impl LocationWriteResult {
    /// Returns the durable admission decision.
    #[must_use]
    pub const fn admission(self) -> LocationAdmission {
        self.admission
    }

    /// Returns the persisted sequence when the point was accepted.
    #[must_use]
    pub const fn sequence(self) -> Option<u64> {
        self.sequence
    }

    /// Returns the durable moving-time checkpoint associated with this write outcome.
    #[must_use]
    pub const fn moving_time_checkpoint(self) -> MovingTimeCheckpoint {
        self.moving_time_checkpoint
    }
}

#[cfg(test)]
mod route_projection_cancellation_tests {
    use super::{RouteProjectionCancellation, StorageError, endpoint_is_visible};
    use cutout_ride_maps::{LatitudeE7, LongitudeE7, RouteViewport};
    use rusqlite::{Connection, ErrorCode};
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc::sync_channel,
        },
        thread,
    };

    #[test]
    fn explicit_cancellation_interrupts_a_long_sqlite_query() {
        let connection = Connection::open_in_memory().expect("SQLite opens");
        let cancellation = RouteProjectionCancellation::new();
        cancellation.install_interrupt(connection.get_interrupt_handle());
        let (query_entered_sender, query_entered_receiver) = sync_channel(0);
        let (query_release_sender, query_release_receiver) = sync_channel(0);
        let first_progress_callback = Arc::new(AtomicBool::new(true));
        let callback_is_first = Arc::clone(&first_progress_callback);
        connection
            .progress_handler(
                1_000,
                Some(move || {
                    if callback_is_first.swap(false, Ordering::AcqRel) {
                        query_entered_sender
                            .send(())
                            .expect("test query is waiting for cancellation");
                        query_release_receiver
                            .recv()
                            .expect("test query release is sent");
                    }
                    false
                }),
            )
            .expect("SQLite progress handler hook installs");

        let query = thread::spawn(move || {
            connection.query_row(
                "WITH RECURSIVE numbers(value) AS (
                     SELECT 1
                     UNION ALL
                     SELECT value + 1 FROM numbers LIMIT 5000000
                 )
                 SELECT sum(value) FROM numbers",
                [],
                |row| row.get::<_, i64>(0),
            )
        });

        query_entered_receiver
            .recv()
            .expect("query reached its first progress callback");
        cancellation.cancel();
        query_release_sender
            .send(())
            .expect("query release is received");

        let error = query
            .join()
            .expect("query thread does not panic")
            .expect_err("cancellation interrupts the query");
        assert!(matches!(
            error,
            rusqlite::Error::SqliteFailure(failure, _)
                if failure.code == ErrorCode::OperationInterrupted
        ));
    }

    #[test]
    fn endpoint_query_maps_sqlite_interrupt_to_cancellation() {
        let connection = Connection::open_in_memory().expect("SQLite opens");
        connection
            .execute_batch(
                "CREATE TABLE ride_points (
                    ride_id TEXT NOT NULL,
                    sequence INTEGER NOT NULL,
                    latitude_e7 INTEGER NOT NULL,
                    longitude_e7 INTEGER NOT NULL
                );
                INSERT INTO ride_points VALUES ('ride', 0, 400000000, -1050000000);",
            )
            .expect("route point exists");

        let cancellation = RouteProjectionCancellation::new();
        cancellation.install_interrupt(connection.get_interrupt_handle());
        let query_cancellation = cancellation.clone();
        let (query_entered_sender, query_entered_receiver) = sync_channel(0);
        let (query_release_sender, query_release_receiver) = sync_channel(0);
        let first_progress_callback = Arc::new(AtomicBool::new(true));
        let callback_is_first = Arc::clone(&first_progress_callback);
        connection
            .progress_handler(
                1,
                Some(move || {
                    if callback_is_first.swap(false, Ordering::AcqRel) {
                        query_entered_sender
                            .send(())
                            .expect("endpoint query is waiting for cancellation");
                        query_release_receiver
                            .recv()
                            .expect("endpoint query release is sent");
                    }
                    false
                }),
            )
            .expect("SQLite progress handler hook installs");

        let viewport = RouteViewport::new(
            LatitudeE7::new(399_000_000),
            LatitudeE7::new(401_000_000),
            LongitudeE7::new(-1_051_000_000),
            LongitudeE7::new(-1_049_000_000),
        )
        .expect("viewport is valid");
        let query = thread::spawn(move || {
            endpoint_is_visible(
                &connection,
                "ride",
                Some(0),
                Some(viewport),
                Some(&query_cancellation),
            )
        });

        query_entered_receiver
            .recv()
            .expect("endpoint query reached its first progress callback");
        cancellation.cancel();
        query_release_sender
            .send(())
            .expect("endpoint query release is received");

        assert!(matches!(
            query.join().expect("query thread does not panic"),
            Err(StorageError::Cancelled)
        ));
    }
}

fn default_route_projection_cancellation() -> RouteProjectionCancellation {
    let deadline = Instant::now()
        .checked_add(DEFAULT_ROUTE_PROJECTION_TIMEOUT)
        .unwrap_or_else(Instant::now);
    RouteProjectionCancellation::with_deadline(deadline)
}

#[cfg(test)]
mod default_route_projection_tests {
    use super::{Instant, default_route_projection_cancellation};

    #[test]
    fn default_route_projection_cancellation_has_a_rust_owned_deadline() {
        let cancellation = default_route_projection_cancellation();

        assert!(!cancellation.is_cancelled());
        assert!(!cancellation.is_expired());
        assert!(
            cancellation
                .deadline()
                .expect("default projection has a deadline")
                > Instant::now()
        );
    }
}

#[cfg(test)]
mod spatial_identity_tests {
    use super::{SpatialRowId, StorageError};

    #[test]
    fn sqlite_row_ids_are_validated_before_cross_table_use() {
        let id = SpatialRowId::from_sqlite(7).expect("positive SQLite row IDs are valid");
        assert_eq!(id.get(), 7);
        assert!(matches!(
            SpatialRowId::from_sqlite(0),
            Err(StorageError::InvalidStoredValue {
                field: "spatial row identifier",
                ..
            })
        ));
        assert!(matches!(
            SpatialRowId::from_sqlite(i64::from(i32::MAX) + 1),
            Err(StorageError::InvalidStoredValue {
                field: "spatial row identifier",
                ..
            })
        ));
    }
}

impl PendingLocationWrite {
    /// Returns the durable result when the worker has completed the write.
    ///
    /// `None` means that the worker is still processing the command. A terminal result is
    /// returned at most once.
    pub fn try_result(&mut self) -> Option<Result<LocationWriteResult, StorageError>> {
        if self.consumed {
            return None;
        }
        match self.response.try_recv() {
            Ok(result) => {
                self.consumed = true;
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.consumed = true;
                Some(Err(StorageError::ResponseDropped))
            }
        }
    }

    /// Waits for a durable result until a monotonic deadline, consuming a terminal result.
    ///
    /// `None` means that the worker is still delayed or queued when the deadline arrives; the
    /// ticket can be polled again later. Once a result or worker disconnect is observed, this
    /// ticket is consumed and subsequent polling returns `None`. This bounds caller wait time
    /// without pretending that a still-running `SQLite` command has been cancelled.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::ResponseDropped`] when the database worker disappears before
    /// sending a result.
    pub fn wait_result_until(
        &mut self,
        deadline: Instant,
    ) -> Result<Option<Result<LocationWriteResult, StorageError>>, StorageError> {
        if self.consumed {
            return Ok(None);
        }
        let timeout = deadline.saturating_duration_since(Instant::now());
        match self.response.recv_timeout(timeout) {
            Ok(result) => {
                self.consumed = true;
                Ok(Some(result))
            }
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.consumed = true;
                Err(StorageError::ResponseDropped)
            }
        }
    }

    /// Waits for this queued write to reach a durable result.
    ///
    /// This is intended for lifecycle barriers that must settle location writes before changing
    /// the ride state. The database worker remains the only owner of the `SQLite` connection.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::ResponseDropped`] when the database worker disappears before
    /// sending a result.
    pub fn wait_result(self) -> Result<Result<LocationWriteResult, StorageError>, StorageError> {
        self.response
            .recv()
            .map_err(|_| StorageError::ResponseDropped)
    }
}

impl PendingBmsVoltageWrite {
    /// Returns the durable result when the worker has completed the write.
    ///
    /// `None` means that the worker is still processing the batch. A terminal result is returned
    /// at most once.
    pub fn try_result(&mut self) -> Option<Result<(), StorageError>> {
        if self.consumed {
            return None;
        }
        match self.response.try_recv() {
            Ok(result) => {
                self.consumed = true;
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.consumed = true;
                Some(Err(StorageError::ResponseDropped))
            }
        }
    }

    /// Waits for the durable BMS batch result.
    ///
    /// Lifecycle barriers may wait for this result; platform callbacks should use
    /// [`Self::try_result`] instead.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::ResponseDropped`] when the database worker disappears before
    /// sending a result.
    pub fn wait_result(self) -> Result<(), StorageError> {
        self.response
            .recv()
            .map_err(|_| StorageError::ResponseDropped)?
    }
}

impl PendingRideCheckpoint {
    /// Returns the barrier result when all earlier worker commands have settled.
    ///
    /// `None` means that the worker has not reached this barrier yet. A terminal result is
    /// returned at most once.
    pub fn try_result(&mut self) -> Option<Result<(), StorageError>> {
        if self.consumed {
            return None;
        }
        match self.response.try_recv() {
            Ok(result) => {
                self.consumed = true;
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.consumed = true;
                Some(Err(StorageError::ResponseDropped))
            }
        }
    }

    /// Waits for all earlier storage commands to settle.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::ResponseDropped`] if the worker disappears before reaching the
    /// barrier, or the first error reported by an earlier queued command.
    pub fn wait_result(self) -> Result<(), StorageError> {
        self.response
            .recv()
            .map_err(|_| StorageError::ResponseDropped)?
    }
}

impl PendingRideSessionMarkerWrite {
    /// Returns the durable result once; `None` means the worker is still busy.
    pub fn try_result(&mut self) -> Option<Result<(), StorageError>> {
        if self.consumed {
            return None;
        }
        match self.response.try_recv() {
            Ok(result) => {
                self.consumed = true;
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.consumed = true;
                Some(Err(StorageError::ResponseDropped))
            }
        }
    }
}

impl PendingMusicEventWrite {
    /// Returns the durable result when the worker has completed the event.
    ///
    /// `None` means that the worker is still processing the command. A terminal result is
    /// returned at most once.
    pub fn try_result(&mut self) -> Option<Result<MusicTimelineRecordResult, StorageError>> {
        if self.consumed {
            return None;
        }
        match self.response.try_recv() {
            Ok(result) => {
                self.consumed = true;
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.consumed = true;
                Some(Err(StorageError::ResponseDropped))
            }
        }
    }
}

impl PendingMusicHistoryRead {
    /// Returns the durable history when the worker has completed the query.
    pub fn try_result(&mut self) -> Option<Result<MusicHistory, StorageError>> {
        if self.consumed {
            return None;
        }
        match self.response.try_recv() {
            Ok(result) => {
                self.consumed = true;
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.consumed = true;
                Some(Err(StorageError::ResponseDropped))
            }
        }
    }
}

impl PendingMusicHistoryWrite {
    /// Returns the durable mutation result when the worker has completed it.
    pub fn try_result(&mut self) -> Option<Result<(), StorageError>> {
        if self.consumed {
            return None;
        }
        match self.response.try_recv() {
            Ok(result) => {
                self.consumed = true;
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.consumed = true;
                Some(Err(StorageError::ResponseDropped))
            }
        }
    }
}

impl PendingLiveRideCreation {
    /// Returns the new ride identity when the worker has committed the creation.
    pub fn try_result(&mut self) -> Option<Result<RideId, StorageError>> {
        self.try_committed_result()
            .map(|result| result.map(CommittedRideMovingTimeCheckpoint::ride_id))
    }

    /// Returns the creation and moving-time checkpoint when the worker has committed them.
    pub fn try_committed_result(
        &mut self,
    ) -> Option<Result<CommittedRideMovingTimeCheckpoint, StorageError>> {
        if self.consumed {
            return None;
        }
        match self.response.try_recv() {
            Ok(result) => {
                self.consumed = true;
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.consumed = true;
                Some(Err(StorageError::ResponseDropped))
            }
        }
    }

    /// Waits for the durable ride-creation result.
    ///
    /// # Errors
    ///
    /// Returns the storage error or [`StorageError::ResponseDropped`] if the worker stops.
    pub fn wait_result(self) -> Result<RideId, StorageError> {
        self.wait_committed_result()
            .map(CommittedRideMovingTimeCheckpoint::ride_id)
    }

    /// Waits for the ride and moving-time checkpoint committed by its creation transaction.
    ///
    /// # Errors
    ///
    /// Returns the storage error or [`StorageError::ResponseDropped`] if the worker stops.
    pub fn wait_committed_result(self) -> Result<CommittedRideMovingTimeCheckpoint, StorageError> {
        self.response
            .recv()
            .map_err(|_| StorageError::ResponseDropped)?
    }
}

impl PendingRideRestoreSnapshot {
    /// Returns the restore snapshot when the worker has completed its query.
    pub fn try_result(&mut self) -> Option<Result<RideRestoreSnapshot, StorageError>> {
        if self.consumed {
            return None;
        }
        match self.response.try_recv() {
            Ok(result) => {
                self.consumed = true;
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.consumed = true;
                Some(Err(StorageError::ResponseDropped))
            }
        }
    }

    /// Waits for the worker's restore snapshot query to finish.
    ///
    /// # Errors
    ///
    /// Returns the query error or [`StorageError::ResponseDropped`] if the worker stops.
    pub fn wait_result(self) -> Result<RideRestoreSnapshot, StorageError> {
        self.response
            .recv()
            .map_err(|_| StorageError::ResponseDropped)?
    }
}

impl PendingRideLifecycleTransition {
    /// Returns the new lifecycle state when the worker has committed the transition.
    pub fn try_result(&mut self) -> Option<Result<RideLifecycleState, StorageError>> {
        self.try_committed_result()
            .map(|result| result.map(CommittedRideMovingTimeCheckpoint::lifecycle))
    }

    /// Returns the lifecycle state and moving-time checkpoint committed by the transition.
    pub fn try_committed_result(
        &mut self,
    ) -> Option<Result<CommittedRideMovingTimeCheckpoint, StorageError>> {
        if self.consumed {
            return None;
        }
        match self.response.try_recv() {
            Ok(result) => {
                self.consumed = true;
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.consumed = true;
                Some(Err(StorageError::ResponseDropped))
            }
        }
    }

    /// Waits for the committed lifecycle transition result.
    ///
    /// Use [`Self::try_result`] from callbacks or polling loops.
    ///
    /// # Errors
    ///
    /// Returns the storage error or [`StorageError::ResponseDropped`] if the worker stops.
    pub fn wait_result(self) -> Result<RideLifecycleState, StorageError> {
        self.wait_committed_result()
            .map(CommittedRideMovingTimeCheckpoint::lifecycle)
    }

    /// Waits for the lifecycle state and checkpoint committed by the transition.
    ///
    /// # Errors
    ///
    /// Returns the storage error or [`StorageError::ResponseDropped`] if the worker stops.
    pub fn wait_committed_result(self) -> Result<CommittedRideMovingTimeCheckpoint, StorageError> {
        self.response
            .recv()
            .map_err(|_| StorageError::ResponseDropped)?
    }
}

impl PendingVerifiedConnectionAdmission {
    /// Returns the admission result when the worker completes this command.
    ///
    /// `None` means that the command is still queued or executing. A terminal result is returned
    /// at most once.
    pub fn try_result(
        &mut self,
    ) -> Option<Result<VerifiedConnectionAdmissionOutcome, StorageError>> {
        if self.consumed {
            return None;
        }
        match self.response.try_recv() {
            Ok(result) => {
                self.consumed = true;
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.consumed = true;
                Some(Err(StorageError::ResponseDropped))
            }
        }
    }

    /// Waits for the ordered connection admission result.
    ///
    /// Platform callbacks should use [`Self::try_result`] rather than waiting.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::ResponseDropped`] when the worker exits before replying, or the
    /// storage error produced by the command.
    pub fn wait_result(self) -> Result<VerifiedConnectionAdmissionOutcome, StorageError> {
        self.response
            .recv()
            .map_err(|_| StorageError::ResponseDropped)?
    }
}

impl RideDatabase {
    /// Opens or reuses the one canonical database service for this process.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the path, schema, `SQLite` runtime, or worker cannot be opened.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        service::open(path)
    }

    /// Reacquires the database service after its worker has exited.
    ///
    /// A worker failure disconnects the sender held by this handle. Ordinary requests recover
    /// transparently; use this when an explicit health check or fresh handle is needed. If the
    /// worker is still healthy, this returns another handle to the existing service.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::open`] when the service cannot be reacquired.
    pub fn reopen(&self) -> Result<Self, StorageError> {
        service::reopen(self)
    }

    /// Returns the process-wide service identity.
    #[must_use]
    pub const fn service_id(&self) -> Uuid {
        self.service_id
    }

    /// Returns the bounded startup recovery snapshot produced during acquisition.
    #[must_use]
    pub const fn bootstrap(&self) -> &BootstrapSnapshot {
        &self.bootstrap
    }

    /// Returns runtime `SQLite` capabilities through the worker.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker is stopped or cannot report its capabilities.
    pub fn capabilities(&self) -> Result<SqliteCapabilities, StorageError> {
        self.request(|reply| Command::Capabilities { reply })
    }

    /// Verifies the database integrity on the storage worker.
    ///
    /// This can scan the entire database, so callers should use it only for explicit
    /// maintenance or diagnostics rather than normal startup.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::IntegrityCheckFailed`] when SQLite reports corruption, or another
    /// [`StorageError`] when the worker cannot complete the check.
    pub fn integrity_check(&self) -> Result<(), StorageError> {
        self.request(|reply| Command::IntegrityCheck { reply })
    }

    /// Creates a draft ride record.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot create the row.
    pub fn create_ride(
        &self,
        source: RideSource,
        created_at_ms: u64,
    ) -> Result<RideId, StorageError> {
        self.create_ride_with_monotonic_start(source, created_at_ms, None)
    }

    /// Creates a draft ride with separate wall-clock and monotonic start timestamps.
    ///
    /// The wall-clock value is used for history ordering; the monotonic value is used only for
    /// elapsed-duration calculations and may be absent for imported records.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot create the ride.
    pub fn create_ride_with_monotonic_start(
        &self,
        source: RideSource,
        created_at_ms: u64,
        monotonic_created_at_ms: Option<u64>,
    ) -> Result<RideId, StorageError> {
        self.request(move |reply| Command::CreateRide {
            source,
            created_at_ms,
            monotonic_created_at_ms,
            reply,
        })
    }

    /// Atomically creates and starts a live ride with its candidate vehicle metadata.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when metadata validation, the lifecycle transition, or the
    /// transactional write fails.
    pub fn create_started_ride_with_monotonic_start(
        &self,
        source: RideSource,
        created_at_ms: u64,
        monotonic_created_at_ms: Option<u64>,
        candidate_vehicle: Option<&str>,
    ) -> Result<RideId, StorageError> {
        let candidate_vehicle =
            normalize_optional_stored_text(candidate_vehicle, "candidate vehicle")?;
        let occurred_at_ms = wall_clock_now_milliseconds()?;
        self.request(move |reply| Command::CreateStartedRide {
            source,
            created_at_ms,
            monotonic_created_at_ms,
            candidate_vehicle,
            occurred_at_ms,
            reply,
        })
    }

    /// Creates an active live ride with its candidate vehicle in one transaction.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the candidate is invalid or the worker cannot commit the
    /// ride.
    pub fn create_started_live_ride(
        &self,
        created_at_ms: u64,
        monotonic_created_at_ms: u64,
        candidate_vehicle: Option<&str>,
    ) -> Result<RideId, StorageError> {
        self.queue_create_started_live_ride(
            created_at_ms,
            monotonic_created_at_ms,
            candidate_vehicle,
        )?
        .wait_result()
    }

    /// Creates a live ride and returns the checkpoint committed by the same transaction.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the candidate is invalid or the worker cannot commit the
    /// ride.
    pub fn create_started_live_ride_with_checkpoint(
        &self,
        created_at_ms: u64,
        monotonic_created_at_ms: u64,
        candidate_vehicle: Option<&str>,
    ) -> Result<CommittedRideMovingTimeCheckpoint, StorageError> {
        self.queue_create_started_live_ride(
            created_at_ms,
            monotonic_created_at_ms,
            candidate_vehicle,
        )?
        .wait_committed_result()
    }

    /// Queues an active live ride creation without waiting for SQLite.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::QueueFull`] when the bounded queue is saturated,
    /// [`StorageError::WorkerStopped`] when the worker is unavailable, or a validation error for
    /// an invalid candidate vehicle.
    pub fn queue_create_started_live_ride(
        &self,
        created_at_ms: u64,
        monotonic_created_at_ms: u64,
        candidate_vehicle: Option<&str>,
    ) -> Result<PendingLiveRideCreation, StorageError> {
        self.queue_create_started_live_ride_with_music_policy(
            created_at_ms,
            monotonic_created_at_ms,
            candidate_vehicle,
            None,
        )
    }

    /// Queues a live ride and its initial music-history privacy policy as one transaction.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::QueueFull`] when the bounded queue is saturated,
    /// [`StorageError::WorkerStopped`] when the worker is unavailable, or a validation or
    /// transaction error when the ride and policy cannot be committed together.
    pub fn queue_create_started_live_ride_with_music_policy(
        &self,
        created_at_ms: u64,
        monotonic_created_at_ms: u64,
        candidate_vehicle: Option<&str>,
        music_history_policy: Option<MusicHistoryPolicy>,
    ) -> Result<PendingLiveRideCreation, StorageError> {
        let candidate_vehicle =
            normalize_optional_stored_text(candidate_vehicle, "candidate vehicle")?;
        let (reply, response) = response_channel();
        self.enqueue(Command::CreateStartedLiveRide {
            created_at_ms,
            monotonic_created_at_ms,
            candidate_vehicle,
            music_history_policy,
            reply,
        })?;
        Ok(PendingLiveRideCreation {
            response,
            consumed: false,
        })
    }

    /// Queues a barrier behind all work already submitted to the database worker.
    ///
    /// When the returned handle completes, every preceding queued write has committed or
    /// reported its failure. Commands submitted later are not included in the barrier.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::QueueFull`] when the bounded queue is saturated or
    /// [`StorageError::WorkerStopped`] when the worker is unavailable.
    pub fn queue_ride_checkpoint(&self) -> Result<PendingRideCheckpoint, StorageError> {
        let (reply, response) = response_channel();
        self.enqueue(Command::RideCheckpoint { reply })?;
        Ok(PendingRideCheckpoint {
            response,
            consumed: false,
        })
    }

    /// Waits for capacity and for every preceding worker command to finish.
    ///
    /// Call this from a background recording executor when queue saturation must apply
    /// backpressure rather than discard the rest of a native location callback.
    ///
    /// # Errors
    ///
    /// Returns a worker or storage error when the barrier cannot complete.
    pub fn wait_for_ride_checkpoint(&self) -> Result<(), StorageError> {
        self.request_blocking(|reply| Command::RideCheckpoint { reply })
    }

    /// Waits for preceding worker writes and commits clock-only location evidence.
    ///
    /// The last durable point is revalidated before the single ride-row update.
    /// Call this on the background recording executor.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker or database cannot commit the checkpoint.
    pub fn checkpoint_location_observation(
        &self,
        checkpoint: RideLocationObservationCheckpoint,
    ) -> Result<RideLocationObservationCheckpointOutcome, StorageError> {
        self.request_blocking(move |reply| Command::CheckpointLocationObservation {
            checkpoint,
            reply,
        })
    }

    /// Atomically settles one recovered ride and optionally creates its replacement.
    ///
    /// The old ride and replacement share one SQLite transaction. A failed replacement therefore
    /// rolls back finalization instead of leaving recovery with a saved ride and no new session.
    /// Retrying after a committed transaction observes the replacement as the next recoverable
    /// ride after the next database acquisition, so recovery cannot duplicate it.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when either ride finalization or replacement creation cannot be
    /// validated or committed by the storage worker.
    pub fn settle_recovered_ride(
        &self,
        ride_id: RideId,
        discard_empty: bool,
        occurred_at_ms: u64,
        monotonic_at_ms: u64,
        replacement_candidate_vehicle: Option<&str>,
    ) -> Result<Option<RideId>, StorageError> {
        self.settle_recovered_ride_with_checkpoint(
            ride_id,
            discard_empty,
            occurred_at_ms,
            monotonic_at_ms,
            replacement_candidate_vehicle,
        )
        .map(|result| {
            result
                .filter(|checkpoint| checkpoint.lifecycle() == RideLifecycleState::Active)
                .map(CommittedRideMovingTimeCheckpoint::ride_id)
        })
    }

    /// Settles recovery and returns the final ride checkpoint from the same transaction.
    ///
    /// A replacement ride is returned when one was created; otherwise a continued or saved ride
    /// is returned when its checkpoint was published. Empty rides discarded during recovery have
    /// no current ride result.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if recovery state is invalid or the worker cannot commit it.
    pub fn settle_recovered_ride_with_checkpoint(
        &self,
        ride_id: RideId,
        discard_empty: bool,
        occurred_at_ms: u64,
        monotonic_at_ms: u64,
        replacement_candidate_vehicle: Option<&str>,
    ) -> Result<Option<CommittedRideMovingTimeCheckpoint>, StorageError> {
        let replacement_candidate_vehicle = normalize_optional_stored_text(
            replacement_candidate_vehicle,
            "replacement candidate vehicle",
        )?;
        self.request_blocking(move |reply| Command::SettleRecoveredRide {
            ride_id,
            discard_empty,
            occurred_at_ms,
            monotonic_at_ms,
            replacement_candidate_vehicle,
            reply,
        })
    }

    /// Settles an expired inactive live ride only when canonical wheel motion is parked.
    ///
    /// The evidence comparison, expiry policy, parked-motion check, Active check, and settlement
    /// share one SQLite transaction. The caller supplies the last accepted material route
    /// timestamps after draining location writes; GPS observations are not inactivity evidence.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::NotFound`] for an unknown ride,
    /// [`StorageError::RideInactivityConflict`] when the state or expected evidence changed, or a
    /// storage error when settlement cannot be validated or committed.
    pub fn settle_inactive_live_ride_with_checkpoint(
        &self,
        ride_id: RideId,
        expected_evidence: RideInactivityEvidence,
        occurred_at_ms: u64,
        monotonic_at_ms: u64,
        replacement_candidate_vehicle: Option<&str>,
    ) -> Result<Option<CommittedRideMovingTimeCheckpoint>, StorageError> {
        let replacement_candidate_vehicle = normalize_optional_stored_text(
            replacement_candidate_vehicle,
            "replacement candidate vehicle",
        )?;
        self.request_blocking(move |reply| Command::SettleInactiveLiveRide {
            ride_id,
            expected_evidence,
            occurred_at_ms,
            monotonic_at_ms,
            replacement_candidate_vehicle,
            reply,
        })
    }

    /// Replaces the Rust-owned candidate, association, and telemetry metadata for one ride.
    ///
    /// Every argument is written as given. A `None` argument clears that stored column, so
    /// callers must provide a complete metadata snapshot rather than a partial update.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the ride is missing, metadata is invalid, or the worker
    /// cannot commit the update.
    pub fn update_ride_map_metadata(
        &self,
        ride_id: RideId,
        candidate_vehicle: Option<&str>,
        associated_vehicle: Option<&str>,
        associated_at_ms: Option<u64>,
        last_telemetry_at_ms: Option<u64>,
    ) -> Result<(), StorageError> {
        self.update_ride_map_metadata_with_motion(
            ride_id,
            candidate_vehicle,
            associated_vehicle,
            associated_at_ms,
            last_telemetry_at_ms,
            None,
        )
        .map(|_| ())
    }

    /// Updates complete association metadata and optional wheel-motion evidence atomically.
    /// A missing motion observation preserves the prior durable observation.
    ///
    /// # Errors
    /// Returns a storage error for missing rides or invalid/out-of-order evidence.
    pub fn update_ride_map_metadata_with_motion(
        &self,
        ride_id: RideId,
        candidate_vehicle: Option<&str>,
        associated_vehicle: Option<&str>,
        associated_at_ms: Option<u64>,
        last_telemetry_at_ms: Option<u64>,
        motion: Option<RideMotionObservation>,
    ) -> Result<MovingTimeCheckpoint, StorageError> {
        self.update_ride_map_metadata_with_moving_time(
            ride_id,
            candidate_vehicle,
            associated_vehicle,
            associated_at_ms,
            last_telemetry_at_ms,
            motion,
            None,
        )
    }

    /// Updates metadata and verified-device inactivity evidence in the same transaction.
    ///
    /// Call this only when an already accepted metadata/telemetry checkpoint is being written;
    /// it does not create an extra write for each device packet.
    ///
    /// # Errors
    /// Returns a storage error when metadata is invalid, the ride is missing, or the worker
    /// cannot commit the checkpoint and inactivity evidence together.
    #[allow(
        clippy::too_many_arguments,
        reason = "the metadata boundary keeps the complete checkpoint and evidence snapshot explicit"
    )]
    pub fn update_ride_map_metadata_with_inactivity_evidence(
        &self,
        ride_id: RideId,
        candidate_vehicle: Option<&str>,
        associated_vehicle: Option<&str>,
        associated_at_ms: Option<u64>,
        last_telemetry_at_ms: Option<u64>,
        motion: Option<RideMotionObservation>,
        moving_time_observation: Option<MovingTimeObservation>,
        inactivity_evidence: RideInactivityEvidence,
    ) -> Result<MovingTimeCheckpoint, StorageError> {
        let candidate_vehicle =
            normalize_optional_stored_text(candidate_vehicle, "candidate vehicle")?;
        let associated_vehicle =
            normalize_optional_stored_text(associated_vehicle, "associated vehicle")?;
        self.request(move |reply| Command::UpdateRideMapMetadata {
            ride_id,
            candidate_vehicle,
            associated_vehicle,
            associated_at_ms,
            last_telemetry_at_ms,
            motion,
            moving_time_observation,
            inactivity_evidence: Some(inactivity_evidence),
            reply,
        })
    }

    /// Updates complete metadata and persists an optional verified-wheel speed observation in
    /// the same material write.
    ///
    /// # Errors
    /// Returns a storage error when the ride is missing, evidence is invalid, or SQLite cannot
    /// commit the metadata and moving-time checkpoint together.
    #[allow(
        clippy::too_many_arguments,
        reason = "the storage boundary keeps the complete metadata transaction payload explicit"
    )]
    pub fn update_ride_map_metadata_with_moving_time(
        &self,
        ride_id: RideId,
        candidate_vehicle: Option<&str>,
        associated_vehicle: Option<&str>,
        associated_at_ms: Option<u64>,
        last_telemetry_at_ms: Option<u64>,
        motion: Option<RideMotionObservation>,
        moving_time_observation: Option<MovingTimeObservation>,
    ) -> Result<MovingTimeCheckpoint, StorageError> {
        let candidate_vehicle =
            normalize_optional_stored_text(candidate_vehicle, "candidate vehicle")?;
        let associated_vehicle =
            normalize_optional_stored_text(associated_vehicle, "associated vehicle")?;
        self.request(move |reply| Command::UpdateRideMapMetadata {
            ride_id,
            candidate_vehicle,
            associated_vehicle,
            associated_at_ms,
            last_telemetry_at_ms,
            motion,
            moving_time_observation,
            inactivity_evidence: None,
            reply,
        })
    }

    /// Checkpoints stationary GPS clocks and coordinates without adding route geometry.
    ///
    /// # Errors
    /// Returns a storage error if the transaction cannot be committed.
    pub fn checkpoint_stationary_location_observation(
        &self,
        checkpoint: RideStationaryLocationCheckpoint,
    ) -> Result<RideLocationObservationCheckpointOutcome, StorageError> {
        self.request_blocking(
            move |reply| Command::CheckpointStationaryLocationObservation { checkpoint, reply },
        )
    }

    /// Stores a batch of raw BMS voltage samples independently of ride lifecycle.
    ///
    /// Replaying a previously submitted batch is idempotent by observation-event identity.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when a timestamp cannot be represented by SQLite or the worker
    /// cannot commit the batch.
    pub fn record_bms_voltage_samples(
        &self,
        samples: &[BmsVoltageSampleRecord],
    ) -> Result<(), StorageError> {
        self.queue_bms_voltage_samples(samples.to_vec())?
            .wait_result()
    }

    /// Queues a BMS batch without waiting for the SQLite worker.
    ///
    /// The bounded queue applies backpressure at submission time. The returned ticket can be
    /// polled for the durable result.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::QueueFull`] when the bounded queue is saturated, or
    /// [`StorageError::WorkerStopped`] when the worker is unavailable.
    pub fn queue_bms_voltage_samples(
        &self,
        samples: Vec<BmsVoltageSampleRecord>,
    ) -> Result<PendingBmsVoltageWrite, StorageError> {
        let (reply, response) = response_channel();
        self.enqueue(Command::RecordBmsVoltageSamples { samples, reply })?;
        Ok(PendingBmsVoltageWrite {
            response,
            consumed: false,
        })
    }

    /// Queues verified-device history, policy, lifecycle, and metadata as one worker transaction.
    ///
    /// The command is ordered with every other operation on the existing bounded database
    /// worker. It always remembers the verified identity and applies ride metadata; the automatic
    /// lifecycle plan and its metadata run only when the identity matches the selected device;
    /// otherwise the alternate metadata snapshot is applied. The method returns after bounded
    /// enqueue and does not wait for SQLite completion.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::QueueFull`] when the bounded queue is saturated,
    /// [`StorageError::WorkerStopped`] when the worker is unavailable, or a validation error for
    /// an invalid platform identifier.
    pub fn queue_verified_connection_admission(
        &self,
        platform_identifier: &str,
        updated_at_ms: u64,
        mut lifecycle_mutations: Vec<VerifiedConnectionLifecycleMutation>,
        metadata: Option<VerifiedConnectionRideMetadata>,
        unselected_metadata: Option<VerifiedConnectionRideMetadata>,
    ) -> Result<PendingVerifiedConnectionAdmission, StorageError> {
        if lifecycle_mutations.len() > MAX_VERIFIED_CONNECTION_LIFECYCLE_MUTATIONS {
            return Err(StorageError::TooManyConnectionAdmissionMutations {
                actual: lifecycle_mutations.len(),
                maximum: MAX_VERIFIED_CONNECTION_LIFECYCLE_MUTATIONS,
            });
        }
        let start_count = lifecycle_mutations
            .iter()
            .filter(|mutation| match mutation {
                VerifiedConnectionLifecycleMutation::Transition { .. } => false,
                VerifiedConnectionLifecycleMutation::StartLive { .. } => true,
            })
            .count();
        let metadata_targets_created_ride = |metadata: &Option<VerifiedConnectionRideMetadata>| {
            metadata
                .as_ref()
                .is_some_and(|metadata| match metadata.target {
                    VerifiedConnectionRideTarget::Existing(_) => false,
                    VerifiedConnectionRideTarget::Created => true,
                })
        };
        if start_count > 1
            || ((metadata_targets_created_ride(&metadata)
                || metadata_targets_created_ride(&unselected_metadata))
                && start_count != 1)
        {
            return Err(StorageError::InvalidConnectionAdmissionPlan);
        }
        let platform_identifier =
            normalize_stored_text(platform_identifier, "platform identifier")?;
        for mutation in &mut lifecycle_mutations {
            if let VerifiedConnectionLifecycleMutation::StartLive {
                candidate_vehicle, ..
            } = mutation
            {
                *candidate_vehicle = normalize_optional_stored_text(
                    candidate_vehicle.as_deref(),
                    "candidate vehicle",
                )?;
            }
        }
        let normalize_metadata = |metadata: VerifiedConnectionRideMetadata|
         -> Result<VerifiedConnectionRideMetadata, StorageError> {
            Ok(VerifiedConnectionRideMetadata {
                target: metadata.target,
                candidate_vehicle: normalize_optional_stored_text(
                    metadata.candidate_vehicle.as_deref(),
                    "candidate vehicle",
                )?,
                associated_vehicle: normalize_optional_stored_text(
                    metadata.associated_vehicle.as_deref(),
                    "associated vehicle",
                )?,
                associated_at_ms: metadata.associated_at_ms,
                last_telemetry_at_ms: metadata.last_telemetry_at_ms,
                moving_time_source_boundary_at_ms: metadata.moving_time_source_boundary_at_ms,
            })
        };
        let metadata = metadata.map(normalize_metadata).transpose()?;
        let unselected_metadata = unselected_metadata.map(normalize_metadata).transpose()?;
        let (reply, response) = response_channel();
        self.enqueue(Command::VerifiedConnectionAdmission {
            platform_identifier,
            updated_at_ms,
            lifecycle_mutations,
            metadata,
            unselected_metadata,
            reply,
        })?;
        Ok(PendingVerifiedConnectionAdmission {
            response,
            consumed: false,
        })
    }

    /// Stores the selected platform-local device identifier.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the identifier is empty or the worker cannot commit it.
    pub fn save_selected_device(
        &self,
        platform_identifier: &str,
        updated_at_ms: u64,
    ) -> Result<(), StorageError> {
        let platform_identifier =
            normalize_stored_text(platform_identifier, "platform identifier")?;
        self.request(move |reply| Command::SaveSelectedDevice {
            platform_identifier,
            updated_at_ms,
            reply,
        })
    }

    /// Stores a device name and selects the same device in one database transaction.
    ///
    /// A blank or identifier-equal display name is ignored, preserving any existing name.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when an input is invalid or the worker cannot commit the update.
    pub fn remember_selected_device(
        &self,
        platform_identifier: &str,
        display_name: Option<&str>,
        updated_at_ms: u64,
    ) -> Result<(), StorageError> {
        let platform_identifier =
            normalize_stored_text(platform_identifier, "platform identifier")?;
        let display_name = normalize_optional_device_name(display_name, &platform_identifier)?;
        self.request(move |reply| Command::RememberSelectedDevice {
            platform_identifier,
            display_name,
            updated_at_ms,
            reply,
        })
    }

    /// Loads the selected platform-local device identifier.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot query the value.
    pub fn selected_device(&self) -> Result<Option<String>, StorageError> {
        self.request(|reply| Command::SelectedDevice { reply })
    }

    /// Clears the selected platform-local device identifier.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot commit the deletion.
    pub fn clear_selected_device(&self) -> Result<(), StorageError> {
        self.request(|reply| Command::ClearSelectedDevice { reply })
    }

    /// Stores the last verified platform-local device identity independently of user selection.
    ///
    /// This record is Rust-owned connection history. It is deliberately not changed by clearing
    /// the user's selected device, because a later explicit GPS-only start may still use the
    /// last verified connection as an association candidate.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the identity is empty or the worker cannot commit it.
    pub fn remember_last_connected_device(
        &self,
        platform_identifier: &str,
        updated_at_ms: u64,
    ) -> Result<(), StorageError> {
        let platform_identifier =
            normalize_stored_text(platform_identifier, "platform identifier")?;
        self.request(move |reply| Command::RememberLastConnectedDevice {
            platform_identifier,
            updated_at_ms,
            reply,
        })
    }

    /// Loads the last verified platform-local device identity.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot query the value.
    pub fn last_connected_device(&self) -> Result<Option<String>, StorageError> {
        self.request(|reply| Command::LastConnectedDevice { reply })
    }

    /// Clears the Rust-owned last-connected-device record.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot commit the deletion.
    pub fn clear_last_connected_device(&self) -> Result<(), StorageError> {
        self.request(|reply| Command::ClearLastConnectedDevice { reply })
    }

    /// Loads ride autostart, enabled by default for new and migrated databases.
    ///
    /// # Errors
    /// Returns [`StorageError`] when the worker cannot read the preference.
    pub fn ride_autostart_enabled(&self) -> Result<bool, StorageError> {
        self.request(|reply| Command::RideAutostartEnabled { reply })
    }

    /// Saves whether future eligible connections may start a ride.
    ///
    /// # Errors
    /// Returns [`StorageError`] when the worker cannot commit the preference.
    pub fn save_ride_autostart_enabled(&self, enabled: bool) -> Result<(), StorageError> {
        self.request(|reply| Command::SaveRideAutostartEnabled { enabled, reply })
    }

    /// Reads optional automatic diagnostic recording consent; absent consent is disabled.
    ///
    /// # Errors
    /// Returns a storage error when the preference cannot be read.
    pub fn automatic_diagnostic_recording_enabled(&self) -> Result<bool, StorageError> {
        if !self.diagnostic_recording_available.load(Ordering::Acquire)
            || !self.worker_alive.load(Ordering::Acquire)
        {
            return Err(StorageError::WorkerStopped);
        }
        Ok(self.diagnostic_recording_enabled.load(Ordering::Acquire))
    }

    /// Persists an explicit choice without changing ride recording or manual capture consent.
    ///
    /// # Errors
    /// Returns a storage error when the preference cannot be committed.
    pub fn save_automatic_diagnostic_recording_enabled(
        &self,
        enabled: bool,
    ) -> Result<(), StorageError> {
        self.request(|reply| Command::SaveAutomaticDiagnosticRecordingEnabled { enabled, reply })
    }

    /// Stores phone-generated alarm preferences for one device.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the identity or preferences are invalid, or the worker cannot
    /// commit the update.
    pub fn save_phone_alarm_preferences(
        &self,
        device_identity: &str,
        preferences: PhoneAlarmPreferencesRecord,
    ) -> Result<(), StorageError> {
        let device_identity = normalize_stored_text(device_identity, "device identity")?;
        self.request(move |reply| Command::SavePhoneAlarmPreferences {
            device_identity,
            preferences,
            reply,
        })
    }

    /// Loads phone-generated alarm preferences for one device.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the identity is invalid or the worker cannot query it.
    pub fn phone_alarm_preferences(
        &self,
        device_identity: &str,
    ) -> Result<Option<PhoneAlarmPreferencesRecord>, StorageError> {
        let device_identity = normalize_stored_text(device_identity, "device identity")?;
        self.request(move |reply| Command::PhoneAlarmPreferences {
            device_identity,
            reply,
        })
    }

    /// Loads the saved phone-drive mode and limits for one stable device identity.
    ///
    /// A device with no explicit saved settings receives the disabled default. Legacy global
    /// settings are retained separately and are never attributed to a device.
    ///
    /// # Errors
    /// Returns [`StorageError`] when the identity is invalid or the stored values are corrupt.
    pub fn phone_drive_preferences(
        &self,
        device_identity: &str,
    ) -> Result<PhoneDriveSettingsRecord, StorageError> {
        let device_identity = normalize_stored_text(device_identity, "device identity")?;
        self.request(|reply| {
            Command::PhoneControl(phone_control::Command::LoadForDevice {
                device_identity,
                reply,
            })
        })
    }

    /// Atomically stores one device's mode and current limits.
    ///
    /// # Errors
    /// Returns [`StorageError`] when the identity is invalid or the database cannot commit.
    pub fn save_phone_drive_preferences(
        &self,
        device_identity: &str,
        settings: PhoneDriveSettingsRecord,
    ) -> Result<(), StorageError> {
        let device_identity = normalize_stored_text(device_identity, "device identity")?;
        self.request(|reply| {
            Command::PhoneControl(phone_control::Command::SaveForDevice {
                device_identity,
                settings,
                reply,
            })
        })
    }

    /// Stores the opt-in music-history policy for one ride.
    ///
    /// Disabling history also removes any previously stored music events.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::NotFound`] when the ride does not exist or a storage error when
    /// the worker cannot commit the policy.
    pub fn save_music_history_policy(
        &self,
        ride_id: RideId,
        policy: MusicHistoryPolicy,
    ) -> Result<(), StorageError> {
        self.request(move |reply| Command::SaveMusicHistoryPolicy {
            ride_id,
            policy,
            reply,
        })
    }

    /// Queues a bounded durable history-policy update without waiting for SQLite.
    ///
    /// # Errors
    /// Returns [`StorageError::QueueFull`] when the worker queue is saturated, or
    /// [`StorageError::WorkerStopped`] when the worker is unavailable.
    pub fn queue_save_music_history_policy(
        &self,
        ride_id: RideId,
        policy: MusicHistoryPolicy,
    ) -> Result<PendingMusicHistoryWrite, StorageError> {
        let (reply, response) = response_channel();
        self.enqueue(Command::SaveMusicHistoryPolicy {
            ride_id,
            policy,
            reply,
        })?;
        Ok(PendingMusicHistoryWrite {
            response,
            consumed: false,
        })
    }

    /// Queues a bounded music-history deletion without waiting for SQLite.
    ///
    /// # Errors
    /// Returns [`StorageError::QueueFull`] when the worker queue is saturated, or
    /// [`StorageError::WorkerStopped`] when the worker is unavailable.
    pub fn queue_delete_music_history(
        &self,
        ride_id: RideId,
    ) -> Result<PendingMusicHistoryWrite, StorageError> {
        let (reply, response) = response_channel();
        self.enqueue(Command::DeleteMusicHistory { ride_id, reply })?;
        Ok(PendingMusicHistoryWrite {
            response,
            consumed: false,
        })
    }

    /// Atomically admits a live transition using durable privacy, sequence, and observation state.
    ///
    /// # Errors
    /// Returns a storage error when the ride or history is invalid or the commit fails.
    pub fn record_music_event(
        &self,
        ride_id: RideId,
        event: MusicRideEvent,
    ) -> Result<MusicTimelineOutcome, StorageError> {
        self.record_music_event_with_sequence(ride_id, event)
            .map(|result| result.outcome)
    }

    /// Atomically admits a live transition and returns its durable ride-local sequence.
    ///
    /// The sequence is present only when a new event was appended; duplicate, out-of-order,
    /// disabled, and full outcomes do not allocate one.
    ///
    /// # Errors
    /// Returns a storage error when the ride or history is invalid or the commit fails.
    pub fn record_music_event_with_sequence(
        &self,
        ride_id: RideId,
        event: MusicRideEvent,
    ) -> Result<MusicTimelineRecordResult, StorageError> {
        self.request(move |reply| Command::RecordMusicEvent {
            ride_id,
            event,
            reply,
        })
    }

    /// Queues one live music transition without waiting for the SQLite worker.
    ///
    /// The worker reads the current durable history policy and applies its privacy rules before
    /// committing the event. This bounded queue is the live provider-callback path.
    ///
    /// # Errors
    /// Returns [`StorageError::QueueFull`] when the worker queue is saturated, or
    /// [`StorageError::WorkerStopped`] when the worker is unavailable.
    pub fn queue_record_music_event(
        &self,
        ride_id: RideId,
        event: MusicRideEvent,
    ) -> Result<PendingMusicEventWrite, StorageError> {
        let (reply, response) = response_channel();
        self.enqueue(Command::RecordMusicEvent {
            ride_id,
            event,
            reply,
        })?;
        Ok(PendingMusicEventWrite {
            response,
            consumed: false,
        })
    }

    /// Queues one music-history query without waiting for the SQLite worker.
    ///
    /// # Errors
    /// Returns [`StorageError::QueueFull`] when the worker queue is saturated, or
    /// [`StorageError::WorkerStopped`] when the worker is unavailable.
    pub fn queue_music_history(
        &self,
        ride_id: RideId,
    ) -> Result<PendingMusicHistoryRead, StorageError> {
        let (reply, response) = response_channel();
        self.enqueue(Command::MusicHistory { ride_id, reply })?;
        Ok(PendingMusicHistoryRead {
            response,
            consumed: false,
        })
    }

    /// Loads history status and events together through the canonical worker.
    ///
    /// # Errors
    /// Returns a storage error when history is unavailable or invalid.
    pub fn music_history(&self, ride_id: RideId) -> Result<MusicHistory, StorageError> {
        self.request(move |reply| Command::MusicHistory { ride_id, reply })
    }

    /// Persists one already privacy-filtered music transition for import or restore.
    ///
    /// This is the privileged raw writer: unlike [`Self::record_music_event`], an explicit
    /// policy may create a missing retention row. Provider callbacks should use the live writer
    /// so lifecycle and opt-in admission remain authoritative. Events are appended in contiguous
    /// sequence order and bounded by the portable music timeline capacity. A repeated sequence
    /// is idempotent.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::NotFound`], [`StorageError::MusicTimelineFull`], or a typed
    /// storage error when the worker cannot commit the event.
    #[allow(
        dead_code,
        reason = "crate-local import contract is covered by persistence tests"
    )]
    pub(crate) fn save_music_event(
        &self,
        ride_id: RideId,
        policy: MusicHistoryPolicy,
        sequence: u64,
        event: MusicRideEvent,
    ) -> Result<(), StorageError> {
        let sequence = i64::try_from(sequence).map_err(|_| StorageError::InvalidStoredValue {
            field: "music sequence",
            value: sequence.to_string(),
        })?;
        self.request(move |reply| Command::SaveMusicEvent {
            ride_id,
            policy,
            sequence,
            event,
            reply,
        })
    }

    /// Deletes all music metadata for one ride while preserving the ride itself.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::NotFound`] when the ride does not exist or a storage error when
    /// the worker cannot commit the deletion.
    pub fn delete_music_history(&self, ride_id: RideId) -> Result<(), StorageError> {
        self.request(move |reply| Command::DeleteMusicHistory { ride_id, reply })
    }

    /// Removes Spotify listening events without deleting rides or other providers' history.
    ///
    /// # Errors
    /// Returns a storage error when the worker cannot commit the cleanup.
    pub fn delete_spotify_music_history(&self) -> Result<(), StorageError> {
        self.request(|reply| Command::DeleteSpotifyMusicHistory { reply })
    }

    /// Loads the bounded music timeline for one ride in sequence order.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::NotFound`] when the ride does not exist or a typed storage error
    /// when persisted music metadata cannot be decoded.
    #[allow(
        dead_code,
        reason = "crate-local raw projection is covered by persistence tests"
    )]
    pub(crate) fn music_events(
        &self,
        ride_id: RideId,
    ) -> Result<Vec<MusicRideEvent>, StorageError> {
        self.request(move |reply| Command::MusicEvents { ride_id, reply })
    }

    /// Loads the persisted music-history policy for one ride.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::NotFound`] when the ride does not exist or a storage error when
    /// the persisted policy cannot be decoded.
    pub fn music_history_policy(
        &self,
        ride_id: RideId,
    ) -> Result<MusicHistoryPolicy, StorageError> {
        self.request(move |reply| Command::MusicHistoryPolicy { ride_id, reply })
    }

    /// Loads the durable music-history state for one ride.
    ///
    /// A ride without a history row is [`MusicHistoryState::Missing`]. Explicit
    /// deletion is represented by [`MusicHistoryState::Deleted`].
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::NotFound`] when the ride does not exist or a storage error when
    /// the persisted state cannot be decoded.
    pub fn music_history_state(&self, ride_id: RideId) -> Result<MusicHistoryState, StorageError> {
        self.request(move |reply| Command::MusicHistoryState { ride_id, reply })
    }
    /// Stores the display name associated with a platform-local device identifier.
    ///
    /// # Errors
    ///
    /// Returns a storage error when the identity or display name is empty or the worker cannot
    /// commit the device record.
    pub fn save_device_name(
        &self,
        platform_identifier: &str,
        display_name: &str,
        updated_at_ms: u64,
    ) -> Result<(), StorageError> {
        let platform_identifier =
            normalize_stored_text(platform_identifier, "platform identifier")?;
        let display_name = normalize_stored_text(display_name, "display name")?;
        self.request(move |reply| Command::SaveDeviceName {
            platform_identifier,
            display_name,
            updated_at_ms,
            reply,
        })
    }

    /// Migrates one legacy display name and returns its canonical value.
    ///
    /// Blank or identifier-equal values are treated as absent. The operation is idempotent.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when an input is invalid or the worker cannot commit the update.
    pub fn migrate_device_name(
        &self,
        platform_identifier: &str,
        display_name: &str,
        updated_at_ms: u64,
    ) -> Result<Option<String>, StorageError> {
        let platform_identifier =
            normalize_stored_text(platform_identifier, "platform identifier")?;
        let display_name =
            normalize_optional_device_name(Some(display_name), &platform_identifier)?;
        let Some(display_name) = display_name else {
            return Ok(None);
        };
        self.request(move |reply| Command::MigrateDeviceName {
            platform_identifier,
            display_name,
            updated_at_ms,
            reply,
        })
    }

    /// Loads a persisted display name for a platform-local device identifier.
    ///
    /// # Errors
    ///
    /// Returns a storage error when the worker cannot query the device record.
    pub fn device_name(&self, platform_identifier: &str) -> Result<Option<String>, StorageError> {
        let platform_identifier = platform_identifier.trim().to_owned();
        self.request(move |reply| Command::DeviceName {
            platform_identifier,
            reply,
        })
    }

    /// Stores a learned voltage-sag model for one device identity.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the identity is empty or the worker cannot commit it.
    pub fn save_voltage_sag_model(
        &self,
        device_identity: &str,
        model: VoltageSagModelRecord,
    ) -> Result<(), StorageError> {
        if device_identity.trim().is_empty() {
            return Err(StorageError::InvalidStoredValue {
                field: "device identity",
                value: "empty".to_owned(),
            });
        }
        self.request(move |reply| Command::SaveVoltageSagModel {
            device_identity: device_identity.to_owned(),
            model,
            reply,
        })
    }

    /// Loads a learned voltage-sag model for one device identity.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot query the value.
    pub fn voltage_sag_model(
        &self,
        device_identity: &str,
    ) -> Result<Option<VoltageSagModelRecord>, StorageError> {
        self.request(move |reply| Command::VoltageSagModel {
            device_identity: device_identity.to_owned(),
            reply,
        })
    }

    /// Removes a learned voltage-sag model.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the identity is empty or the worker cannot commit the deletion.
    pub fn remove_voltage_sag_model(&self, device_identity: &str) -> Result<(), StorageError> {
        if device_identity.trim().is_empty() {
            return Err(StorageError::InvalidStoredValue {
                field: "device identity",
                value: "empty".to_owned(),
            });
        }
        self.request(move |reply| Command::RemoveVoltageSagModel {
            device_identity: device_identity.to_owned(),
            reply,
        })
    }

    /// Stores opaque Rust-owned ride-session marker bytes.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot commit the marker.
    pub fn save_ride_session_marker(&self, marker: &[u8]) -> Result<(), StorageError> {
        self.request(move |reply| Command::SaveRideSessionMarker {
            marker: marker.to_vec(),
            reply,
        })
    }

    /// Queues opaque marker replacement or removal without waiting for worker capacity.
    ///
    /// # Errors
    /// Returns [`StorageError::QueueFull`] or [`StorageError::WorkerStopped`] on rejection.
    pub fn queue_ride_session_marker(
        &self,
        marker: Option<Vec<u8>>,
    ) -> Result<PendingRideSessionMarkerWrite, StorageError> {
        let (reply, response) = response_channel();
        let command = match marker {
            Some(marker) => Command::SaveRideSessionMarker { marker, reply },
            None => Command::ClearRideSessionMarker { reply },
        };
        self.maintenance_demand.fetch_add(1, Ordering::AcqRel);
        self.sender.try_send(command).map_err(|error| match error {
            mpsc::TrySendError::Full(_) => StorageError::QueueFull,
            mpsc::TrySendError::Disconnected(_) => StorageError::WorkerStopped,
        })?;
        self.maintenance_demand.fetch_add(1, Ordering::AcqRel);
        Ok(PendingRideSessionMarkerWrite {
            response,
            consumed: false,
        })
    }

    pub(crate) fn recover_marker_worker(&self) -> Result<Self, StorageError> {
        if self.worker_has_exited() && self.worker_can_restart() {
            self.reopen()
        } else {
            service::current_marker_handle(self.service_id)
        }
    }

    /// Loads opaque Rust-owned ride-session marker bytes.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot query the marker.
    pub fn ride_session_marker(&self) -> Result<Option<Vec<u8>>, StorageError> {
        self.request(|reply| Command::RideSessionMarker { reply })
    }

    /// Clears opaque Rust-owned ride-session marker bytes.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot commit the deletion.
    pub fn clear_ride_session_marker(&self) -> Result<(), StorageError> {
        self.request(|reply| Command::ClearRideSessionMarker { reply })
    }

    /// Validates a PEVCAP artifact and returns the bounded facts a user must confirm.
    /// No capture or ride is published. An existing receipt changes only the proposed
    /// outcome to [`PevcapImportOutcome::AlreadyImported`]; warnings remain available.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the artifact cannot be hashed, replayed, decoded, or bounded.
    pub fn preflight_pevcap(
        &self,
        path: &Path,
        encoding: PevcapEncoding,
    ) -> Result<PevcapImportPreview, StorageError> {
        let mut preview = preflight_pevcap(path, encoding, Some(PevcapLimits::IMPORT))?;
        if self
            .request(|reply| Command::PevcapImportLookup {
                digest: preview.artifact_digest.clone(),
                reply,
            })?
            .is_some()
        {
            preview.outcome = PevcapImportOutcome::AlreadyImported;
        }
        Ok(preview)
    }

    #[cfg(test)]
    pub(crate) fn fail_next_pevcap_finish_response_for_test() {
        worker::drop_next_pevcap_finish_response_for_test();
    }

    /// Confirms a reviewed PEVCAP preview using bounded byte and location batches.
    /// Original bytes and the receipt are retained in SQLite, and any derived ride becomes
    /// visible with them; GPS-free captures do not create an empty ride. Source files are staged
    /// temporarily during import and are not retained as a second permanent copy.
    ///
    /// Reconfirmation preserves the receipt and ride identity and can backfill older
    /// file-only receipts into SQLite from the supplied source.
    /// `created_at_ms` is Unix milliseconds used for a new import, not its capture timestamp.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the source changed after preflight or the import cannot be
    /// committed atomically.
    pub fn confirm_pevcap_import(
        &self,
        preview: &PevcapImportPreview,
        created_at_ms: u64,
    ) -> Result<PevcapImportReceipt, StorageError> {
        self.confirm_pevcap_publication(preview, created_at_ms, &CapturePublication::Import)
    }

    fn confirm_pevcap_publication(
        &self,
        preview: &PevcapImportPreview,
        created_at_ms: u64,
        publication: &CapturePublication,
    ) -> Result<PevcapImportReceipt, StorageError> {
        verify_pevcap_preview_source(preview)?;
        if let Some(receipt) = self.request(|reply| Command::PevcapImportLookup {
            digest: preview.artifact_digest.clone(),
            reply,
        })? {
            return self.confirm_existing_pevcap(preview, receipt, publication);
        }
        let outcome = match publication {
            CapturePublication::Recording(_) | CapturePublication::SubmissionSnapshot => {
                NewPevcapImportOutcome::CaptureOnly
            }
            CapturePublication::Import => NewPevcapImportOutcome::try_from(preview.outcome)?,
        };

        let staged_path = pevcap_staging_path(self.path.as_ref(), preview.artifact_digest());
        #[cfg(test)]
        pevcap_confirmation_hooks::run(&pevcap_confirmation_hooks::BEFORE_BEGIN);
        // Reserve the digest before publishing bytes. A rejected competing importer must never
        // own cleanup of the artifact used by the admitted importer.
        let begin = self.request(|reply| Command::BeginPevcapImport {
            digest: preview.artifact_digest.clone(),
            managed_path: staged_path.clone(),
            outcome,
            created_at_ms,
            reply,
        })?;
        let PevcapBegin::Started { ride_id } = begin else {
            return match begin {
                PevcapBegin::Duplicate(receipt) => {
                    self.confirm_existing_pevcap(preview, receipt, publication)
                }
                PevcapBegin::Started { .. } => unreachable!(),
            };
        };

        let result = (|| {
            let mut staged = stage_pevcap_source(self.path.as_ref(), preview, &staged_path)?;
            #[cfg(test)]
            pevcap_confirmation_hooks::run(&pevcap_confirmation_hooks::BEFORE_STORE);
            capture_data::store(self, preview, staged.path()?)?;
            let location_count = if let Some(ride_id) = ride_id {
                stream_pevcap_location_batches(staged.path()?, preview.encoding(), |samples| {
                    self.request(|reply| Command::AppendPevcapLocationBatch {
                        ride_id,
                        samples,
                        reply,
                    })
                })?
            } else {
                0
            };
            // The work row still owns this path if the process stops before Finish commits.
            staged.remove()?;
            self.request(|reply| Command::FinishPevcapImport {
                completion: PevcapImportCompletion {
                    digest: preview.artifact_digest.clone(),
                    ride_id,
                    managed_path: staged_path.clone(),
                    outcome,
                    artifact_size: preview.artifact_size,
                    record_count: preview.record_count,
                    location_count,
                    duration_milliseconds: preview.duration_milliseconds,
                    imported_at_ms: created_at_ms,
                    publication: publication.clone(),
                },
                reply,
            })
        })();
        if let Err(StorageError::ResponseDropped) = &result {
            // The finish transaction may have committed before its reply was lost. Reconcile the
            // durable SQLite receipt before aborting the reservation, because the transaction may
            // already own the canonical bytes and receipt.
            match self.request(|reply| Command::PevcapImportLookup {
                digest: preview.artifact_digest.clone(),
                reply,
            }) {
                Ok(Some(receipt)) => {
                    publication.validate_duplicate(&receipt)?;
                    validate_existing_pevcap_confirmation(&receipt, preview, publication)?;
                    self.verify_stored_pevcap(preview)?;
                    return Ok(PevcapImportReceipt {
                        duplicate: false,
                        ..receipt
                    });
                }
                Ok(None) => {}
                Err(_) => return result,
            }
        }
        if result.is_err() {
            // Keep the digest reserved until cleanup finishes, so a retry cannot reuse these
            // newly owned bytes between the abort and removal.
            let _ = self.request(|reply| Command::AbortPevcapImport {
                digest: preview.artifact_digest.clone(),
                ride_id,
                reply,
            });
            #[cfg(test)]
            pevcap_confirmation_hooks::run(&pevcap_confirmation_hooks::AFTER_ABORT);
        }
        result
    }

    fn confirm_existing_pevcap(
        &self,
        preview: &PevcapImportPreview,
        receipt: PevcapImportReceipt,
        publication: &CapturePublication,
    ) -> Result<PevcapImportReceipt, StorageError> {
        publication.validate_duplicate(&receipt)?;
        validate_existing_pevcap_confirmation(&receipt, preview, publication)?;
        capture_data::store(self, preview, preview.source_path())?;
        self.request(|reply| Command::PublishCaptureData {
            digest: preview.artifact_digest.clone(),
            reply,
        })?;
        self.verify_stored_pevcap(preview)?;
        Ok(receipt)
    }

    fn verify_stored_pevcap(&self, preview: &PevcapImportPreview) -> Result<(), StorageError> {
        let mut digest = Sha256::new();
        let mut total = 0_u64;
        let mut sequence = 0_u64;
        while let Some(chunk) = self.pevcap_capture_chunk(preview.artifact_digest(), sequence)? {
            total = total
                .checked_add(u64::try_from(chunk.len()).map_err(|_| {
                    StorageError::PevcapCaptureIntegrity("chunk size overflowed".to_owned())
                })?)
                .ok_or_else(|| {
                    StorageError::PevcapCaptureIntegrity("artifact size overflowed".to_owned())
                })?;
            if total > preview.artifact_size() {
                return Err(StorageError::PevcapPreviewChanged);
            }
            digest.update(chunk);
            sequence = sequence.checked_add(1).ok_or_else(|| {
                StorageError::PevcapCaptureIntegrity("chunk sequence overflowed".to_owned())
            })?;
        }
        if total != preview.artifact_size()
            || hex_encode(digest.finalize()) != preview.artifact_digest()
        {
            return Err(StorageError::PevcapCaptureIntegrity(
                "stored byte length or digest does not match capture identity".to_owned(),
            ));
        }
        Ok(())
    }

    /// Reads one bounded chunk of an imported capture's original bytes from SQLite.
    /// Chunks are at most 64 KiB; `None` means absent, incomplete, or end of capture.
    /// Neither the source nor a managed artifact file is needed.
    ///
    /// # Errors
    /// Returns [`StorageError`] when SQLite or the database worker cannot read the chunk.
    pub fn pevcap_capture_chunk(
        &self,
        digest: &str,
        sequence: u64,
    ) -> Result<Option<Vec<u8>>, StorageError> {
        self.request(|reply| Command::CaptureDataChunk {
            digest: digest.to_owned(),
            sequence,
            reply,
        })
    }

    /// Exports one published capture's exact original bytes from SQLite.
    ///
    /// The destination must not already exist. Reads remain bounded to one stored
    /// 64 KiB chunk at a time, and the completed file is retained only when its
    /// SHA-256 matches `digest`.
    ///
    /// # Errors
    /// Returns [`StorageError`] if the digest is invalid, the capture is missing or
    /// corrupt, a resource bound is exceeded, or the destination cannot be synced.
    pub fn export_pevcap_capture(
        &self,
        digest: &str,
        destination: &Path,
    ) -> Result<String, StorageError> {
        self.export_pevcap_capture_with_chunk_hook_inner(digest, destination, |_| Ok(()))
    }

    #[cfg(test)]
    pub(crate) fn export_pevcap_capture_with_chunk_hook(
        &self,
        digest: &str,
        destination: &Path,
        after_chunk: impl FnMut(u64) -> Result<(), StorageError>,
    ) -> Result<String, StorageError> {
        self.export_pevcap_capture_with_chunk_hook_inner(digest, destination, after_chunk)
    }

    fn export_pevcap_capture_with_chunk_hook_inner(
        &self,
        digest: &str,
        destination: &Path,
        mut after_chunk: impl FnMut(u64) -> Result<(), StorageError>,
    ) -> Result<String, StorageError> {
        let mut expected_digest = [0_u8; 32];
        if digest.len() != 64
            || hex::decode_to_slice(digest, &mut expected_digest).is_err()
            || hex_encode(expected_digest) != digest
        {
            return Err(StorageError::InvalidPevcapCaptureDigest);
        }

        let mut output = crate::atomic_export::AtomicExport::new(destination)?;
        let mut content_digest = Sha256::new();
        let mut byte_count = 0_u64;
        let mut sequence = 0_u64;
        loop {
            let Some(chunk) = self.pevcap_capture_chunk(digest, sequence)? else {
                break;
            };
            byte_count = byte_count
                .checked_add(u64::try_from(chunk.len()).map_err(|_| {
                    StorageError::PevcapCaptureIntegrity(
                        "chunk length cannot be represented".to_owned(),
                    )
                })?)
                .ok_or_else(|| {
                    StorageError::PevcapCaptureIntegrity("byte count overflowed".to_owned())
                })?;
            check_limit_result(PevcapLimits::IMPORT.check_artifact_bytes(byte_count))?;
            content_digest.update(&chunk);
            output.file_mut().write_all(&chunk)?;
            after_chunk(sequence)?;
            sequence = sequence.checked_add(1).ok_or_else(|| {
                StorageError::PevcapCaptureIntegrity("chunk sequence overflowed".to_owned())
            })?;
        }

        if byte_count == 0 {
            return Err(StorageError::PevcapCaptureNotFound);
        }
        if content_digest.finalize().as_slice() != expected_digest {
            return Err(StorageError::PevcapCaptureIntegrity(
                "stored byte digest does not match capture identity".to_owned(),
            ));
        }

        output.publish()?;
        Ok(digest.to_owned())
    }

    /// Lists published SQLite-backed captures in descending import-time/digest order.
    ///
    /// This includes captures without GPS or a ride. Staged bytes and legacy file-only
    /// receipts are excluded. Reading history never opens an external capture file.
    /// Cursors do not freeze a snapshot: newly imported entries ahead of a cursor appear
    /// when the first page is refreshed, not while continuing an older page.
    ///
    /// # Errors
    /// Returns [`StorageError`] when the worker or stored metadata cannot be read.
    pub fn list_pevcap_captures(
        &self,
        cursor: Option<PevcapCaptureCursor>,
        limit: QueryLimit,
    ) -> Result<PevcapCapturePage, StorageError> {
        self.request(|reply| Command::ListPevcapCaptures {
            cursor,
            limit,
            reply,
        })
    }

    /// Creates an empty canonical trail definition.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when spatial indexing is unavailable or the worker fails.
    pub fn create_trail(&self, name: &str) -> Result<TrailId, StorageError> {
        self.request(move |reply| Command::CreateTrail {
            name: name.to_owned(),
            reply,
        })
    }

    /// Appends a segment to a canonical trail and indexes its bounding box.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when spatial indexing is unavailable or the trail is missing.
    pub fn append_trail_segment(
        &self,
        trail_id: TrailId,
        sequence: u32,
        start: Coordinate,
        end: Coordinate,
    ) -> Result<(), StorageError> {
        self.request(move |reply| Command::AppendTrailSegment {
            trail_id,
            sequence,
            start,
            end,
            reply,
        })
    }

    /// Returns trail segments intersecting a WGS84 bounding box.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when spatial indexing is unavailable or the worker fails.
    pub fn trail_segments_in_bounds(
        &self,
        bounds: GeoBounds,
        cursor: Option<TrailSegmentCursor>,
        limit: QueryLimit,
    ) -> Result<TrailSegmentPage, StorageError> {
        self.request(move |reply| Command::TrailSegmentsInBounds {
            bounds,
            cursor,
            limit,
            reply,
        })
    }

    /// Stores a charging/food map point and indexes its coordinate.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when spatial indexing is unavailable or the point is invalid.
    pub fn create_map_point(
        &self,
        name: &str,
        coordinate: Coordinate,
    ) -> Result<MapPointId, StorageError> {
        self.request(move |reply| Command::CreateMapPoint {
            name: name.to_owned(),
            coordinate,
            reply,
        })
    }

    /// Returns charging/food points intersecting a WGS84 bounding box.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when spatial indexing is unavailable or the worker fails.
    pub fn map_points_in_bounds(
        &self,
        bounds: GeoBounds,
        cursor: Option<MapPointCursor>,
        limit: QueryLimit,
    ) -> Result<MapPointPage, StorageError> {
        self.request(move |reply| Command::MapPointsInBounds {
            bounds,
            cursor,
            limit,
            reply,
        })
    }

    /// Rebuilds every derived spatial index from canonical fixed-point rows.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when `RTree` is unavailable or rebuilding cannot commit atomically.
    pub fn rebuild_spatial_indexes(&self) -> Result<(), StorageError> {
        self.request(|reply| Command::RebuildSpatialIndexes { reply })
    }

    /// Writes a consistent `SQLite` backup to a caller-selected file.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the destination cannot be prepared or copied.
    pub fn backup_to(&self, path: &Path) -> Result<(), StorageError> {
        let destination = canonical_backup_path(path)?;
        self.request(move |reply| Command::Backup { destination, reply })
    }

    /// Exports one canonical ride summary as a versioned JSON document.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the ride is missing or the destination cannot be written.
    pub fn export_ride_json(&self, ride_id: RideId, path: &Path) -> Result<(), StorageError> {
        let destination = canonical_backup_path(path)?;
        self.request(move |reply| Command::ExportRideJson {
            ride_id,
            destination,
            reply,
        })
    }

    /// Applies one lifecycle event to a ride.
    ///
    /// The bounded command queue is waited on so lifecycle actions make progress even while
    /// location writes fill it.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the ride is missing, the transition is invalid, or the
    /// worker stops before accepting the command.
    pub fn transition(
        &self,
        ride_id: RideId,
        event: RideEvent,
    ) -> Result<RideLifecycleState, StorageError> {
        self.transition_blocking_with_timestamp(ride_id, event, None)
    }

    /// Applies one lifecycle event with its caller-supplied monotonic timestamp.
    ///
    /// Wall-clock ordering remains captured by Rust; the monotonic value drives elapsed-duration
    /// and pause accounting and must come from the same clock as route samples.
    ///
    /// The bounded command queue is waited on so lifecycle actions make progress even while
    /// location writes fill it.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the ride or worker rejects the transition.
    pub fn transition_at(
        &self,
        ride_id: RideId,
        event: RideEvent,
        monotonic_at_ms: u64,
    ) -> Result<RideLifecycleState, StorageError> {
        self.transition_blocking_with_timestamp(ride_id, event, Some(monotonic_at_ms))
    }

    /// Applies a timestamped lifecycle event and returns its transaction's moving-time checkpoint.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the ride or worker rejects the transition.
    pub fn transition_at_with_checkpoint(
        &self,
        ride_id: RideId,
        event: RideEvent,
        monotonic_at_ms: u64,
    ) -> Result<CommittedRideMovingTimeCheckpoint, StorageError> {
        self.queue_transition_at(ride_id, event, monotonic_at_ms)?
            .wait_committed_result()
    }

    /// Applies a lifecycle event and checkpoints the latest telemetry receipt in one row update.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the ride or worker rejects the transition.
    pub fn transition_at_with_telemetry(
        &self,
        ride_id: RideId,
        event: RideEvent,
        monotonic_at_ms: u64,
        last_telemetry_at_ms: Option<u64>,
    ) -> Result<RideLifecycleState, StorageError> {
        self.transition_at_with_telemetry_and_checkpoint(
            ride_id,
            event,
            monotonic_at_ms,
            last_telemetry_at_ms,
        )
        .map(CommittedRideMovingTimeCheckpoint::lifecycle)
    }

    /// Applies a lifecycle event with telemetry and returns the committed moving-time checkpoint.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the ride or worker rejects the transition.
    pub fn transition_at_with_telemetry_and_checkpoint(
        &self,
        ride_id: RideId,
        event: RideEvent,
        monotonic_at_ms: u64,
        last_telemetry_at_ms: Option<u64>,
    ) -> Result<CommittedRideMovingTimeCheckpoint, StorageError> {
        let occurred_at_ms = wall_clock_now_milliseconds()?;
        self.request_blocking(move |reply| Command::Transition {
            ride_id,
            event,
            occurred_at_ms,
            monotonic_at_ms: Some(monotonic_at_ms),
            last_telemetry_at_ms,
            reply,
        })
    }

    /// Queues one timestamped lifecycle transition without waiting for SQLite.
    ///
    /// The bounded queue returns `QueueFull` immediately when it cannot accept the command.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::QueueFull`] when the command queue is full, or a worker error when
    /// the command cannot be enqueued.
    pub fn queue_transition_at(
        &self,
        ride_id: RideId,
        event: RideEvent,
        monotonic_at_ms: u64,
    ) -> Result<PendingRideLifecycleTransition, StorageError> {
        self.queue_transition_at_with_telemetry(ride_id, event, monotonic_at_ms, None)
    }

    /// Queues a lifecycle event and telemetry receipt checkpoint without waiting for SQLite.
    ///
    /// Both are committed in the same row update; a rejected transition changes neither.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the bounded queue or worker cannot accept the command.
    pub fn queue_transition_at_with_telemetry(
        &self,
        ride_id: RideId,
        event: RideEvent,
        monotonic_at_ms: u64,
        last_telemetry_at_ms: Option<u64>,
    ) -> Result<PendingRideLifecycleTransition, StorageError> {
        let occurred_at_ms = wall_clock_now_milliseconds()?;
        let (reply, response) = response_channel();
        self.enqueue(Command::Transition {
            ride_id,
            event,
            occurred_at_ms,
            monotonic_at_ms: Some(monotonic_at_ms),
            last_telemetry_at_ms,
            reply,
        })?;
        Ok(PendingRideLifecycleTransition {
            response,
            consumed: false,
        })
    }

    fn transition_blocking_with_timestamp(
        &self,
        ride_id: RideId,
        event: RideEvent,
        monotonic_at_ms: Option<u64>,
    ) -> Result<RideLifecycleState, StorageError> {
        let occurred_at_ms = wall_clock_now_milliseconds()?;
        self.request_blocking(move |reply| Command::Transition {
            ride_id,
            event,
            occurred_at_ms,
            monotonic_at_ms,
            last_telemetry_at_ms: None,
            reply,
        })
        .map(CommittedRideMovingTimeCheckpoint::lifecycle)
    }

    /// Appends one location through the worker, returning its admission decision.
    ///
    /// [`LocationAdmission::Accepted`] means the point was stored. The other successful outcomes
    /// (`Duplicate`, `OutOfOrder`, `AccuracyTooLow`, and `UnrealisticJump`) explain why it was not
    /// added to the route.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the ride is missing, not accepting samples, or the worker
    /// cannot commit the point.
    pub fn append_location(
        &self,
        ride_id: RideId,
        sample: LocationSample,
    ) -> Result<LocationAdmission, StorageError> {
        self.append_location_with_segment(ride_id, sample, 0)
    }

    /// Appends one location with its Rust-owned route segment identity and returns its admission.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the ride is missing, not accepting samples, or the worker
    /// cannot commit the point.
    pub fn append_location_with_segment(
        &self,
        ride_id: RideId,
        sample: LocationSample,
        segment_id: u64,
    ) -> Result<LocationAdmission, StorageError> {
        self.append_location_with_segment_and_telemetry(
            ride_id,
            sample,
            segment_id,
            RouteTelemetryState::GpsOnly,
        )
    }

    /// Appends one location with segment and telemetry provenance, returning its admission.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the ride is missing, not accepting samples, or the worker
    /// cannot commit the point.
    pub fn append_location_with_segment_and_telemetry(
        &self,
        ride_id: RideId,
        sample: LocationSample,
        segment_id: u64,
        telemetry_state: RouteTelemetryState,
    ) -> Result<LocationAdmission, StorageError> {
        self.request(move |reply| Command::AppendLocation {
            ride_id,
            sample,
            segment_id: RideMapSegmentId::new(segment_id),
            telemetry_state,
            reply,
        })
    }

    /// Queues one location write without waiting for the `SQLite` worker.
    ///
    /// The bounded queue applies backpressure at submission time. Once accepted, callers can
    /// poll the returned ticket for the durable admission result.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::QueueFull`] when the bounded queue is saturated, or
    /// [`StorageError::WorkerStopped`] when the worker is unavailable.
    pub fn queue_location(
        &self,
        ride_id: RideId,
        sample: LocationSample,
        segment_id: RideMapSegmentId,
        start_reason: RideSegmentStartReason,
        telemetry_state: RouteTelemetryState,
    ) -> Result<PendingLocationWrite, StorageError> {
        self.queue_location_with_mode(
            ride_id,
            sample,
            segment_id,
            start_reason,
            telemetry_state,
            RideLocationCheckpoint::without_inactivity_evidence(None, None),
            LocationQueueMode::Bounded,
        )
    }

    /// Queues an accepted location with receipt-causal moving-time evidence in the same write.
    ///
    /// The optional verified-wheel observation is applied at the shared receipt boundary before
    /// the phone observation, preserving wheel priority without a separate telemetry write.
    ///
    /// # Errors
    /// Returns a storage error when observations are inconsistent or the bounded worker cannot
    /// accept the queued write.
    #[allow(
        clippy::too_many_arguments,
        reason = "the queued location transaction carries the complete route and metric payload"
    )]
    pub fn queue_location_with_moving_time(
        &self,
        ride_id: RideId,
        sample: LocationSample,
        segment_id: RideMapSegmentId,
        start_reason: RideSegmentStartReason,
        telemetry_state: RouteTelemetryState,
        moving_time_observation: MovingTimeObservation,
        latest_vehicle_moving_time_observation: Option<MovingTimeObservation>,
    ) -> Result<PendingLocationWrite, StorageError> {
        self.queue_location_with_mode(
            ride_id,
            sample,
            segment_id,
            start_reason,
            telemetry_state,
            RideLocationCheckpoint::without_inactivity_evidence(
                Some(moving_time_observation),
                latest_vehicle_moving_time_observation,
            ),
            LocationQueueMode::Bounded,
        )
    }

    /// Queues a location with moving-time evidence, waiting until bounded worker capacity is
    /// available. Use from background recorders, never from a UI executor or database worker.
    ///
    /// # Errors
    /// Returns a storage error when observations are inconsistent or the bounded worker cannot
    /// accept the queued write.
    #[allow(
        clippy::too_many_arguments,
        reason = "the queued location transaction carries the complete route and metric payload"
    )]
    pub fn queue_location_with_moving_time_waiting_for_capacity(
        &self,
        ride_id: RideId,
        sample: LocationSample,
        segment_id: RideMapSegmentId,
        start_reason: RideSegmentStartReason,
        telemetry_state: RouteTelemetryState,
        moving_time_observation: MovingTimeObservation,
        latest_vehicle_moving_time_observation: Option<MovingTimeObservation>,
    ) -> Result<PendingLocationWrite, StorageError> {
        self.queue_location_with_mode(
            ride_id,
            sample,
            segment_id,
            start_reason,
            telemetry_state,
            RideLocationCheckpoint::without_inactivity_evidence(
                Some(moving_time_observation),
                latest_vehicle_moving_time_observation,
            ),
            LocationQueueMode::WaitForCapacity,
        )
    }

    /// Queues a location from a background recorder, waiting for worker queue capacity.
    ///
    /// The queue stays bounded. This may block until another command is consumed, so callers
    /// must not use it on a UI executor or from the database worker. The returned ticket still
    /// reports the durable admission result separately from submission.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::WorkerStopped`] when the worker is unavailable.
    pub fn queue_location_waiting_for_capacity(
        &self,
        ride_id: RideId,
        sample: LocationSample,
        segment_id: RideMapSegmentId,
        start_reason: RideSegmentStartReason,
        telemetry_state: RouteTelemetryState,
    ) -> Result<PendingLocationWrite, StorageError> {
        self.queue_location_with_mode(
            ride_id,
            sample,
            segment_id,
            start_reason,
            telemetry_state,
            RideLocationCheckpoint::without_inactivity_evidence(None, None),
            LocationQueueMode::WaitForCapacity,
        )
    }

    /// Queues one location with its moving-time and inactivity checkpoint in the same write.
    ///
    /// Evidence is persisted only when the location is accepted. The mode controls whether a
    /// full bounded queue returns immediately or waits for capacity.
    ///
    /// # Errors
    /// Returns a storage error for inconsistent motion evidence, queue saturation in bounded
    /// mode, or an unavailable worker.
    #[allow(
        clippy::too_many_arguments,
        reason = "the location boundary carries the sample, segment, and checkpoint atomically"
    )]
    pub fn queue_location_with_checkpoint(
        &self,
        ride_id: RideId,
        sample: LocationSample,
        segment_id: RideMapSegmentId,
        start_reason: RideSegmentStartReason,
        telemetry_state: RouteTelemetryState,
        checkpoint: RideLocationCheckpoint,
        queue_mode: LocationQueueMode,
    ) -> Result<PendingLocationWrite, StorageError> {
        self.queue_location_with_mode(
            ride_id,
            sample,
            segment_id,
            start_reason,
            telemetry_state,
            checkpoint,
            queue_mode,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "the location boundary carries the sample, segment, and checkpoint atomically"
    )]
    fn queue_location_with_mode(
        &self,
        ride_id: RideId,
        sample: LocationSample,
        segment_id: RideMapSegmentId,
        start_reason: RideSegmentStartReason,
        telemetry_state: RouteTelemetryState,
        checkpoint: RideLocationCheckpoint,
        queue_mode: LocationQueueMode,
    ) -> Result<PendingLocationWrite, StorageError> {
        validate_paired_moving_time_observations(
            sample,
            checkpoint.moving_time_observation,
            checkpoint.latest_vehicle_moving_time_observation,
        )?;
        let (reply, response) = response_channel();
        let command = Command::AppendLocationAsync {
            ride_id,
            sample,
            segment_id,
            start_reason,
            telemetry_state,
            moving_time_observation: checkpoint.moving_time_observation,
            latest_vehicle_moving_time_observation: checkpoint
                .latest_vehicle_moving_time_observation,
            inactivity_evidence: checkpoint.inactivity_evidence,
            reply,
        };
        match queue_mode {
            LocationQueueMode::Bounded => self.enqueue(command)?,
            LocationQueueMode::WaitForCapacity => self.enqueue_blocking(command)?,
        }
        Ok(PendingLocationWrite {
            response,
            consumed: false,
        })
    }

    /// Enqueues a location behind a deterministic worker gate for persistence tests.
    #[cfg(test)]
    pub(crate) fn enqueue_location_with_worker_gate_for_test(
        &self,
        ride_id: RideId,
        sample: LocationSample,
        segment_id: RideMapSegmentId,
        telemetry_state: RouteTelemetryState,
        entered: SyncSender<()>,
        release: Receiver<()>,
    ) -> Result<PendingLocationWrite, StorageError> {
        let (reply, response) = response_channel();
        self.enqueue(Command::AppendLocationWithWorkerGate {
            ride_id,
            sample,
            segment_id,
            telemetry_state,
            entered,
            release,
            reply,
        })?;
        Ok(PendingLocationWrite {
            response,
            consumed: false,
        })
    }

    /// Enqueues a location and deliberately loses its terminal response after worker failure.
    #[cfg(test)]
    pub(crate) fn enqueue_location_with_worker_failure_for_test(
        &self,
        _ride_id: RideId,
        _sample: LocationSample,
        _segment_id: u64,
        _telemetry_state: RouteTelemetryState,
        entered: SyncSender<()>,
    ) -> Result<PendingLocationWrite, StorageError> {
        let (reply, response) = response_channel();
        self.enqueue(Command::AppendLocationWithWorkerFailure { entered, reply })?;
        Ok(PendingLocationWrite {
            response,
            consumed: false,
        })
    }

    /// Loads the durable summary projection for one ride.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::NotFound`] when the ride does not exist or another worker error
    /// prevents the query.
    pub fn summary(&self, ride_id: RideId) -> Result<RideSummary, StorageError> {
        self.request(move |reply| Command::Summary { ride_id, reply })
    }

    /// Loads a ride summary and its persisted elapsed duration without applying history filters.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::NotFound`] when the ride does not exist or another worker error
    /// prevents the query.
    pub fn summary_with_duration(
        &self,
        ride_id: RideId,
    ) -> Result<(RideSummary, u64), StorageError> {
        self.request(move |reply| Command::SummaryWithDuration { ride_id, reply })
    }

    /// Loads one visible ride record by its stable identifier without paging through history.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot query or decode the record.
    pub fn find_ride(&self, ride_id: RideId) -> Result<Option<RideRecord>, StorageError> {
        self.request(move |reply| Command::FindRide { ride_id, reply })
    }

    /// Loads one visible ride by id only when it still matches the active history filters.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot query or decode the record.
    pub fn find_ride_filtered(
        &self,
        ride_id: RideId,
        query: RideHistoryQuery,
    ) -> Result<Option<RideRecord>, StorageError> {
        self.request(move |reply| Command::FindRideFiltered {
            ride_id,
            query,
            reply,
        })
    }

    /// Finds the newest open ride that can be adopted by a map core.
    ///
    /// A database-open pass normally changes active and paused rides to interrupted. The active
    /// and paused cases remain here for same-process retries after another core has already
    /// durably restored the ride. Terminal rides are never recovery candidates.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot query or decode the record.
    pub fn newest_recoverable_ride(&self) -> Result<Option<RideRecord>, StorageError> {
        self.request(|reply| Command::NewestRecoverableRide { reply })
    }

    /// Reads the persisted app-presence and verified-device timestamps for one ride.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::NotFound`] when the ride does not exist, or a storage error when
    /// the worker cannot read or decode the evidence.
    pub fn ride_inactivity_evidence(
        &self,
        ride_id: RideId,
    ) -> Result<Option<RideInactivityEvidence>, StorageError> {
        self.request(move |reply| Command::RideInactivityEvidence { ride_id, reply })
    }

    /// Persists an ordered app-presence observation for a recoverable ride.
    ///
    /// Older or duplicate observations return the current evidence without changing it.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::NotFound`] for an unknown ride,
    /// [`StorageError::InvalidRideState`] for a terminal ride, or a storage error when the
    /// timestamp cannot be represented or committed.
    pub fn observe_ride_app_presence(
        &self,
        ride_id: RideId,
        order: RideAppPresenceOrder,
        presence: RideAppPresence,
        observed_at: WallClockUnixMilliseconds,
    ) -> Result<RideInactivityEvidence, StorageError> {
        self.request(move |reply| Command::ObserveRideAppPresence {
            ride_id,
            order,
            presence,
            observed_at,
            reply,
        })
    }

    /// Persists a newer verified-device observation without changing app-presence evidence.
    ///
    /// This is for bounded checkpoints. Telemetry paths should attach the latest Core snapshot
    /// to an existing [`RideLocationCheckpoint`] or metadata transaction, not write once per
    /// packet.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::NotFound`] for an unknown ride,
    /// [`StorageError::InvalidRideState`] for a terminal ride, or a storage error when the
    /// timestamp cannot be represented or committed.
    pub fn observe_verified_device_seen(
        &self,
        ride_id: RideId,
        observed_at: WallClockUnixMilliseconds,
    ) -> Result<RideInactivityEvidence, StorageError> {
        self.request(move |reply| Command::ObserveVerifiedDeviceSeen {
            ride_id,
            observed_at,
            reply,
        })
    }

    /// Queues the bounded durable state needed to restore a mobile ride-map core.
    ///
    /// The query runs as one worker command so its ride, route tail, and associated history state
    /// are read without another database command interleaving. Enqueue never waits for SQLite.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::QueueFull`] when the worker queue is saturated or a worker error
    /// when the command cannot be accepted.
    pub fn queue_restore_snapshot(&self) -> Result<PendingRideRestoreSnapshot, StorageError> {
        let (reply, response) = response_channel();
        self.enqueue(Command::RestoreSnapshot { reply })?;
        Ok(PendingRideRestoreSnapshot {
            response,
            consumed: false,
        })
    }

    /// Lists one bounded page of visible rides in stable newest-first order.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot query or decode the page.
    pub fn list_rides(
        &self,
        cursor: Option<RideCursor>,
        limit: QueryLimit,
    ) -> Result<RidePage, StorageError> {
        self.list_rides_filtered(cursor, limit, RideHistoryQuery::default())
    }

    /// Lists one bounded page of visible rides with Rust-owned history filters.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot query or decode the page.
    pub fn list_rides_filtered(
        &self,
        cursor: Option<RideCursor>,
        limit: QueryLimit,
        query: RideHistoryQuery,
    ) -> Result<RidePage, StorageError> {
        self.request(move |reply| Command::ListRides {
            cursor,
            limit,
            query,
            reply,
        })
    }

    /// Projects the bounded history page into subdued contextual routes.
    ///
    /// Rust loads at most the requested history page, excludes `selected_ride`, and projects
    /// only the configured number of remaining routes. Each route uses the same viewport,
    /// privacy, endpoint, and segment policy as selected-route projection. The aggregate point
    /// budget prevents a dense history page from multiplying the per-route display budget.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::DeadlineExceeded`] when the Rust-owned projection deadline expires
    /// or another typed storage error when the page or route cannot be decoded.
    pub fn project_history_context(
        &self,
        query: RideHistoryQuery,
        selected_ride: Option<RideId>,
        budget: HistoryContextBudget,
        viewport: Option<RouteViewport>,
        privacy: RoutePrivacyPolicy,
    ) -> Result<HistoryContextProjection, StorageError> {
        let cancellation = default_route_projection_cancellation();
        let deadline = cancellation.deadline();
        let (reply, response) = response_channel();
        self.enqueue(Command::ProjectHistoryContext {
            query,
            selected_ride,
            budget,
            viewport,
            privacy,
            cancellation: cancellation.clone(),
            reply,
        })?;
        match deadline {
            Some(deadline) => receive_until(&response, deadline, &cancellation),
            None => receive(&response),
        }
    }

    /// Lists every vehicle identity referenced by a visible ride.
    ///
    /// The result is not derived from a history page, so older vehicles remain available to the
    /// history filter even when the first page is full. Display names come from the device table.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot query or decode the options.
    pub fn list_ride_history_vehicle_options(
        &self,
    ) -> Result<Vec<RideHistoryVehicleOption>, StorageError> {
        self.request(|reply| Command::ListRideHistoryVehicleOptions { reply })
    }

    /// Loads one bounded page of canonical route points in ascending sequence order.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::NotFound`] when the ride does not exist, or another typed storage
    /// error when the page cannot be decoded.
    pub fn route_points(
        &self,
        ride_id: RideId,
        cursor: Option<RoutePointCursor>,
        limit: QueryLimit,
    ) -> Result<RoutePointPage, StorageError> {
        self.request(move |reply| Command::RoutePoints {
            ride_id,
            cursor,
            limit,
            reply,
        })
    }

    /// Projects a durable route through Rust-owned viewport, LOD, and privacy policy.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::NotFound`] when the ride does not exist or another storage error
    /// when the route cannot be decoded.
    pub fn project_route_points(
        &self,
        ride_id: RideId,
        viewport: Option<RouteViewport>,
        budget: RouteDisplayBudget,
        privacy: RoutePrivacyPolicy,
    ) -> Result<RoutePointProjection, StorageError> {
        self.project_route_points_cancellable(
            ride_id,
            viewport,
            budget,
            privacy,
            &default_route_projection_cancellation(),
        )
    }

    /// Projects a durable route while honoring cooperative cancellation.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Cancelled`] when cancellation is requested, or another storage
    /// error when the route cannot be decoded.
    pub fn project_route_points_cancellable(
        &self,
        ride_id: RideId,
        viewport: Option<RouteViewport>,
        budget: RouteDisplayBudget,
        privacy: RoutePrivacyPolicy,
        cancellation: &RouteProjectionCancellation,
    ) -> Result<RoutePointProjection, StorageError> {
        let deadline = cancellation.deadline();
        let (reply, response) = response_channel();
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            cancellation.cancel();
            return Err(StorageError::DeadlineExceeded);
        }
        self.enqueue(Command::ProjectRoutePoints {
            ride_id,
            viewport,
            budget,
            privacy,
            cancellation: Some(cancellation.clone()),
            reply,
        })?;
        match deadline {
            Some(deadline) => receive_until(&response, deadline, cancellation),
            None => receive(&response),
        }
    }

    /// Installs a deterministic `SQLite` progress gate for an in-flight projection test.
    #[cfg(test)]
    pub(crate) fn install_route_projection_test_gate(
        &self,
        entered: SyncSender<()>,
        release: Receiver<()>,
    ) -> Result<(), StorageError> {
        self.request(|reply| Command::InstallRouteProjectionTestGate {
            entered,
            release,
            reply,
        })
    }

    /// Stops the worker without releasing its owner entry, for worker-recovery tests.
    #[cfg(test)]
    pub(crate) fn stop_worker_for_test(&self) -> Result<(), StorageError> {
        let result = self.request(|reply| Command::StopForTest { reply });
        if result.is_ok() {
            service::wait_for_worker_exit(self.service_id);
        }
        result
    }

    /// Marks the service as shutting down without stopping its worker, for recovery tests.
    #[cfg(test)]
    pub(crate) fn begin_shutdown_for_test(&self) -> Result<(), StorageError> {
        service::begin_shutdown(self.service_id).map(|_| ())
    }

    /// Stops the process-wide worker and releases its ownership slot.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot stop or its ownership slot cannot be
    /// released.
    pub fn shutdown(self) -> Result<(), StorageError> {
        self.maintenance_demand.fetch_add(1, Ordering::AcqRel);
        let sender = service::begin_shutdown(self.service_id)?;
        let (reply, response) = response_channel();
        if sender.send(Command::Shutdown { reply }).is_err() {
            service::cancel_shutdown(self.service_id);
            return Err(StorageError::WorkerStopped);
        }
        self.maintenance_demand.fetch_add(1, Ordering::AcqRel);
        receive(&response)?;
        service::finish_shutdown(self.service_id)
    }

    fn request<T>(&self, build: impl FnOnce(Reply<T>) -> Command) -> Result<T, StorageError> {
        let (reply, response) = response_channel();
        self.enqueue(build(reply))?;
        receive(&response)
    }

    fn request_blocking<T>(
        &self,
        build: impl FnOnce(Reply<T>) -> Command,
    ) -> Result<T, StorageError> {
        let (reply, response) = response_channel();
        self.enqueue_blocking(build(reply))?;
        receive(&response)
    }

    fn enqueue(&self, command: Command) -> Result<(), StorageError> {
        self.maintenance_demand.fetch_add(1, Ordering::AcqRel);
        if self.worker_has_exited() {
            let result = self.enqueue_after_worker_exit(command);
            if result.is_ok() {
                self.maintenance_demand.fetch_add(1, Ordering::AcqRel);
            }
            return result;
        }
        match self.sender.try_send(command) {
            Ok(()) => {
                self.maintenance_demand.fetch_add(1, Ordering::AcqRel);
                Ok(())
            }
            Err(mpsc::TrySendError::Full(_)) => Err(StorageError::QueueFull),
            Err(mpsc::TrySendError::Disconnected(command)) => {
                let result = self.enqueue_after_worker_exit(command);
                if result.is_ok() {
                    self.maintenance_demand.fetch_add(1, Ordering::AcqRel);
                }
                result
            }
        }
    }

    fn enqueue_blocking(&self, command: Command) -> Result<(), StorageError> {
        self.maintenance_demand.fetch_add(1, Ordering::AcqRel);
        if self.worker_has_exited() {
            let result = self.enqueue_after_worker_exit_blocking(command);
            if result.is_ok() {
                self.maintenance_demand.fetch_add(1, Ordering::AcqRel);
            }
            return result;
        }
        match self.sender.send(command) {
            Ok(()) => {
                self.maintenance_demand.fetch_add(1, Ordering::AcqRel);
                Ok(())
            }
            Err(mpsc::SendError(command)) => {
                let result = self.enqueue_after_worker_exit_blocking(command);
                if result.is_ok() {
                    self.maintenance_demand.fetch_add(1, Ordering::AcqRel);
                }
                result
            }
        }
    }

    fn enqueue_after_worker_exit(&self, command: Command) -> Result<(), StorageError> {
        if !self.worker_can_restart() {
            return Err(StorageError::WorkerStopped);
        }
        let recovered = self.reopen()?;
        recovered
            .sender
            .try_send(command)
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => StorageError::QueueFull,
                mpsc::TrySendError::Disconnected(_) => StorageError::WorkerStopped,
            })
    }

    fn enqueue_after_worker_exit_blocking(&self, command: Command) -> Result<(), StorageError> {
        if !self.worker_can_restart() {
            return Err(StorageError::WorkerStopped);
        }
        let recovered = self.reopen()?;
        recovered
            .sender
            .send(command)
            .map_err(|_| StorageError::WorkerStopped)
    }

    fn worker_has_exited(&self) -> bool {
        service::worker_has_exited(self.service_id)
    }

    /// Returns whether this handle still belongs to a process-owned database service.
    ///
    /// A handle may be stale after another command has already recovered the worker, so this
    /// checks the stable service identity rather than the worker join state. An explicit shutdown
    /// removes the owner entry, so stale handles remain unusable instead of silently starting a
    /// new service during a later callback.
    fn worker_can_restart(&self) -> bool {
        service::can_restart(self.service_id)
    }
}

type Reply<T> = mpsc::Sender<Result<T, StorageError>>;

fn response_channel<T>() -> (Reply<T>, Receiver<Result<T, StorageError>>) {
    mpsc::channel()
}

fn receive<T>(response: &Receiver<Result<T, StorageError>>) -> Result<T, StorageError> {
    response.recv().map_err(|_| StorageError::ResponseDropped)?
}

fn receive_until<T>(
    response: &Receiver<Result<T, StorageError>>,
    deadline: Instant,
    cancellation: &RouteProjectionCancellation,
) -> Result<T, StorageError> {
    let timeout = deadline.saturating_duration_since(Instant::now());
    match response.recv_timeout(timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            cancellation.cancel();
            Err(StorageError::DeadlineExceeded)
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(StorageError::ResponseDropped),
    }
}

fn projection_checkpoint(
    cancellation: Option<&RouteProjectionCancellation>,
) -> Result<(), StorageError> {
    if let Some(cancellation) = cancellation
        && let Some(error) = cancellation.abort_error()
    {
        return Err(error);
    }
    Ok(())
}

fn projection_sqlite<T>(
    result: rusqlite::Result<T>,
    cancellation: Option<&RouteProjectionCancellation>,
) -> Result<T, StorageError> {
    result.map_err(|error| {
        if let rusqlite::Error::SqliteFailure(failure, _) = &error
            && failure.code == ErrorCode::OperationInterrupted
        {
            if cancellation.is_some_and(RouteProjectionCancellation::is_expired) {
                StorageError::DeadlineExceeded
            } else if cancellation.is_some_and(RouteProjectionCancellation::is_cancelled) {
                StorageError::Cancelled
            } else {
                StorageError::Sqlite(error)
            }
        } else {
            StorageError::Sqlite(error)
        }
    })
}

enum PevcapBegin {
    Started { ride_id: Option<RideId> },
    Duplicate(PevcapImportReceipt),
}

#[derive(Clone)]
enum CapturePublication {
    Import,
    Recording(RecordedCapture),
    SubmissionSnapshot,
}

impl CapturePublication {
    fn recording(&self) -> Option<&RecordedCapture> {
        match self {
            Self::Import | Self::SubmissionSnapshot => None,
            Self::Recording(recording) => Some(recording),
        }
    }

    fn validate_duplicate(&self, receipt: &PevcapImportReceipt) -> Result<(), StorageError> {
        match self {
            Self::Import | Self::SubmissionSnapshot => Ok(()),
            Self::Recording(recording) if receipt.recording.as_ref() == Some(recording) => Ok(()),
            Self::Recording(_) => Err(StorageError::CaptureIdentityConflict),
        }
    }
}

struct PevcapImportCompletion {
    digest: String,
    ride_id: Option<RideId>,
    managed_path: PathBuf,
    outcome: NewPevcapImportOutcome,
    artifact_size: u64,
    record_count: u64,
    location_count: u64,
    duration_milliseconds: u64,
    imported_at_ms: u64,
    publication: CapturePublication,
}

enum Command {
    PhoneControl(phone_control::Command),
    DeviceVehicle(device_vehicle::Command),
    DeviceDrivetrain(device_drivetrain::Command),
    CaptureSubmission(capture_submission::Command),
    Charging(charging::Command),
    BeginLiveCapture {
        id: LiveCaptureId,
        header_json: Vec<u8>,
        started_at_ms: u64,
        reply: Reply<()>,
    },
    AppendLiveCaptureEvent {
        id: LiveCaptureId,
        kind: LiveCaptureEventKind,
        receipt_monotonic_ms: u64,
        source_monotonic_offset_ms: Option<i64>,
        source_wall_clock_unix_ms: Option<u64>,
        payload: Vec<u8>,
        location: Option<LiveCaptureLocationObservation>,
        ble_record: Option<Box<PevcapRecord>>,
        reply: Reply<u64>,
    },
    FinishLiveCapture {
        id: LiveCaptureId,
        finished_at_ms: u64,
        integrity: LiveCaptureIntegrity,
        reply: Reply<()>,
    },
    FinalizeLiveCapture {
        id: LiveCaptureId,
        header_json: Vec<u8>,
        finished_at_ms: u64,
        integrity: LiveCaptureIntegrity,
        reply: Reply<()>,
    },
    UpdateLiveCaptureHeader {
        id: LiveCaptureId,
        header_json: Vec<u8>,
        reply: Reply<()>,
    },
    ReadLiveCapture {
        id: LiveCaptureId,
        after_sequence: Option<i64>,
        limit: QueryLimit,
        reply: Reply<LiveCaptureSnapshot>,
    },
    ReadLiveCaptureSemanticTelemetry {
        id: LiveCaptureId,
        sequence: i64,
        reply: Reply<Option<cutout_core::PevcapSemanticTelemetry>>,
    },
    RetainLiveCaptureContext {
        id: LiveCaptureId,
        header_json: Vec<u8>,
        context_json: String,
        reply: Reply<()>,
    },
    ListLiveCaptureHistory {
        cursor: Option<LiveCaptureHistoryCursor>,
        limit: QueryLimit,
        reply: Reply<LiveCaptureHistoryPage>,
    },
    RecordedCaptureLookup {
        id: crate::CaptureArtifactId,
        reply: Reply<Option<PevcapImportReceipt>>,
    },
    ListPevcapCaptures {
        cursor: Option<PevcapCaptureCursor>,
        limit: QueryLimit,
        reply: Reply<PevcapCapturePage>,
    },
    BeginCaptureData {
        digest: String,
        encoding: PevcapEncoding,
        artifact_size: u64,
        reply: Reply<capture_data::CaptureStart>,
    },
    AppendCaptureData {
        digest: String,
        sequence: u64,
        chunk: capture_data::CaptureChunk,
        reply: Reply<()>,
    },
    PublishCaptureData {
        digest: String,
        reply: Reply<()>,
    },
    AbortCaptureData {
        digest: String,
        reply: Reply<()>,
    },
    CaptureDataChunk {
        digest: String,
        sequence: u64,
        reply: Reply<Option<Vec<u8>>>,
    },
    Capabilities {
        reply: Reply<SqliteCapabilities>,
    },
    IntegrityCheck {
        reply: Reply<()>,
    },
    CreateRide {
        source: RideSource,
        created_at_ms: u64,
        monotonic_created_at_ms: Option<u64>,
        reply: Reply<RideId>,
    },
    CreateStartedLiveRide {
        created_at_ms: u64,
        monotonic_created_at_ms: u64,
        candidate_vehicle: Option<String>,
        music_history_policy: Option<MusicHistoryPolicy>,
        reply: Reply<CommittedRideMovingTimeCheckpoint>,
    },
    SettleRecoveredRide {
        ride_id: RideId,
        discard_empty: bool,
        occurred_at_ms: u64,
        monotonic_at_ms: u64,
        replacement_candidate_vehicle: Option<String>,
        reply: Reply<Option<CommittedRideMovingTimeCheckpoint>>,
    },
    SettleInactiveLiveRide {
        ride_id: RideId,
        expected_evidence: RideInactivityEvidence,
        occurred_at_ms: u64,
        monotonic_at_ms: u64,
        replacement_candidate_vehicle: Option<String>,
        reply: Reply<Option<CommittedRideMovingTimeCheckpoint>>,
    },
    CreateStartedRide {
        source: RideSource,
        created_at_ms: u64,
        monotonic_created_at_ms: Option<u64>,
        candidate_vehicle: Option<String>,
        occurred_at_ms: u64,
        reply: Reply<RideId>,
    },
    UpdateRideMapMetadata {
        ride_id: RideId,
        candidate_vehicle: Option<String>,
        associated_vehicle: Option<String>,
        associated_at_ms: Option<u64>,
        last_telemetry_at_ms: Option<u64>,
        motion: Option<RideMotionObservation>,
        moving_time_observation: Option<MovingTimeObservation>,
        inactivity_evidence: Option<RideInactivityEvidence>,
        reply: Reply<MovingTimeCheckpoint>,
    },
    RecordBmsVoltageSamples {
        samples: Vec<BmsVoltageSampleRecord>,
        reply: Reply<()>,
    },
    SaveDeviceName {
        platform_identifier: String,
        display_name: String,
        updated_at_ms: u64,
        reply: Reply<()>,
    },
    MigrateDeviceName {
        platform_identifier: String,
        display_name: String,
        updated_at_ms: u64,
        reply: Reply<Option<String>>,
    },
    DeviceName {
        platform_identifier: String,
        reply: Reply<Option<String>>,
    },
    SaveSelectedDevice {
        platform_identifier: String,
        updated_at_ms: u64,
        reply: Reply<()>,
    },
    RememberSelectedDevice {
        platform_identifier: String,
        display_name: Option<String>,
        updated_at_ms: u64,
        reply: Reply<()>,
    },
    SelectedDevice {
        reply: Reply<Option<String>>,
    },
    ClearSelectedDevice {
        reply: Reply<()>,
    },
    RememberLastConnectedDevice {
        platform_identifier: String,
        updated_at_ms: u64,
        reply: Reply<()>,
    },
    VerifiedConnectionAdmission {
        platform_identifier: String,
        updated_at_ms: u64,
        lifecycle_mutations: Vec<VerifiedConnectionLifecycleMutation>,
        metadata: Option<VerifiedConnectionRideMetadata>,
        unselected_metadata: Option<VerifiedConnectionRideMetadata>,
        reply: Reply<VerifiedConnectionAdmissionOutcome>,
    },
    LastConnectedDevice {
        reply: Reply<Option<String>>,
    },
    ClearLastConnectedDevice {
        reply: Reply<()>,
    },
    RideAutostartEnabled {
        reply: Reply<bool>,
    },
    SaveAutomaticDiagnosticRecordingEnabled {
        enabled: bool,
        reply: Reply<()>,
    },
    SaveRideAutostartEnabled {
        enabled: bool,
        reply: Reply<()>,
    },
    SavePhoneAlarmPreferences {
        device_identity: String,
        preferences: PhoneAlarmPreferencesRecord,
        reply: Reply<()>,
    },
    PhoneAlarmPreferences {
        device_identity: String,
        reply: Reply<Option<PhoneAlarmPreferencesRecord>>,
    },
    RecordMusicEvent {
        ride_id: RideId,
        event: MusicRideEvent,
        reply: Reply<MusicTimelineRecordResult>,
    },
    MusicHistory {
        ride_id: RideId,
        reply: Reply<MusicHistory>,
    },
    SaveMusicHistoryPolicy {
        ride_id: RideId,
        policy: MusicHistoryPolicy,
        reply: Reply<()>,
    },
    #[allow(
        dead_code,
        reason = "crate-local import contract is covered by persistence tests"
    )]
    SaveMusicEvent {
        ride_id: RideId,
        policy: MusicHistoryPolicy,
        sequence: i64,
        event: MusicRideEvent,
        reply: Reply<()>,
    },
    DeleteMusicHistory {
        ride_id: RideId,
        reply: Reply<()>,
    },
    DeleteSpotifyMusicHistory {
        reply: Reply<()>,
    },
    #[allow(
        dead_code,
        reason = "crate-local raw projection is covered by persistence tests"
    )]
    MusicEvents {
        ride_id: RideId,
        reply: Reply<Vec<MusicRideEvent>>,
    },
    MusicHistoryPolicy {
        ride_id: RideId,
        reply: Reply<MusicHistoryPolicy>,
    },
    MusicHistoryState {
        ride_id: RideId,
        reply: Reply<MusicHistoryState>,
    },
    SaveVoltageSagModel {
        device_identity: String,
        model: VoltageSagModelRecord,
        reply: Reply<()>,
    },
    VoltageSagModel {
        device_identity: String,
        reply: Reply<Option<VoltageSagModelRecord>>,
    },
    RemoveVoltageSagModel {
        device_identity: String,
        reply: Reply<()>,
    },
    SaveRideSessionMarker {
        marker: Vec<u8>,
        reply: Reply<()>,
    },
    RideSessionMarker {
        reply: Reply<Option<Vec<u8>>>,
    },
    ClearRideSessionMarker {
        reply: Reply<()>,
    },
    PevcapImportLookup {
        digest: String,
        reply: Reply<Option<PevcapImportReceipt>>,
    },
    BeginPevcapImport {
        digest: String,
        managed_path: PathBuf,
        outcome: NewPevcapImportOutcome,
        created_at_ms: u64,
        reply: Reply<PevcapBegin>,
    },
    AppendPevcapLocationBatch {
        ride_id: RideId,
        samples: Vec<PevcapRoutePoint>,
        reply: Reply<u64>,
    },
    FinishPevcapImport {
        completion: PevcapImportCompletion,
        reply: Reply<PevcapImportReceipt>,
    },
    AbortPevcapImport {
        digest: String,
        ride_id: Option<RideId>,
        reply: Reply<()>,
    },
    CreateTrail {
        name: String,
        reply: Reply<TrailId>,
    },
    AppendTrailSegment {
        trail_id: TrailId,
        sequence: u32,
        start: Coordinate,
        end: Coordinate,
        reply: Reply<()>,
    },
    TrailSegmentsInBounds {
        bounds: GeoBounds,
        cursor: Option<TrailSegmentCursor>,
        limit: QueryLimit,
        reply: Reply<TrailSegmentPage>,
    },
    CreateMapPoint {
        name: String,
        coordinate: Coordinate,
        reply: Reply<MapPointId>,
    },
    MapPointsInBounds {
        bounds: GeoBounds,
        cursor: Option<MapPointCursor>,
        limit: QueryLimit,
        reply: Reply<MapPointPage>,
    },
    RebuildSpatialIndexes {
        reply: Reply<()>,
    },
    Backup {
        destination: PathBuf,
        reply: Reply<()>,
    },
    ExportRideJson {
        ride_id: RideId,
        destination: PathBuf,
        reply: Reply<()>,
    },
    Transition {
        ride_id: RideId,
        event: RideEvent,
        occurred_at_ms: u64,
        monotonic_at_ms: Option<u64>,
        last_telemetry_at_ms: Option<u64>,
        reply: Reply<CommittedRideMovingTimeCheckpoint>,
    },
    AppendLocation {
        ride_id: RideId,
        sample: LocationSample,
        segment_id: RideMapSegmentId,
        telemetry_state: RouteTelemetryState,
        reply: Reply<LocationAdmission>,
    },
    AppendLocationAsync {
        ride_id: RideId,
        sample: LocationSample,
        segment_id: RideMapSegmentId,
        start_reason: RideSegmentStartReason,
        telemetry_state: RouteTelemetryState,
        moving_time_observation: Option<MovingTimeObservation>,
        latest_vehicle_moving_time_observation: Option<MovingTimeObservation>,
        inactivity_evidence: Option<RideInactivityEvidence>,
        reply: Reply<LocationWriteResult>,
    },
    #[cfg(test)]
    AppendLocationWithWorkerGate {
        ride_id: RideId,
        sample: LocationSample,
        segment_id: RideMapSegmentId,
        telemetry_state: RouteTelemetryState,
        entered: SyncSender<()>,
        release: Receiver<()>,
        reply: Reply<LocationWriteResult>,
    },
    #[cfg(test)]
    AppendLocationWithWorkerFailure {
        entered: SyncSender<()>,
        reply: Reply<LocationWriteResult>,
    },
    Summary {
        ride_id: RideId,
        reply: Reply<RideSummary>,
    },
    SummaryWithDuration {
        ride_id: RideId,
        reply: Reply<(RideSummary, u64)>,
    },
    FindRide {
        ride_id: RideId,
        reply: Reply<Option<RideRecord>>,
    },
    FindRideFiltered {
        ride_id: RideId,
        query: RideHistoryQuery,
        reply: Reply<Option<RideRecord>>,
    },
    NewestRecoverableRide {
        reply: Reply<Option<RideRecord>>,
    },
    RideInactivityEvidence {
        ride_id: RideId,
        reply: Reply<Option<RideInactivityEvidence>>,
    },
    ObserveRideAppPresence {
        ride_id: RideId,
        order: RideAppPresenceOrder,
        presence: RideAppPresence,
        observed_at: WallClockUnixMilliseconds,
        reply: Reply<RideInactivityEvidence>,
    },
    ObserveVerifiedDeviceSeen {
        ride_id: RideId,
        observed_at: WallClockUnixMilliseconds,
        reply: Reply<RideInactivityEvidence>,
    },
    RestoreSnapshot {
        reply: Reply<RideRestoreSnapshot>,
    },
    ListRides {
        cursor: Option<RideCursor>,
        limit: QueryLimit,
        query: RideHistoryQuery,
        reply: Reply<RidePage>,
    },
    ProjectHistoryContext {
        query: RideHistoryQuery,
        selected_ride: Option<RideId>,
        budget: HistoryContextBudget,
        viewport: Option<RouteViewport>,
        privacy: RoutePrivacyPolicy,
        cancellation: RouteProjectionCancellation,
        reply: Reply<HistoryContextProjection>,
    },
    ListRideHistoryVehicleOptions {
        reply: Reply<Vec<RideHistoryVehicleOption>>,
    },
    RoutePoints {
        ride_id: RideId,
        cursor: Option<RoutePointCursor>,
        limit: QueryLimit,
        reply: Reply<RoutePointPage>,
    },
    ProjectRoutePoints {
        ride_id: RideId,
        viewport: Option<RouteViewport>,
        budget: RouteDisplayBudget,
        privacy: RoutePrivacyPolicy,
        cancellation: Option<RouteProjectionCancellation>,
        reply: Reply<RoutePointProjection>,
    },
    #[cfg(test)]
    InstallRouteProjectionTestGate {
        entered: SyncSender<()>,
        release: Receiver<()>,
        reply: Reply<()>,
    },
    #[cfg(test)]
    StopForTest {
        reply: Reply<()>,
    },
    Shutdown {
        reply: Reply<()>,
    },
    RideCheckpoint {
        reply: Reply<()>,
    },
    CheckpointStationaryLocationObservation {
        checkpoint: RideStationaryLocationCheckpoint,
        reply: Reply<RideLocationObservationCheckpointOutcome>,
    },
    CheckpointLocationObservation {
        checkpoint: RideLocationObservationCheckpoint,
        reply: Reply<RideLocationObservationCheckpointOutcome>,
    },
}

fn canonical_database_path(path: &Path) -> Result<PathBuf, StorageError> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let parent = absolute.parent().ok_or(StorageError::InvalidPath)?;
    let filename = absolute.file_name().ok_or(StorageError::InvalidPath)?;
    fs::create_dir_all(parent)?;
    Ok(parent.canonicalize()?.join(filename))
}

fn canonical_backup_path(path: &Path) -> Result<PathBuf, StorageError> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let parent = absolute.parent().ok_or(StorageError::InvalidPath)?;
    let filename = absolute.file_name().ok_or(StorageError::InvalidPath)?;
    fs::create_dir_all(parent)?;
    let parent = parent.canonicalize()?;
    let destination = parent.join(filename);
    if destination.exists() {
        return Err(StorageError::InvalidPath);
    }
    Ok(destination)
}

fn configure_connection(
    connection: &mut Connection,
    database_path: &Path,
) -> Result<BootstrapSnapshot, StorageError> {
    connection.pragma_update(None, "foreign_keys", "ON")?;
    let foreign_keys: i64 =
        connection.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
    if foreign_keys != 1 {
        return Err(StorageError::Sqlite(rusqlite::Error::InvalidQuery));
    }
    if !sqlite_capabilities(connection)?.has_rtree() {
        return Err(StorageError::SpatialCapabilityUnavailable);
    }
    let page_count: u64 = connection.pragma_query_value(None, "page_count", |row| row.get(0))?;
    if page_count == 0 {
        connection.pragma_update(None, "page_size", DATABASE_PAGE_SIZE_BYTES)?;
    }
    migrate(connection)?;
    repair_legacy_ride_creation_times(connection)?;
    verify_current_schema(connection)?;
    // Live capture commits every observation. WAL avoids journaling and then
    // overwriting the same database pages for each event. Keep each successful
    // commit synced: NORMAL would weaken the recording durability contract.
    let journal: String =
        connection.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
    if journal != "wal" {
        return Err(StorageError::Sqlite(rusqlite::Error::InvalidQuery));
    }
    connection.pragma_update(None, "synchronous", "FULL")?;
    recover_abandoned_pevcap_imports(connection, database_path)?;
    let recovered_rides = recover_interrupted_rides(connection)?;
    Ok(BootstrapSnapshot {
        recovered_rides: recovered_rides.into(),
    })
}

#[cfg(test)]
mod sqlite_durability_tests {
    use super::{
        Connection, DATABASE_PAGE_SIZE_BYTES, LiveCaptureEventKind, LiveCaptureId,
        configure_connection,
    };

    #[test]
    fn file_database_uses_wal_with_full_commit_durability() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ride.sqlite");
        let mut connection = Connection::open(&path).unwrap();
        // The policy must be explicit on the worker connection, not inherited
        // from a default or measured on an unrelated observer connection.
        connection
            .pragma_update(None, "synchronous", "NORMAL")
            .unwrap();
        configure_connection(&mut connection, &path).unwrap();
        let journal: String = connection
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        let synchronous: u32 = connection
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .unwrap();
        let page_size: u32 = connection
            .pragma_query_value(None, "page_size", |row| row.get(0))
            .unwrap();
        assert_eq!(journal, "wal");
        assert_eq!(synchronous, 2);
        assert_eq!(page_size, DATABASE_PAGE_SIZE_BYTES);
    }

    #[test]
    fn committed_wal_capture_survives_process_exit_and_backup_includes_it() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ride.sqlite");
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "storage::sqlite_durability_tests::capture_process_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("CUTOUT_SQLITE_CRASH_FIXTURE", &path)
            .status()
            .unwrap();
        assert!(child.success());
        assert!(
            std::fs::metadata(path.with_extension("sqlite-wal"))
                .unwrap()
                .len()
                > 0
        );
        let mut connection = Connection::open(&path).unwrap();
        configure_connection(&mut connection, &path).unwrap();
        let state: (u64, u64) = connection.query_row(
            "SELECT next_sequence, (SELECT COUNT(*) FROM live_capture_events) FROM live_capture_sessions",
            [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
        assert_eq!(state, (1, 1));
        super::integrity_check(&connection).unwrap();
        let backup_path = directory.path().join("backup.sqlite");
        super::backup(&connection, &backup_path).unwrap();
        let backup = Connection::open(&backup_path).unwrap();
        assert_eq!(
            backup
                .query_row("SELECT payload FROM live_capture_events", [], |row| row
                    .get::<_, Vec<u8>>(0))
                .unwrap(),
            b"committed"
        );
        super::integrity_check(&backup).unwrap();
    }

    #[test]
    #[ignore = "subprocess fixture invoked by the WAL recovery regression"]
    fn capture_process_fixture() {
        let path =
            std::path::PathBuf::from(std::env::var_os("CUTOUT_SQLITE_CRASH_FIXTURE").unwrap());
        let mut connection = Connection::open(&path).unwrap();
        configure_connection(&mut connection, &path).unwrap();
        let id = LiveCaptureId::new();
        super::live_capture::begin(&connection, id, b"header", 0).unwrap();
        super::live_capture::append(
            &mut connection,
            &super::live_capture::LiveCaptureEventAppend {
                id,
                kind: LiveCaptureEventKind::Notification,
                receipt_monotonic_ms: 1,
                source_monotonic_offset_ms: None,
                source_wall_clock_unix_ms: None,
                payload: b"committed",
                location: None,
                ble_record: None,
            },
        )
        .unwrap();
        // Exit without dropping SQLite, with a later transaction still open.
        // Recovery must retain the completed event and discard this mutation.
        connection
            .execute_batch("BEGIN; UPDATE live_capture_sessions SET next_sequence = 2;")
            .unwrap();
        std::process::exit(0);
    }
}

fn integrity_check(connection: &Connection) -> Result<(), StorageError> {
    let quick_check: String =
        connection.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
    if quick_check != "ok" {
        return Err(StorageError::IntegrityCheckFailed(quick_check));
    }
    Ok(())
}

/// Repairs rides created with a monotonic timestamp before the durable wall-clock boundary was
/// enforced. The update is idempotent and uses the first persisted wall-clock sample when one is
/// available, otherwise the durable update time.
pub(crate) fn repair_legacy_ride_creation_times(
    connection: &mut Connection,
) -> Result<(), StorageError> {
    const MIN_UNIX_MILLISECONDS: u64 = 100_000_000_000;

    let transaction = connection.transaction()?;
    transaction.execute(
        "UPDATE rides
         SET created_at_ms = COALESCE(
             (
                 SELECT MIN(wall_clock_ms)
                 FROM ride_points
                 WHERE ride_points.ride_id = rides.id
                   AND wall_clock_ms >= ?1
                   AND wall_clock_ms <= rides.updated_at_ms
             ),
             updated_at_ms
         )
         WHERE created_at_ms < ?1
           AND updated_at_ms >= ?1",
        [MIN_UNIX_MILLISECONDS],
    )?;
    transaction.execute(
        "UPDATE rides
         SET monotonic_created_at_ms = (
             SELECT MIN(monotonic_ms)
             FROM ride_points
             WHERE ride_points.ride_id = rides.id
         )
         WHERE monotonic_created_at_ms IS NULL
           AND source = 'live'
           AND EXISTS (
               SELECT 1
               FROM ride_points
               WHERE ride_points.ride_id = rides.id
           )",
        [],
    )?;
    transaction.commit()?;
    Ok(())
}

fn recover_abandoned_pevcap_imports(
    connection: &mut Connection,
    database_path: &Path,
) -> Result<(), StorageError> {
    let paths = {
        let mut statement = connection
            .prepare("SELECT artifact_path FROM pevcap_import_work ORDER BY artifact_digest")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let transaction = connection.transaction()?;
    transaction.execute(
        "DELETE FROM rides WHERE source = 'pevcap_import' AND state = 'draft'",
        [],
    )?;
    transaction.execute("DELETE FROM pevcap_import_work", [])?;
    transaction.execute(
        "DELETE FROM pevcap_captures WHERE receipt_digest IS NULL",
        [],
    )?;
    transaction.commit()?;
    let mut managed_directory_name = database_path.as_os_str().to_owned();
    managed_directory_name.push(".pevcap-imports");
    let managed_directory = PathBuf::from(managed_directory_name);
    match fs::symlink_metadata(&managed_directory) {
        Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => return Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(StorageError::Io(error)),
    }
    for path in paths {
        let path = Path::new(&path);
        if path.parent() != Some(managed_directory.as_path()) {
            continue;
        }
        if let Err(error) = fs::remove_file(path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(StorageError::Io(error));
        }
    }
    for entry in fs::read_dir(&managed_directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(digest) = name
            .strip_prefix(".stage-")
            .and_then(|name| name.strip_suffix(".tmp"))
        else {
            continue;
        };
        if digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

fn recover_interrupted_rides(connection: &mut Connection) -> Result<Vec<RideId>, StorageError> {
    let transaction = connection.transaction()?;
    let recovered = {
        let mut statement = transaction
            .prepare("SELECT id FROM rides WHERE state IN ('active', 'paused') ORDER BY id")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut recovered = Vec::new();
        for row in rows {
            let value = row?;
            let id = Uuid::parse_str(&value).map_err(|_| StorageError::InvalidStoredValue {
                field: "ride identifier",
                value,
            })?;
            recovered.push(RideId::from_uuid(id));
        }
        recovered
    };
    transaction.execute(
        "UPDATE rides
         SET state = 'interrupted',
             completed_duration_ms = CASE
                 WHEN monotonic_created_at_ms IS NOT NULL
                     AND monotonic_last_event_ms IS NOT NULL
                     THEN MAX(0, monotonic_last_event_ms - monotonic_created_at_ms
                         - paused_duration_ms
                         - CASE WHEN state = 'paused' AND paused_at_ms IS NOT NULL
                             THEN monotonic_last_event_ms - paused_at_ms ELSE 0 END)
                 ELSE completed_duration_ms
             END
         WHERE state IN ('active', 'paused')",
        [],
    )?;
    for ride_id in &recovered {
        let checkpoint = load_moving_time_checkpoint(&transaction, *ride_id)?;
        let parts = checkpoint.continuation.state_parts();
        let moving_changed = parts.cursor_milliseconds.is_some()
            || parts.phone_gps.is_some()
            || parts.vehicle.is_some()
            || parts.latest_phone_gps_source_milliseconds.is_some()
            || parts.latest_vehicle_source_milliseconds.is_some();
        let elapsed_parts = checkpoint.elapsed_time.state_parts();
        let elapsed_changed = elapsed_parts.running || elapsed_parts.anchor_milliseconds.is_some();
        if moving_changed || elapsed_changed {
            let mut continuation = checkpoint.continuation;
            if moving_changed {
                let at = transaction.query_row(
                    "SELECT COALESCE(monotonic_last_event_ms, 0) FROM rides WHERE id = ?1",
                    [ride_id.uuid().to_string()],
                    |row| row.get::<_, u64>(0),
                )?;
                continuation = MovingTimeContinuation::restored_after_restart(
                    parts,
                    MonotonicMilliseconds::new(at),
                )
                .map_err(moving_time_storage_error)?;
            }
            let revision = checkpoint.revision.checked_add(1).ok_or_else(|| {
                StorageError::InvalidStoredValue {
                    field: "moving-time revision",
                    value: checkpoint.revision.to_string(),
                }
            })?;
            save_moving_time_checkpoint(
                &transaction,
                *ride_id,
                revision,
                continuation,
                checkpoint.elapsed_time.restored_after_restart(),
                None,
                None,
            )?;
        }
    }
    transaction.commit()?;
    Ok(recovered)
}

fn sqlite_capabilities(connection: &Connection) -> Result<SqliteCapabilities, StorageError> {
    let version_string: String =
        connection.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
    let version = SqliteVersion::parse(&version_string)?;
    let mut statement = connection.prepare("PRAGMA compile_options")?;
    let options = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut rtree = false;
    let mut fts5 = false;
    for option in options {
        match option?.as_str() {
            "ENABLE_RTREE" => rtree = true,
            "ENABLE_FTS5" => fts5 = true,
            _ => {}
        }
    }
    Ok(SqliteCapabilities {
        version,
        rtree,
        fts5,
    })
}

fn backup(connection: &Connection, destination: &Path) -> Result<(), StorageError> {
    connection.execute("VACUUM INTO ?1", params![destination.to_string_lossy()])?;
    Ok(())
}

fn export_ride_json(
    connection: &Connection,
    ride_id: RideId,
    destination: &Path,
) -> Result<(), StorageError> {
    let (source, state, created_at_ms, updated_at_ms, point_count, distance_mm): (
        String,
        String,
        u64,
        u64,
        u64,
        u64,
    ) = connection
        .query_row(
            "SELECT source, state, created_at_ms, updated_at_ms, point_count, distance_mm
             FROM rides WHERE id = ?1",
            params![ride_id.uuid().to_string()],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()?
        .ok_or(StorageError::NotFound)?;
    let music_history = export_music_history(connection, ride_id)?;
    let json = serde_json::json!({
        "schema_version": 2,
        "ride_id": ride_id.uuid().to_string(),
        "source": source,
        "state": state,
        "created_at_ms": created_at_ms,
        "updated_at_ms": updated_at_ms,
        "point_count": point_count,
        "distance_mm": distance_mm,
        "music_history": music_history,
    })
    .to_string();
    fs::write(destination, json)?;
    Ok(())
}

fn export_music_history(
    connection: &Connection,
    ride_id: RideId,
) -> Result<serde_json::Value, StorageError> {
    let state = music_history_state(connection, ride_id)?;
    let events = match state {
        MusicHistoryState::HumanReadable | MusicHistoryState::Redacted => {
            let mut statement = connection.prepare(
                "SELECT sequence, provider, item_identifier, title, artist, kind,
                        monotonic_at_ms, wall_clock_at_ms, clock_uncertainty_milliseconds
                 FROM ride_music_event WHERE ride_id = ?1 ORDER BY sequence",
            )?;
            statement
                .query_map([ride_id.uuid().to_string()], |row| {
                    let item_identifier = row.get::<_, Option<String>>(2)?;
                    let display_metadata = state == MusicHistoryState::HumanReadable;
                    let title = if display_metadata {
                        row.get::<_, Option<String>>(3)?
                    } else {
                        None
                    };
                    let artist = if display_metadata {
                        row.get::<_, Option<String>>(4)?
                    } else {
                        None
                    };
                    Ok(serde_json::json!({
                        "sequence": row.get::<_, u64>(0)?,
                        "provider": row.get::<_, String>(1)?,
                        "item_identifier": item_identifier,
                        "title": title,
                        "artist": artist,
                        "kind": row.get::<_, String>(5)?,
                        "monotonic_at_ms": row.get::<_, u64>(6)?,
                        "wall_clock_at_ms": row.get::<_, u64>(7)?,
                        "clock_uncertainty_milliseconds": row.get::<_, u64>(8)?,
                    }))
                })?
                .collect::<Result<Vec<_>, _>>()?
        }
        MusicHistoryState::Missing | MusicHistoryState::Disabled | MusicHistoryState::Deleted => {
            Vec::new()
        }
    };
    Ok(serde_json::json!({
        "state": export_music_history_state_name(state),
        "events": events,
    }))
}

fn export_music_history_state_name(state: MusicHistoryState) -> &'static str {
    match state {
        MusicHistoryState::Missing => "missing",
        MusicHistoryState::Disabled => "disabled",
        MusicHistoryState::Redacted => "redacted",
        MusicHistoryState::HumanReadable => "human_readable",
        MusicHistoryState::Deleted => "deleted",
    }
}

fn create_ride(
    connection: &Connection,
    source: RideSource,
    created_at_ms: u64,
    monotonic_created_at_ms: Option<u64>,
) -> Result<RideId, StorageError> {
    let ride_id = RideId::new();
    connection.execute(
        "INSERT INTO rides
            (id, source, state, created_at_ms, monotonic_created_at_ms, updated_at_ms, point_count, distance_mm)
         VALUES (?1, ?2, 'draft', ?3, ?4, ?3, 0, 0)",
        params![
            ride_id.uuid().to_string(),
            source.as_db(),
            created_at_ms,
            monotonic_created_at_ms
        ],
    )?;
    Ok(ride_id)
}

fn create_started_live_ride(
    connection: &mut Connection,
    created_at_ms: u64,
    monotonic_created_at_ms: u64,
    candidate_vehicle: Option<&str>,
    music_history_policy: Option<MusicHistoryPolicy>,
) -> Result<CommittedRideMovingTimeCheckpoint, StorageError> {
    let transaction = connection.transaction()?;
    let ride_id = create_started_live_ride_in_transaction(
        &transaction,
        created_at_ms,
        monotonic_created_at_ms,
        candidate_vehicle,
    )?;
    if let Some(policy) = music_history_policy {
        apply_music_history_policy(&transaction, ride_id, policy)?;
    }
    let committed =
        committed_ride_moving_time_checkpoint(&transaction, ride_id, RideLifecycleState::Active)?;
    transaction.commit()?;
    Ok(committed)
}

fn committed_ride_moving_time_checkpoint(
    connection: &Connection,
    ride_id: RideId,
    lifecycle: RideLifecycleState,
) -> Result<CommittedRideMovingTimeCheckpoint, StorageError> {
    Ok(CommittedRideMovingTimeCheckpoint {
        ride_id,
        lifecycle,
        checkpoint: load_moving_time_checkpoint(connection, ride_id)?,
    })
}

#[allow(
    clippy::too_many_lines,
    reason = "recovery settlement keeps finalization, prefix restoration, and replacement atomic"
)]
fn settle_recovered_ride(
    connection: &mut Connection,
    ride_id: RideId,
    discard_empty: bool,
    occurred_at_ms: u64,
    monotonic_at_ms: u64,
    replacement_candidate_vehicle: Option<&str>,
) -> Result<Option<CommittedRideMovingTimeCheckpoint>, StorageError> {
    let transaction = connection.transaction()?;
    let committed = settle_recovered_ride_in_transaction(
        &transaction,
        ride_id,
        discard_empty,
        occurred_at_ms,
        monotonic_at_ms,
        replacement_candidate_vehicle,
    )?;
    transaction.commit()?;
    Ok(committed)
}

fn settle_inactive_live_ride(
    connection: &mut Connection,
    ride_id: RideId,
    expected_evidence: RideInactivityEvidence,
    occurred_at_ms: u64,
    monotonic_at_ms: u64,
    replacement_candidate_vehicle: Option<&str>,
) -> Result<Option<CommittedRideMovingTimeCheckpoint>, StorageError> {
    let transaction = connection.transaction()?;
    let ride = find_ride(&transaction, ride_id)?.ok_or(StorageError::NotFound)?;
    let eligibility = if ride
        .motion_observation()
        .is_some_and(|motion| motion.parked)
    {
        RideInactivityEligibility::VerifiedParked
    } else {
        RideInactivityEligibility::NotCanonicallyParked
    };
    if ride.state() != RideLifecycleState::Active
        || ride.source() != RideSource::Live
        || expected_evidence.app_inactive_since().is_none()
        || ride_inactivity::read(&transaction, ride_id)?.unwrap_or_default() != expected_evidence
        || expected_evidence.inactivity_recovery_decision(
            WallClockUnixMilliseconds::new(occurred_at_ms),
            eligibility,
        ) != Some(RideRecoveryDecision::Finalize)
    {
        return Err(StorageError::RideInactivityConflict);
    }

    let discard_empty = ride.summary().point_count().as_u64() == 0
        && ride.last_telemetry_at_milliseconds().is_none();
    if !discard_empty {
        transition_ride(
            &transaction,
            ride_id,
            RideEvent::Interrupt,
            occurred_at_ms,
            Some(monotonic_at_ms),
        )?;
    }
    let committed = settle_recovered_ride_in_transaction(
        &transaction,
        ride_id,
        discard_empty,
        occurred_at_ms,
        monotonic_at_ms,
        replacement_candidate_vehicle,
    )?;
    transaction.commit()?;
    Ok(committed)
}

#[allow(
    clippy::too_many_lines,
    reason = "recovery settlement keeps finalization, prefix restoration, and replacement atomic"
)]
fn settle_recovered_ride_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    ride_id: RideId,
    discard_empty: bool,
    occurred_at_ms: u64,
    monotonic_at_ms: u64,
    replacement_candidate_vehicle: Option<&str>,
) -> Result<Option<CommittedRideMovingTimeCheckpoint>, StorageError> {
    if discard_empty {
        transition_ride(
            transaction,
            ride_id,
            RideEvent::Discard,
            occurred_at_ms,
            Some(monotonic_at_ms),
        )?;
    } else {
        let write_state = load_ride_write_state(transaction, ride_id)?;
        let interrupted_ride = if write_state.lifecycle() == RideLifecycleState::Interrupted {
            find_ride(transaction, ride_id)?
        } else {
            None
        };
        let interrupted_moving_prefix = interrupted_ride
            .as_ref()
            .map(RideRecord::last_recorded_point_moving_time);
        let interrupted_elapsed_prefix =
            if write_state.lifecycle() == RideLifecycleState::Interrupted {
                if write_state.paused_at_milliseconds().is_some() {
                    Some(
                        load_moving_time_checkpoint(transaction, ride_id)?
                            .elapsed_time
                            .summary(),
                    )
                } else {
                    interrupted_ride
                        .as_ref()
                        .map(RideRecord::last_recorded_point_elapsed_time)
                }
            } else {
                None
            };
        let recovered_point_duration_ms = if write_state.lifecycle()
            == RideLifecycleState::Interrupted
            && write_state.paused_at_milliseconds().is_none()
        {
            write_state.last_recorded_point_duration_ms()
        } else {
            None
        };
        if let Some(point_duration_ms) = recovered_point_duration_ms
            && point_duration_ms > write_state.completed_duration_ms()
        {
            return Err(StorageError::InvalidStoredValue {
                field: "rides.last_recorded_point_duration_ms",
                value: point_duration_ms.to_string(),
            });
        }
        // Database-open recovery stores an explicit pause as Interrupted so the mobile core can
        // decide whether the 24-hour policy has expired. Once it expires, settle it through the
        // effective Paused lifecycle: Stop closes the pause timing, then Save publishes history.
        if write_state.lifecycle() == RideLifecycleState::Interrupted
            && write_state.paused_at_milliseconds().is_some()
        {
            transition_ride(
                transaction,
                ride_id,
                RideEvent::Stop,
                occurred_at_ms,
                Some(monotonic_at_ms),
            )?;
        }
        transition_ride(
            transaction,
            ride_id,
            RideEvent::Save,
            occurred_at_ms,
            Some(monotonic_at_ms),
        )?;
        if let Some(point_duration_ms) = recovered_point_duration_ms {
            transaction.execute(
                "UPDATE rides SET completed_duration_ms = ?1 WHERE id = ?2 AND state = 'saved'",
                params![point_duration_ms, ride_id.uuid().to_string()],
            )?;
        }
        if let Some(prefix) = interrupted_moving_prefix {
            let checkpoint = load_moving_time_checkpoint(transaction, ride_id)?;
            let mut parts = checkpoint.continuation.state_parts();
            parts.moving_milliseconds = prefix.moving.as_u64();
            parts.observed_milliseconds = prefix.observed.as_u64();
            parts.unknown_milliseconds = prefix.unknown.as_u64();
            parts.cursor_milliseconds = None;
            parts.phone_gps = None;
            parts.vehicle = None;
            parts.latest_phone_gps_source_milliseconds = None;
            parts.latest_vehicle_source_milliseconds = None;
            parts.availability = prefix.availability;
            parts.incomplete_history = prefix.availability != MovingTimeAvailability::Complete;
            let continuation = MovingTimeContinuation::from_state_parts(parts)
                .map_err(moving_time_storage_error)?;
            let revision = checkpoint.revision.checked_add(1).ok_or_else(|| {
                StorageError::InvalidStoredValue {
                    field: "moving-time revision",
                    value: checkpoint.revision.to_string(),
                }
            })?;
            save_moving_time_checkpoint(
                transaction,
                ride_id,
                revision,
                continuation,
                interrupted_elapsed_prefix.map_or(
                    checkpoint.elapsed_time,
                    ElapsedTimeContinuation::from_summary,
                ),
                None,
                None,
            )?;
        }
    }
    let replacement = replacement_candidate_vehicle.map(|candidate| {
        create_started_live_ride_in_transaction(
            transaction,
            occurred_at_ms,
            monotonic_at_ms,
            Some(candidate),
        )
    });
    let replacement = match replacement {
        Some(result) => Some(result?),
        None => None,
    };
    let has_replacement = replacement.is_some();
    let result_ride = replacement.or_else(|| (!discard_empty).then_some(ride_id));
    let result_lifecycle = if has_replacement {
        RideLifecycleState::Active
    } else if discard_empty {
        RideLifecycleState::Discarded
    } else {
        RideLifecycleState::Saved
    };
    let committed = result_ride
        .map(|result_ride_id| {
            committed_ride_moving_time_checkpoint(transaction, result_ride_id, result_lifecycle)
        })
        .transpose()?;
    Ok(committed)
}

fn create_started_live_ride_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    created_at_ms: u64,
    monotonic_created_at_ms: u64,
    candidate_vehicle: Option<&str>,
) -> Result<RideId, StorageError> {
    let ride_id = RideId::new();
    transaction.execute(
        "INSERT INTO rides
            (id, source, state, created_at_ms, monotonic_created_at_ms, monotonic_last_event_ms,
             updated_at_ms, point_count, distance_mm, candidate_vehicle, moving_time_cursor_ms,
             elapsed_time_ms, elapsed_time_anchor_ms, elapsed_time_running)
         VALUES (?1, 'live', 'active', ?2, ?3, ?3, ?2, 0, 0, ?4, ?3, 0, ?3, 1)",
        params![
            ride_id.uuid().to_string(),
            created_at_ms,
            monotonic_created_at_ms,
            candidate_vehicle,
        ],
    )?;
    Ok(ride_id)
}

fn create_started_ride(
    connection: &mut Connection,
    source: RideSource,
    created_at_ms: u64,
    monotonic_created_at_ms: Option<u64>,
    candidate_vehicle: Option<&str>,
    occurred_at_ms: u64,
) -> Result<RideId, StorageError> {
    let transaction = connection.transaction()?;
    let ride_id = create_ride(&transaction, source, created_at_ms, monotonic_created_at_ms)?;
    transition_ride(
        &transaction,
        ride_id,
        RideEvent::Start,
        occurred_at_ms,
        monotonic_created_at_ms,
    )?;
    update_ride_map_metadata(&transaction, ride_id, candidate_vehicle, None, None, None)?;
    transaction.commit()?;
    Ok(ride_id)
}

fn update_ride_map_metadata(
    connection: &Connection,
    ride_id: RideId,
    candidate_vehicle: Option<&str>,
    associated_vehicle: Option<&str>,
    associated_at_ms: Option<u64>,
    last_telemetry_at_ms: Option<u64>,
) -> Result<(), StorageError> {
    update_ride_map_metadata_with_motion(
        connection,
        ride_id,
        candidate_vehicle,
        associated_vehicle,
        associated_at_ms,
        last_telemetry_at_ms,
        None,
    )
    .map(|_| ())
}

fn update_ride_map_metadata_with_motion(
    connection: &Connection,
    ride_id: RideId,
    candidate_vehicle: Option<&str>,
    associated_vehicle: Option<&str>,
    associated_at_ms: Option<u64>,
    last_telemetry_at_ms: Option<u64>,
    motion: Option<RideMotionObservation>,
) -> Result<MovingTimeCheckpoint, StorageError> {
    update_ride_map_metadata_with_moving_time_in_transaction(
        connection,
        ride_id,
        candidate_vehicle,
        associated_vehicle,
        associated_at_ms,
        last_telemetry_at_ms,
        motion,
        None,
        None,
        None,
    )
}

#[allow(
    clippy::too_many_arguments,
    reason = "this wrapper forwards the complete metadata transaction payload"
)]
fn update_ride_map_metadata_with_moving_time(
    connection: &mut Connection,
    ride_id: RideId,
    candidate_vehicle: Option<&str>,
    associated_vehicle: Option<&str>,
    associated_at_ms: Option<u64>,
    last_telemetry_at_ms: Option<u64>,
    motion: Option<RideMotionObservation>,
    moving_time_observation: Option<MovingTimeObservation>,
    inactivity_evidence: Option<RideInactivityEvidence>,
) -> Result<MovingTimeCheckpoint, StorageError> {
    let transaction = connection.transaction()?;
    update_ride_map_metadata_with_moving_time_in_transaction(
        &transaction,
        ride_id,
        candidate_vehicle,
        associated_vehicle,
        associated_at_ms,
        last_telemetry_at_ms,
        motion,
        moving_time_observation,
        None,
        inactivity_evidence,
    )?;
    transaction.commit()?;
    load_moving_time_checkpoint(connection, ride_id)
}

#[allow(
    clippy::too_many_arguments,
    reason = "the metadata SQL update forwards its complete transaction payload"
)]
fn update_ride_map_metadata_with_moving_time_in_transaction(
    connection: &Connection,
    ride_id: RideId,
    candidate_vehicle: Option<&str>,
    associated_vehicle: Option<&str>,
    associated_at_ms: Option<u64>,
    last_telemetry_at_ms: Option<u64>,
    motion: Option<RideMotionObservation>,
    moving_time_observation: Option<MovingTimeObservation>,
    moving_time_source_boundary_at_ms: Option<MonotonicMilliseconds>,
    inactivity_evidence: Option<RideInactivityEvidence>,
) -> Result<MovingTimeCheckpoint, StorageError> {
    validate_ride_metadata_moving_time(
        associated_vehicle,
        associated_at_ms,
        last_telemetry_at_ms,
        motion,
        moving_time_observation,
        moving_time_source_boundary_at_ms,
    )?;
    let (previous_state, previous_associated_vehicle) = update_ride_metadata_row_in_transaction(
        connection,
        ride_id,
        candidate_vehicle,
        associated_vehicle,
        associated_at_ms,
        last_telemetry_at_ms,
        motion,
    )?;
    if let Some(evidence) = inactivity_evidence {
        ride_inactivity::persist_snapshot(connection, ride_id, evidence)?;
    }
    let current = load_moving_time_checkpoint(connection, ride_id)?;
    let association_changed = previous_associated_vehicle.as_deref() != associated_vehicle;
    let is_active = previous_state == RideLifecycleState::Active;
    let elapsed_receipt_at = moving_time_observation
        .map(|observation| observation.receipt_at)
        .into_iter()
        .chain(moving_time_source_boundary_at_ms)
        .chain(motion.map(|value| MonotonicMilliseconds::new(value.observed_at_milliseconds)))
        .chain(last_telemetry_at_ms.map(MonotonicMilliseconds::new))
        .chain(associated_at_ms.map(MonotonicMilliseconds::new))
        .max();
    let inferred_boundary = if is_active && association_changed {
        moving_time_observation
            .map(|observation| observation.receipt_at)
            .or_else(|| {
                motion.map(|value| MonotonicMilliseconds::new(value.observed_at_milliseconds))
            })
            .or_else(|| last_telemetry_at_ms.map(MonotonicMilliseconds::new))
            .or_else(|| associated_at_ms.map(MonotonicMilliseconds::new))
            .or_else(|| {
                current
                    .continuation
                    .state_parts()
                    .cursor_milliseconds
                    .map(MonotonicMilliseconds::new)
            })
    } else {
        None
    };
    let source_boundary = if is_active {
        if let Some(boundary) = moving_time_source_boundary_at_ms {
            Some(boundary)
        } else if association_changed {
            Some(
                inferred_boundary.ok_or_else(|| StorageError::InvalidStoredValue {
                    field: "vehicle association boundary",
                    value: "active association changed without a monotonic receipt".to_owned(),
                })?,
            )
        } else {
            None
        }
    } else {
        None
    };
    commit_metadata_moving_time_checkpoint(
        connection,
        ride_id,
        current,
        previous_state,
        source_boundary,
        moving_time_observation,
        elapsed_receipt_at,
    )
}

#[allow(
    clippy::too_many_arguments,
    reason = "metadata columns are updated as one validated snapshot"
)]
fn update_ride_metadata_row_in_transaction(
    connection: &Connection,
    ride_id: RideId,
    candidate_vehicle: Option<&str>,
    associated_vehicle: Option<&str>,
    associated_at_ms: Option<u64>,
    last_telemetry_at_ms: Option<u64>,
    motion: Option<RideMotionObservation>,
) -> Result<(RideLifecycleState, Option<String>), StorageError> {
    let previous_metadata: Option<(String, Option<String>)> = connection
        .query_row(
            "SELECT state, associated_vehicle FROM rides WHERE id = ?1",
            [ride_id.uuid().to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (previous_state, previous_associated_vehicle) =
        previous_metadata.ok_or(StorageError::NotFound)?;
    let previous_state = state_from_db(&previous_state)?;
    let changed = connection.execute(
        "UPDATE rides SET candidate_vehicle = ?2, associated_vehicle = ?3, associated_at_ms = ?4,
             last_telemetry_at_ms = ?5,
             motion_parked = COALESCE(?6, motion_parked),
             motion_observed_monotonic_ms = COALESCE(?7, motion_observed_monotonic_ms),
             last_stationary_motion_monotonic_ms = COALESCE(?8, last_stationary_motion_monotonic_ms)
         WHERE id = ?1 AND (?7 IS NULL OR motion_observed_monotonic_ms IS NULL
             OR motion_observed_monotonic_ms < ?7
             OR (motion_observed_monotonic_ms = ?7 AND motion_parked = ?6))
         AND (?8 IS NULL OR last_stationary_motion_monotonic_ms IS NULL
             OR last_stationary_motion_monotonic_ms <= ?8)",
        params![
            ride_id.uuid().to_string(),
            candidate_vehicle,
            associated_vehicle,
            associated_at_ms,
            last_telemetry_at_ms,
            motion.map(|value| value.parked),
            motion.map(|value| value.observed_at_milliseconds),
            motion.and_then(|value| value.last_stationary_at_milliseconds)
        ],
    )?;
    if changed == 0 {
        let exists = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM rides WHERE id = ?1)",
            [ride_id.uuid().to_string()],
            |row| row.get::<_, bool>(0),
        )?;
        if !exists {
            return Err(StorageError::NotFound);
        }
        return Err(StorageError::InvalidStoredValue {
            field: "wheel motion evidence",
            value: "out of order".to_owned(),
        });
    }
    Ok((previous_state, previous_associated_vehicle))
}

fn validate_ride_metadata_moving_time(
    associated_vehicle: Option<&str>,
    associated_at_ms: Option<u64>,
    last_telemetry_at_ms: Option<u64>,
    motion: Option<RideMotionObservation>,
    moving_time_observation: Option<MovingTimeObservation>,
    moving_time_source_boundary_at_ms: Option<MonotonicMilliseconds>,
) -> Result<(), StorageError> {
    if moving_time_source_boundary_at_ms.is_some_and(|boundary| {
        associated_vehicle.is_none()
            || associated_at_ms.is_none_or(|associated_at| associated_at > boundary.as_u64())
    }) {
        return Err(StorageError::InvalidStoredValue {
            field: "moving-time source boundary",
            value: "requires a confirmed association at or before the boundary".to_owned(),
        });
    }
    if motion.is_some_and(|motion| {
        associated_vehicle.is_none()
            || last_telemetry_at_ms.is_none_or(|at| at < motion.observed_at_milliseconds)
            || associated_at_ms.is_none_or(|at| at > motion.observed_at_milliseconds)
            || motion
                .last_stationary_at_milliseconds
                .is_some_and(|at| at > motion.observed_at_milliseconds)
            || (motion.parked
                && motion.last_stationary_at_milliseconds != Some(motion.observed_at_milliseconds))
    }) {
        return Err(StorageError::InvalidStoredValue {
            field: "wheel motion evidence",
            value: "unassociated or non-current telemetry".to_owned(),
        });
    }
    if moving_time_observation.is_some_and(|observation| {
        observation.source != MovingTimeSource::Vehicle
            || observation.speed_millimetres_per_second.is_none()
            || associated_vehicle.is_none()
            || associated_at_ms.is_none_or(|at| at > observation.source_at.as_u64())
            || last_telemetry_at_ms.is_none_or(|at| at < observation.source_at.as_u64())
    }) {
        return Err(StorageError::InvalidStoredValue {
            field: "verified vehicle moving-time observation",
            value: "unassociated or inconsistent source time".to_owned(),
        });
    }
    Ok(())
}

fn commit_metadata_moving_time_checkpoint(
    connection: &Connection,
    ride_id: RideId,
    current: MovingTimeCheckpoint,
    lifecycle: RideLifecycleState,
    source_boundary: Option<MonotonicMilliseconds>,
    moving_time_observation: Option<MovingTimeObservation>,
    elapsed_receipt_at: Option<MonotonicMilliseconds>,
) -> Result<MovingTimeCheckpoint, StorageError> {
    let mut continuation = current.continuation;
    let mut elapsed_time = current.elapsed_time;
    let is_active = lifecycle == RideLifecycleState::Active;
    if let Some(boundary) = source_boundary {
        continuation
            .close_at(boundary)
            .map_err(moving_time_storage_error)?;
        continuation
            .resume_at(boundary)
            .map_err(moving_time_storage_error)?;
    }
    if is_active && let Some(observation) = moving_time_observation {
        apply_metadata_vehicle_observation(&mut continuation, observation)?;
    }
    if let Some(receipt_at) = elapsed_receipt_at {
        if elapsed_time.state_parts().running {
            elapsed_time.observe(receipt_at);
        } else if lifecycle == RideLifecycleState::Active || lifecycle == RideLifecycleState::Paused
        {
            elapsed_time.resumed(receipt_at);
        }
    }
    let should_commit_checkpoint = source_boundary.is_some()
        || (is_active && moving_time_observation.is_some())
        || elapsed_time != current.elapsed_time;
    if !should_commit_checkpoint {
        return load_moving_time_checkpoint(connection, ride_id);
    }
    let revision =
        current
            .revision
            .checked_add(1)
            .ok_or_else(|| StorageError::InvalidStoredValue {
                field: "moving-time revision",
                value: current.revision.to_string(),
            })?;
    save_moving_time_checkpoint(
        connection,
        ride_id,
        revision,
        continuation,
        elapsed_time,
        None,
        None,
    )?;
    load_moving_time_checkpoint(connection, ride_id)
}

fn apply_metadata_vehicle_observation(
    continuation: &mut MovingTimeContinuation,
    observation: MovingTimeObservation,
) -> Result<(), StorageError> {
    let parts = continuation.state_parts();
    if parts.latest_vehicle_source_milliseconds == Some(observation.source_at.as_u64()) {
        if parts.vehicle.is_some_and(|value| {
            value.source_at_milliseconds == observation.source_at.as_u64()
                && Some(value.speed_millimetres_per_second)
                    == observation.speed_millimetres_per_second
        }) {
            // The same current wheel sample may be repeated by metadata checkpoints.
            return Ok(());
        }
        return Err(StorageError::InvalidStoredValue {
            field: "verified vehicle moving-time observation",
            value: "repeated source time changed its speed".to_owned(),
        });
    }
    if parts
        .latest_vehicle_source_milliseconds
        .is_some_and(|latest| latest > observation.source_at.as_u64())
    {
        // A late metadata completion cannot replace newer vehicle evidence.
        return Ok(());
    }
    if moving_time_observation_fresh(observation) {
        continuation
            .observe(observation)
            .map_err(moving_time_storage_error)?;
    }
    Ok(())
}

fn record_bms_voltage_samples(
    connection: &mut Connection,
    samples: &[BmsVoltageSampleRecord],
) -> Result<(), StorageError> {
    if samples.is_empty() {
        return Ok(());
    }
    let transaction = connection.transaction()?;
    {
        let mut insert = transaction.prepare_cached(
            "INSERT OR IGNORE INTO bms_voltage_samples
                (device_identity, session_identifier, event_sequence, monotonic_ms, wall_clock_ms, observation_index,
                 pack_index, pack_observation_index, millivolts)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;
        for sample in samples {
            insert.execute(params![
                sample.device_identity,
                sample.session_identifier,
                bms_sqlite_integer(sample.event_sequence, "BMS event sequence")?,
                bms_sqlite_integer(sample.monotonic_milliseconds, "BMS monotonic timestamp")?,
                bms_sqlite_integer(sample.wall_clock_milliseconds, "BMS wall clock timestamp")?,
                sample.observation_index,
                sample.pack_index,
                sample.pack_observation_index,
                sample.millivolts,
            ])?;
        }
    }
    transaction.commit()?;
    Ok(())
}

fn bms_sqlite_integer(value: u64, field: &'static str) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::InvalidStoredValue {
        field,
        value: value.to_string(),
    })
}

fn ensure_spatial_schema(
    connection: &Connection,
    state: &mut SpatialSchemaState,
) -> Result<(), StorageError> {
    match state {
        SpatialSchemaState::Ready => return Ok(()),
        SpatialSchemaState::Unavailable => return Err(StorageError::SpatialCapabilityUnavailable),
        SpatialSchemaState::Failed(message) => {
            return Err(StorageError::SpatialSchemaInitialization(message.clone()));
        }
        SpatialSchemaState::Uninitialized => {}
    }
    if !sqlite_capabilities(connection)?.has_rtree() {
        *state = SpatialSchemaState::Unavailable;
        return Err(StorageError::SpatialCapabilityUnavailable);
    }
    for table in [
        "trails",
        "trail_segments",
        "trail_segment_spatial_keys",
        "trail_segments_rtree",
        "map_points",
        "map_point_spatial_keys",
        "map_points_rtree",
    ] {
        if !table_exists(connection, table)? {
            let message = format!("missing migrated table {table}");
            *state = SpatialSchemaState::Failed(message.clone());
            return Err(StorageError::SpatialSchemaInitialization(message));
        }
    }
    *state = SpatialSchemaState::Ready;
    Ok(())
}

fn create_trail(
    connection: &Connection,
    state: &mut SpatialSchemaState,
    name: &str,
) -> Result<TrailId, StorageError> {
    if name.trim().is_empty() {
        return Err(StorageError::InvalidStoredValue {
            field: "trail name",
            value: "empty".to_owned(),
        });
    }
    ensure_spatial_schema(connection, state)?;
    let trail_id = TrailId::new();
    connection.execute(
        "INSERT INTO trails (id, name) VALUES (?1, ?2)",
        params![trail_id.uuid().to_string(), name],
    )?;
    Ok(trail_id)
}

fn append_trail_segment(
    connection: &mut Connection,
    state: &mut SpatialSchemaState,
    trail_id: TrailId,
    sequence: u32,
    start: Coordinate,
    end: Coordinate,
) -> Result<(), StorageError> {
    ensure_spatial_schema(connection, state)?;
    let transaction = connection.transaction()?;
    let trail = trail_id.uuid().to_string();
    let exists: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM trails WHERE id = ?1)",
        params![trail],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(StorageError::NotFound);
    }
    transaction.execute(
        "INSERT INTO trail_segments
            (trail_id, sequence, start_lat_e7, start_lon_e7, end_lat_e7, end_lon_e7)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            trail,
            sequence,
            start.latitude().as_i32(),
            start.longitude().as_i32(),
            end.latitude().as_i32(),
            end.longitude().as_i32(),
        ],
    )?;
    let rtree_id = transaction.query_row(
        "INSERT INTO trail_segment_spatial_keys (trail_id, sequence)
         VALUES (?1, ?2)
         RETURNING rtree_id",
        params![trail, sequence],
        |row| row.get::<_, i64>(0),
    )?;
    let rtree_id = SpatialRowId::from_sqlite(rtree_id)?;
    let min_lat = start.latitude().as_i32().min(end.latitude().as_i32());
    let max_lat = start.latitude().as_i32().max(end.latitude().as_i32());
    let min_lon = start.longitude().as_i32().min(end.longitude().as_i32());
    let max_lon = start.longitude().as_i32().max(end.longitude().as_i32());
    transaction.execute(
        "INSERT INTO trail_segments_rtree
            (id, min_lat_e7, max_lat_e7, min_lon_e7, max_lon_e7)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![rtree_id.get(), min_lat, max_lat, min_lon, max_lon],
    )?;
    transaction.commit()?;
    Ok(())
}

fn trail_segments_in_bounds(
    connection: &Connection,
    state: &mut SpatialSchemaState,
    bounds: GeoBounds,
    cursor: Option<TrailSegmentCursor>,
    limit: QueryLimit,
) -> Result<TrailSegmentPage, StorageError> {
    ensure_spatial_schema(connection, state)?;
    let mut statement = connection.prepare(
        "SELECT s.trail_id, s.sequence, s.start_lat_e7, s.start_lon_e7,
                s.end_lat_e7, s.end_lon_e7
         FROM trail_segments_rtree r
         JOIN trail_segment_spatial_keys k ON k.rtree_id = r.id
         JOIN trail_segments s ON s.trail_id = k.trail_id AND s.sequence = k.sequence
         WHERE r.max_lat_e7 >= ?1 AND r.min_lat_e7 <= ?2
           AND ((?5 = 0 AND r.max_lon_e7 >= ?3 AND r.min_lon_e7 <= ?4)
             OR (?5 = 1 AND (r.max_lon_e7 >= ?3 OR r.min_lon_e7 <= ?4)))
           AND (?6 IS NULL OR s.trail_id > ?6 OR (s.trail_id = ?6 AND s.sequence > ?7))
         ORDER BY s.trail_id, s.sequence
         LIMIT ?8",
    )?;
    let cursor_trail = cursor.map(|cursor| cursor.trail_id.uuid().to_string());
    let cursor_sequence = cursor.map(|cursor| cursor.sequence);
    let rows = statement.query_map(
        params![
            bounds.minimum_latitude,
            bounds.maximum_latitude,
            bounds.minimum_longitude,
            bounds.maximum_longitude,
            bounds.crosses_antimeridian(),
            cursor_trail,
            cursor_sequence,
            i64::from(limit.get()) + 1,
        ],
        trail_segment_from_row,
    )?;
    let mut segments = rows.collect::<Result<Vec<_>, _>>()?;
    let has_more = segments.len() > limit.get() as usize;
    if has_more {
        segments.pop();
    }
    let next_cursor =
        has_more
            .then(|| segments.last())
            .flatten()
            .map(|segment| TrailSegmentCursor {
                trail_id: segment.trail_id,
                sequence: segment.sequence,
            });
    Ok(TrailSegmentPage {
        segments,
        next_cursor,
    })
}

fn trail_segment_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TrailSegment> {
    let value: String = row.get(0)?;
    let trail_id = Uuid::parse_str(&value).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(StorageError::InvalidStoredValue {
                field: "trail identifier",
                value,
            }),
        )
    })?;
    let start = Coordinate::from_fixed_parts(row.get(2)?, row.get(3)?)
        .map_err(|_| rusqlite::Error::InvalidQuery)?;
    let end = Coordinate::from_fixed_parts(row.get(4)?, row.get(5)?)
        .map_err(|_| rusqlite::Error::InvalidQuery)?;
    Ok(TrailSegment {
        trail_id: TrailId::from_uuid(trail_id),
        sequence: row.get(1)?,
        start,
        end,
    })
}

fn create_map_point(
    connection: &mut Connection,
    state: &mut SpatialSchemaState,
    name: &str,
    coordinate: Coordinate,
) -> Result<MapPointId, StorageError> {
    if name.trim().is_empty() {
        return Err(StorageError::InvalidStoredValue {
            field: "map point name",
            value: "empty".to_owned(),
        });
    }
    ensure_spatial_schema(connection, state)?;
    let transaction = connection.transaction()?;
    let point_id = MapPointId::new();
    transaction.execute(
        "INSERT INTO map_points (id, name, latitude_e7, longitude_e7)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            point_id.uuid().as_bytes(),
            name,
            coordinate.latitude().as_i32(),
            coordinate.longitude().as_i32()
        ],
    )?;
    let rtree_id = transaction.query_row(
        "INSERT INTO map_point_spatial_keys (point_id)
         VALUES (?1)
         RETURNING rtree_id",
        params![point_id.uuid().as_bytes()],
        |row| row.get::<_, i64>(0),
    )?;
    let rtree_id = SpatialRowId::from_sqlite(rtree_id)?;
    transaction.execute(
        "INSERT INTO map_points_rtree
            (id, min_lat_e7, max_lat_e7, min_lon_e7, max_lon_e7)
         VALUES (?1, ?2, ?2, ?3, ?3)",
        params![
            rtree_id.get(),
            coordinate.latitude().as_i32(),
            coordinate.longitude().as_i32()
        ],
    )?;
    transaction.commit()?;
    Ok(point_id)
}

fn map_points_in_bounds(
    connection: &Connection,
    state: &mut SpatialSchemaState,
    bounds: GeoBounds,
    cursor: Option<MapPointCursor>,
    limit: QueryLimit,
) -> Result<MapPointPage, StorageError> {
    ensure_spatial_schema(connection, state)?;
    let mut statement = connection.prepare(
        "SELECT p.id, p.name, p.latitude_e7, p.longitude_e7
         FROM map_points_rtree r
         JOIN map_point_spatial_keys k ON k.rtree_id = r.id
         JOIN map_points p ON p.id = k.point_id
         WHERE r.max_lat_e7 >= ?1 AND r.min_lat_e7 <= ?2
           AND ((?5 = 0 AND r.max_lon_e7 >= ?3 AND r.min_lon_e7 <= ?4)
             OR (?5 = 1 AND (r.max_lon_e7 >= ?3 OR r.min_lon_e7 <= ?4)))
           AND (?6 IS NULL OR p.id > ?6)
           ORDER BY p.id
         LIMIT ?7",
    )?;
    let after = cursor.map(|cursor| cursor.0.uuid().as_bytes().to_vec());
    let rows = statement.query_map(
        params![
            bounds.minimum_latitude,
            bounds.maximum_latitude,
            bounds.minimum_longitude,
            bounds.maximum_longitude,
            bounds.crosses_antimeridian(),
            after,
            i64::from(limit.get()) + 1,
        ],
        |row| {
            let id_bytes: Vec<u8> = row.get(0)?;
            let id = Uuid::from_slice(&id_bytes).map_err(|_| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Blob,
                    Box::new(StorageError::InvalidStoredValue {
                        field: "map point identifier",
                        value: hex_encode(id_bytes),
                    }),
                )
            })?;
            let coordinate = Coordinate::from_fixed_parts(row.get(2)?, row.get(3)?)
                .map_err(|_| rusqlite::Error::InvalidQuery)?;
            Ok(MapPoint {
                id: MapPointId::from_uuid(id),
                name: row.get(1)?,
                coordinate,
            })
        },
    )?;
    let mut points = rows.collect::<Result<Vec<_>, _>>()?;
    let has_more = points.len() > limit.get() as usize;
    if has_more {
        points.pop();
    }
    let next_cursor = has_more
        .then(|| points.last())
        .flatten()
        .map(|point| MapPointCursor(point.id));
    Ok(MapPointPage {
        points,
        next_cursor,
    })
}

fn rebuild_spatial_indexes(
    connection: &mut Connection,
    state: &mut SpatialSchemaState,
) -> Result<(), StorageError> {
    ensure_spatial_schema(connection, state)?;
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "
        DELETE FROM trail_segments_rtree;
        DELETE FROM trail_segment_spatial_keys;
        INSERT INTO trail_segment_spatial_keys (trail_id, sequence)
        SELECT trail_id, sequence FROM trail_segments ORDER BY trail_id, sequence;
        INSERT INTO trail_segments_rtree
        SELECT k.rtree_id,
               min(start_lat_e7, end_lat_e7), max(start_lat_e7, end_lat_e7),
               min(start_lon_e7, end_lon_e7), max(start_lon_e7, end_lon_e7)
        FROM trail_segments s
        JOIN trail_segment_spatial_keys k
          ON k.trail_id = s.trail_id AND k.sequence = s.sequence;
        DELETE FROM map_points_rtree;
        DELETE FROM map_point_spatial_keys;
        INSERT INTO map_point_spatial_keys (point_id)
        SELECT id FROM map_points ORDER BY id;
        INSERT INTO map_points_rtree
        SELECT k.rtree_id, p.latitude_e7, p.latitude_e7, p.longitude_e7, p.longitude_e7
        FROM map_points p
        JOIN map_point_spatial_keys k ON k.point_id = p.id
        WHERE NOT EXISTS (SELECT 1 FROM charging_spots c WHERE c.point_id = p.id
                          AND c.status NOT IN ('queued','published'));
        ",
    )?;
    transaction.commit()?;
    Ok(())
}

fn artifact_digest(path: &Path) -> Result<String, StorageError> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex_encode(digest.finalize()))
}

fn verify_pevcap_preview_source(preview: &PevcapImportPreview) -> Result<(), StorageError> {
    let metadata = preview
        .source_path()
        .metadata()
        .map_err(|_| StorageError::PevcapPreviewChanged)?;
    if !metadata.is_file() || metadata.len() != preview.artifact_size {
        return Err(StorageError::PevcapPreviewChanged);
    }
    if artifact_digest(preview.source_path()).map_err(|_| StorageError::PevcapPreviewChanged)?
        != preview.artifact_digest
    {
        return Err(StorageError::PevcapPreviewChanged);
    }
    Ok(())
}

fn preflight_pevcap(
    path: &Path,
    encoding: PevcapEncoding,
    limits: Option<PevcapLimits>,
) -> Result<PevcapImportPreview, StorageError> {
    let path = path.canonicalize().map_err(|_| StorageError::InvalidPath)?;
    let metadata = path.metadata()?;
    if !metadata.is_file() {
        return Err(StorageError::InvalidPath);
    }
    let artifact_size = metadata.len();
    if let Some(limits) = limits {
        check_limit_result(limits.check_artifact_bytes(artifact_size))?;
    }
    let artifact_digest = artifact_digest(&path)?;

    let file = File::open(&path)?;
    let mut reader = PevcapReader::new(BufReader::new(file), encoding)
        .map_err(|error| StorageError::PevcapImport(error.to_string()))?;
    let mut pevcap_usage = PevcapUsage::default();
    while let Some(event) = reader
        .next_event()
        .map_err(|error| StorageError::PevcapImport(error.to_string()))?
    {
        if let PevcapEvent::Record(record) = event {
            pevcap_usage.include_record(
                record.monotonic_ms.as_milliseconds(),
                record.phone_location.is_some(),
            );
            if let Some(limits) = limits {
                check_limit_result(pevcap_usage.check(limits))?;
            }
        }
    }
    let location_count = stream_pevcap_location_batches(&path, encoding, |samples| {
        for point in &samples {
            let milliseconds = point.sample.monotonic_milliseconds().as_u64();
            pevcap_usage.include_location(milliseconds);
        }
        Ok(u64::try_from(samples.len()).unwrap_or(u64::MAX))
    })?;
    if let Some(limits) = limits {
        check_limit_result(pevcap_usage.check(limits))?;
    }
    let duration_milliseconds = pevcap_usage.duration_milliseconds();
    let record_count = pevcap_usage.record_count();
    let outcome = if location_count == 0 {
        PevcapImportOutcome::CaptureOnly
    } else {
        PevcapImportOutcome::RideAndCapture
    };
    let warnings: Arc<[PevcapImportWarning]> = if outcome == PevcapImportOutcome::CaptureOnly {
        Arc::from([PevcapImportWarning::NoRouteLocations])
    } else {
        Arc::from([])
    };
    Ok(PevcapImportPreview {
        source_path: path,
        encoding,
        artifact_digest,
        artifact_size,
        record_count,
        location_count,
        duration_milliseconds,
        outcome,
        warnings,
    })
}

fn check_limit_result<T>(result: Result<T, LimitExceeded>) -> Result<T, StorageError> {
    result.map_err(|error| StorageError::PevcapLimitExceeded {
        resource: error.resource,
        limit: error.limit,
        actual: error.actual,
    })
}

fn check_pevcap_limit(resource: &'static str, limit: u64, actual: u64) -> Result<(), StorageError> {
    check_limit_result(check_limit(resource, limit, actual))
}

/// Copies a reviewed source into its reserved, digest-named staging file.
/// The import-work row is installed first, so startup recovery can remove an interrupted copy.
fn stage_pevcap_source(
    database_path: &Path,
    preview: &PevcapImportPreview,
    destination: &Path,
) -> Result<StagedPevcap, StorageError> {
    let directory = pevcap_import_directory(database_path);
    ensure_managed_pevcap_directory(&directory)?;
    if destination.parent() != Some(directory.as_path())
        || destination != pevcap_staging_path(database_path, preview.artifact_digest())
    {
        return Err(StorageError::PevcapPreviewChanged);
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let result = (|| {
        let source = File::open(&preview.source_path)?;
        let copied = std::io::copy(&mut BufReader::new(source), &mut output)?;
        output.sync_all()?;
        if copied != preview.artifact_size
            || artifact_digest(destination)? != preview.artifact_digest
        {
            return Err(StorageError::PevcapPreviewChanged);
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result?;
    Ok(StagedPevcap {
        path: Some(destination.to_owned()),
    })
}

struct StagedPevcap {
    path: Option<PathBuf>,
}

impl StagedPevcap {
    fn path(&self) -> Result<&Path, StorageError> {
        self.path
            .as_deref()
            .ok_or(StorageError::PevcapPreviewChanged)
    }

    fn remove(&mut self) -> Result<(), StorageError> {
        let Some(path) = self.path.take() else {
            return Ok(());
        };
        if let Err(error) = fs::remove_file(&path) {
            self.path = Some(path);
            return Err(StorageError::Io(error));
        }
        Ok(())
    }
}

impl Drop for StagedPevcap {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = fs::remove_file(path);
        }
    }
}

fn pevcap_staging_path(database_path: &Path, digest: &str) -> PathBuf {
    pevcap_import_directory(database_path).join(format!(".stage-{digest}.tmp"))
}

fn ensure_managed_pevcap_directory(directory: &Path) -> Result<(), StorageError> {
    for _ in 0..MAX_MANAGED_PEVCAP_DIRECTORY_ATTEMPTS {
        match fs::symlink_metadata(directory) {
            Ok(metadata) => {
                let file_type = metadata.file_type();
                return if file_type.is_dir() && !file_type.is_symlink() {
                    Ok(())
                } else {
                    Err(StorageError::PevcapPreviewChanged)
                };
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match fs::create_dir(directory) {
                    Ok(()) => return Ok(()),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(StorageError::Io(error)),
                }
            }
            Err(error) => return Err(StorageError::Io(error)),
        }
    }
    Err(StorageError::PevcapPreviewChanged)
}

#[cfg(test)]
mod sqlite_canonical_pevcap_tests {
    use super::*;

    #[test]
    fn concurrent_same_digest_import_keeps_one_sql_copy_and_cleans_staging() {
        use cutout_core::{
            MonotonicTimestamp, PevcapCapture, PevcapHeader, WallClockUnixTimestamp,
        };

        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("ride.sqlite");
        let source_path = directory.path().join("source.jsonl");
        let header = PevcapHeader::new(
            WallClockUnixTimestamp::new(1_700_000_000_000),
            "test",
            None,
            &[],
            &[],
            None,
            None,
            "test",
            [0; 32],
            &[],
        )
        .unwrap();
        let bytes = PevcapCapture::new(
            header,
            vec![PevcapRecord::link_up(MonotonicTimestamp::new(1), None)],
        )
        .to_jsonl()
        .unwrap()
        .into_bytes();
        fs::write(&source_path, &bytes).unwrap();
        let database = RideDatabase::open(&database_path).unwrap();
        let preview = database
            .preflight_pevcap(&source_path, PevcapEncoding::Jsonl)
            .unwrap();
        let (a_paused, a_ready) = mpsc::channel();
        let (resume_a, a_resume) = mpsc::channel();
        let (b_paused, b_ready) = mpsc::channel();
        let (resume_b, b_resume) = mpsc::channel();
        let timeout = Duration::from_secs(5);

        std::thread::scope(|scope| {
            let a = scope.spawn(|| {
                pevcap_confirmation_hooks::BEFORE_BEGIN.with(|slot| {
                    *slot.borrow_mut() = Some(Box::new(move || {
                        a_paused.send(()).unwrap();
                        a_resume.recv_timeout(timeout).unwrap();
                    }));
                });
                database.confirm_pevcap_import(&preview, 1_700_000_000_001)
            });
            a_ready.recv_timeout(timeout).unwrap();
            let b = scope.spawn(|| {
                pevcap_confirmation_hooks::BEFORE_STORE.with(|slot| {
                    *slot.borrow_mut() = Some(Box::new(move || {
                        b_paused.send(()).unwrap();
                        b_resume.recv_timeout(timeout).unwrap();
                    }));
                });
                database.confirm_pevcap_import(&preview, 1_700_000_000_002)
            });
            b_ready.recv_timeout(timeout).unwrap();
            resume_a.send(()).unwrap();
            let rejected = a.join().unwrap();
            resume_b.send(()).unwrap();
            let admitted = b.join().unwrap().unwrap();
            assert!(matches!(
                rejected,
                Err(StorageError::PevcapImportInProgress)
            ));
            assert!(!admitted.duplicate);
        });

        assert_eq!(
            database
                .pevcap_capture_chunk(&preview.artifact_digest, 0)
                .unwrap()
                .unwrap(),
            bytes
        );
        assert_eq!(
            fs::read_dir(pevcap_import_directory(&database_path))
                .unwrap()
                .count(),
            0
        );
        database.shutdown().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn legacy_archive_worker_preserves_symlink_and_external_bytes() {
        use cutout_core::{
            MonotonicTimestamp, PevcapCapture, PevcapHeader, WallClockUnixTimestamp,
        };
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("ride.sqlite");
        let source_path = directory.path().join("source.jsonl");
        let header = PevcapHeader::new(
            WallClockUnixTimestamp::new(1_700_000_000_000),
            "test",
            None,
            &[],
            &[],
            None,
            None,
            "test",
            [0; 32],
            &[],
        )
        .unwrap();
        let bytes = PevcapCapture::new(
            header,
            vec![PevcapRecord::link_up(MonotonicTimestamp::new(1), None)],
        )
        .to_jsonl()
        .unwrap();
        fs::write(&source_path, &bytes).unwrap();
        let database = RideDatabase::open(&database_path).unwrap();
        let canonical_database_path = fs::canonicalize(&database_path).unwrap();
        let preview = database
            .preflight_pevcap(&source_path, PevcapEncoding::Jsonl)
            .unwrap();
        let receipt = database
            .confirm_pevcap_import(&preview, 1_700_000_000_001)
            .unwrap();
        let managed_directory = pevcap_import_directory(&canonical_database_path);
        let legacy_path = managed_directory.join(format!("{}.jsonl", receipt.artifact_digest));
        let external_path = directory.path().join("external.jsonl");
        fs::write(&external_path, b"outside managed storage").unwrap();
        symlink(&external_path, &legacy_path).unwrap();
        let connection = Connection::open(&database_path).unwrap();
        connection
            .execute(
                "UPDATE pevcap_imports SET artifact_path = ?1 WHERE artifact_digest = ?2",
                params![legacy_path.to_string_lossy(), receipt.artifact_digest],
            )
            .unwrap();
        let stored_path: String = connection
            .query_row(
                "SELECT artifact_path FROM pevcap_imports WHERE artifact_digest = ?1",
                [&receipt.artifact_digest],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(PathBuf::from(stored_path), legacy_path);
        drop(connection);

        let duplicate_preview = database
            .preflight_pevcap(&source_path, PevcapEncoding::Jsonl)
            .unwrap();
        assert!(
            database
                .confirm_pevcap_import(&duplicate_preview, 1_700_000_000_002)
                .unwrap()
                .duplicate
        );
        let digest = receipt.artifact_digest.clone();
        database.shutdown().unwrap();
        Connection::open(&database_path)
            .unwrap()
            .execute_batch("DROP TABLE storage_normalization_progress; PRAGMA user_version = 57;")
            .unwrap();
        let reopened = RideDatabase::open(&database_path).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut completed = false;
        while Instant::now() < deadline {
            let connection = Connection::open(&database_path).unwrap();
            completed = connection
                .query_row(
                    "SELECT artifact_complete AND artifact_digest = ?1
                     FROM storage_normalization_progress WHERE singleton = 1",
                    [&digest],
                    |row| row.get(0),
                )
                .unwrap();
            if completed {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(completed, "archive worker advances past the unsafe symlink");
        assert!(
            fs::symlink_metadata(&legacy_path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(external_path).unwrap(), b"outside managed storage");
        reopened.shutdown().unwrap();
    }
}

/// Checks duplicate receipt facts against the source-bound reviewed preview.
/// `AlreadyImported` is a preview classification, never a persisted capture outcome.
fn validate_existing_pevcap_confirmation(
    receipt: &PevcapImportReceipt,
    preview: &PevcapImportPreview,
    publication: &CapturePublication,
) -> Result<(), StorageError> {
    let source_facts = preflight_pevcap(
        preview.source_path(),
        preview.encoding,
        receipt.recording.is_none().then_some(PevcapLimits::IMPORT),
    )?;
    if preview.artifact_digest != source_facts.artifact_digest
        || preview.artifact_size != source_facts.artifact_size
        || preview.record_count != source_facts.record_count
        || preview.location_count != source_facts.location_count
        || preview.duration_milliseconds != source_facts.duration_milliseconds
        || preview.warnings != source_facts.warnings
        || (preview.outcome != PevcapImportOutcome::AlreadyImported
            && preview.outcome != source_facts.outcome)
    {
        return Err(StorageError::PevcapPreviewChanged);
    }
    // A submission derivative retains GPS bytes without deriving a ride. An existing ordinary
    // import of the same bytes still has its original ride and must retain import validation.
    let submission_capture_only = match publication {
        CapturePublication::SubmissionSnapshot => {
            receipt.outcome == PevcapImportOutcome::CaptureOnly
        }
        CapturePublication::Import | CapturePublication::Recording(_) => false,
    };
    let capture_only = receipt.recording.is_some() || submission_capture_only;
    let outcome_matches = if capture_only {
        receipt.location_count == 0
            && receipt.ride_id.is_none()
            && receipt.outcome == PevcapImportOutcome::CaptureOnly
    } else {
        receipt.location_count == source_facts.location_count
            && receipt.outcome == source_facts.outcome
    };
    if receipt.artifact_digest != preview.artifact_digest
        || receipt.record_count != preview.record_count
        || !outcome_matches
    {
        return Err(StorageError::PevcapPreviewChanged);
    }
    if !capture_only
        && preview.outcome != PevcapImportOutcome::AlreadyImported
        && preview.outcome != receipt.outcome
    {
        return Err(StorageError::PevcapPreviewChanged);
    }
    Ok(())
}

fn pevcap_import_directory(database_path: &Path) -> PathBuf {
    let mut directory_name = database_path.as_os_str().to_owned();
    directory_name.push(".pevcap-imports");
    PathBuf::from(directory_name)
}

fn stream_pevcap_location_batches(
    path: &Path,
    encoding: PevcapEncoding,
    mut append: impl FnMut(Vec<PevcapRoutePoint>) -> Result<u64, StorageError>,
) -> Result<u64, StorageError> {
    let mut record_reader = pevcap_reader(path, encoding)?;
    let mut location_reader = pevcap_reader(path, encoding)?;
    let mut batch = Vec::with_capacity(PEVCAP_LOCATION_BATCH_SIZE);
    let mut accepted = 0_u64;
    let mut recorder = RideMapRecorder::new();
    let mut started = false;
    let mut attached = next_attached_location(&mut record_reader)?;
    let mut independent = next_independent_location(&mut location_reader)?;
    let mut last_emitted_key = None;
    let mut last_attached_key = None;
    loop {
        let take_attached = match (&attached, &independent) {
            (None, None) => break,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (Some(attached), Some(independent)) => {
                attached.monotonic_ms <= independent.monotonic_ms
            }
        };
        let candidate = if take_attached {
            match attached.take() {
                Some(candidate) => candidate,
                None => break,
            }
        } else {
            match independent.take() {
                Some(candidate) => candidate,
                None => break,
            }
        };
        append_pevcap_location(
            candidate.location,
            candidate.monotonic_ms,
            candidate.origin,
            &mut recorder,
            &mut started,
            &mut batch,
            &mut accepted,
            &mut append,
            &mut last_emitted_key,
            &mut last_attached_key,
        )?;
        if take_attached {
            attached = next_attached_location(&mut record_reader)?;
        } else {
            independent = next_independent_location(&mut location_reader)?;
        }
    }
    if !batch.is_empty() {
        accepted = accepted.saturating_add(append(batch)?);
    }
    Ok(accepted)
}

fn pevcap_reader(
    path: &Path,
    encoding: PevcapEncoding,
) -> Result<PevcapReader<BufReader<File>>, StorageError> {
    let file = File::open(path)?;
    PevcapReader::new(BufReader::new(file), encoding)
        .map_err(|error| StorageError::PevcapImport(error.to_string()))
}

#[derive(Clone, Copy)]
enum PevcapLocationOrigin {
    Attached,
    Independent,
}

struct PevcapLocationCandidate {
    location: PevcapPhoneLocation,
    monotonic_ms: u64,
    origin: PevcapLocationOrigin,
}

fn next_attached_location(
    reader: &mut PevcapReader<BufReader<File>>,
) -> Result<Option<PevcapLocationCandidate>, StorageError> {
    loop {
        let event = reader
            .next_event()
            .map_err(|error| StorageError::PevcapImport(error.to_string()))?;
        let Some(event) = event else { return Ok(None) };
        if let PevcapEvent::Record(record) = event
            && let Some(location) = record.phone_location
        {
            return Ok(Some(PevcapLocationCandidate {
                location,
                monotonic_ms: record.monotonic_ms.as_milliseconds(),
                origin: PevcapLocationOrigin::Attached,
            }));
        }
    }
}

fn next_independent_location(
    reader: &mut PevcapReader<BufReader<File>>,
) -> Result<Option<PevcapLocationCandidate>, StorageError> {
    loop {
        let event = reader
            .next_event()
            .map_err(|error| StorageError::PevcapImport(error.to_string()))?;
        let Some(event) = event else { return Ok(None) };
        if let PevcapEvent::Location(location) = event {
            return Ok(Some(PevcapLocationCandidate {
                location: location.location,
                monotonic_ms: location.receipt_monotonic_ms.as_milliseconds(),
                origin: PevcapLocationOrigin::Independent,
            }));
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "The importer mutates one bounded replay state and its append sink together."
)]
fn append_pevcap_location(
    location: PevcapPhoneLocation,
    monotonic_ms: u64,
    origin: PevcapLocationOrigin,
    recorder: &mut RideMapRecorder,
    started: &mut bool,
    batch: &mut Vec<PevcapRoutePoint>,
    accepted: &mut u64,
    append: &mut impl FnMut(Vec<PevcapRoutePoint>) -> Result<u64, StorageError>,
    last_emitted_key: &mut Option<(u64, i32, i32)>,
    last_attached_key: &mut Option<(u64, i32, i32)>,
) -> Result<(), StorageError> {
    let Ok(location) = location.canonical() else {
        return Ok(());
    };
    let Ok(coordinate) =
        Coordinate::from_degrees(location.latitude_degrees, location.longitude_degrees)
    else {
        return Ok(());
    };
    let location_key = (
        location.wall_clock_unix_ms,
        coordinate.latitude().as_i32(),
        coordinate.longitude().as_i32(),
    );
    if *last_emitted_key == Some(location_key)
        || ((match origin {
            PevcapLocationOrigin::Attached => true,
            PevcapLocationOrigin::Independent => false,
        }) && *last_attached_key == Some(location_key))
    {
        return Ok(());
    }
    let horizontal_accuracy_millimetres =
        pevcap_accuracy_millimetres(location.horizontal_accuracy_meters);
    let sample = LocationSample::new(
        coordinate,
        monotonic_ms,
        location.wall_clock_unix_ms,
        horizontal_accuracy_millimetres,
        LocationSource::PevcapImport,
    );
    if !*started {
        recorder
            .start(sample.monotonic_milliseconds(), None)
            .map_err(|_| StorageError::InvalidRideState(RideLifecycleState::Active))?;
        *started = true;
    }
    let segment_id = recorder.segment_id_for_sample(&sample);
    if recorder.check_sample(&sample) != LocationAdmission::Accepted {
        return Ok(());
    }
    let telemetry_state = recorder.telemetry_state_at(sample.monotonic_milliseconds());
    recorder.record_sample(sample);
    let start_reason = recorder
        .points()
        .last()
        .map_or(RideSegmentStartReason::ImportBoundary, |point| {
            point.segment_start_reason()
        });
    *last_emitted_key = Some(location_key);
    if match origin {
        PevcapLocationOrigin::Attached => true,
        PevcapLocationOrigin::Independent => false,
    } {
        *last_attached_key = Some(location_key);
    }
    batch.push(PevcapRoutePoint {
        sample,
        segment_id,
        start_reason,
        telemetry_state,
    });
    if batch.len() == PEVCAP_LOCATION_BATCH_SIZE {
        *accepted = accepted.saturating_add(append(std::mem::take(batch))?);
        batch.reserve(PEVCAP_LOCATION_BATCH_SIZE);
    }
    Ok(())
}

fn pevcap_accuracy_millimetres(metres: Option<f64>) -> Option<u32> {
    let metres = metres?;
    let millimetres = metres * 1_000.0;
    if !millimetres.is_finite() || millimetres < 0.0 || millimetres > f64::from(u32::MAX) {
        return None;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "finite non-negative values are bounded by u32::MAX above"
    )]
    Some(millimetres.round() as u32)
}

fn pevcap_import_receipt(
    connection: &Connection,
    digest: &str,
    duplicate: bool,
) -> Result<Option<PevcapImportReceipt>, StorageError> {
    let row = connection
        .query_row(
            "SELECT ride_id, outcome, record_count, location_count
             FROM pevcap_imports WHERE artifact_digest = ?1",
            [digest],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u64>(2)?,
                    row.get::<_, u64>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((ride_id, outcome, record_count, location_count)) = row else {
        return Ok(None);
    };
    let ride_id = ride_id
        .map(|value| {
            Uuid::parse_str(&value).map(RideId::from_uuid).map_err(|_| {
                StorageError::InvalidStoredValue {
                    field: "PEVCAP ride identifier",
                    value,
                }
            })
        })
        .transpose()?;
    Ok(Some(PevcapImportReceipt {
        recording: recorded_capture::lookup(connection, digest)?,
        ride_id,
        artifact_digest: digest.to_owned(),
        outcome: pevcap_outcome_from_db(&outcome)?,
        record_count,
        location_count,
        duplicate,
    }))
}

fn pevcap_outcome_from_db(value: &str) -> Result<PevcapImportOutcome, StorageError> {
    match value {
        "ride_and_capture" => Ok(PevcapImportOutcome::RideAndCapture),
        "capture_only" => Ok(PevcapImportOutcome::CaptureOnly),
        _ => Err(StorageError::InvalidStoredValue {
            field: "PEVCAP import outcome",
            value: value.to_owned(),
        }),
    }
}

fn begin_pevcap_import(
    connection: &mut Connection,
    digest: &str,
    managed_path: &Path,
    outcome: NewPevcapImportOutcome,
    created_at_ms: u64,
) -> Result<PevcapBegin, StorageError> {
    if let Some(receipt) = pevcap_import_receipt(connection, digest, true)? {
        return Ok(PevcapBegin::Duplicate(receipt));
    }
    let transaction = connection.transaction()?;
    let work_exists: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM pevcap_import_work WHERE artifact_digest = ?1)",
        [digest],
        |row| row.get(0),
    )?;
    if work_exists {
        return Err(StorageError::PevcapImportInProgress);
    }
    let ride_id = match outcome {
        NewPevcapImportOutcome::RideAndCapture => Some(create_ride(
            &transaction,
            RideSource::PevcapImport,
            created_at_ms,
            None,
        )?),
        NewPevcapImportOutcome::CaptureOnly => None,
    };
    transaction.execute(
        "INSERT INTO pevcap_import_work (artifact_digest, artifact_path, ride_id)
         VALUES (?1, ?2, ?3)",
        params![
            digest,
            managed_path.to_string_lossy(),
            ride_id.map(|id| id.uuid().to_string())
        ],
    )?;
    transaction.commit()?;
    Ok(PevcapBegin::Started { ride_id })
}

fn append_pevcap_location_batch(
    connection: &mut Connection,
    ride_id: RideId,
    samples: &[PevcapRoutePoint],
) -> Result<u64, StorageError> {
    let transaction = connection.transaction()?;
    let mut accepted = 0_u64;
    for point in samples {
        if append_location_in_transaction(
            &transaction,
            ride_id,
            point.sample,
            point.segment_id,
            point.telemetry_state,
            SegmentStartReasonInput::Recorded(point.start_reason),
            LocationWriteMode::PevcapImport,
            None,
            None,
            None,
        )?
        .admission()
            == LocationAdmission::Accepted
        {
            accepted = accepted.saturating_add(1);
        }
    }
    transaction.commit()?;
    Ok(accepted)
}

fn finish_pevcap_import(
    connection: &mut Connection,
    completion: &PevcapImportCompletion,
) -> Result<PevcapImportReceipt, StorageError> {
    let PevcapImportCompletion {
        digest,
        ride_id,
        managed_path,
        outcome,
        artifact_size,
        record_count,
        location_count,
        duration_milliseconds,
        imported_at_ms,
        publication,
    } = completion;
    let transaction = connection.transaction()?;
    let work_exists: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM pevcap_import_work
         WHERE artifact_digest = ?1 AND artifact_path = ?2)",
        params![digest, managed_path.to_string_lossy()],
        |row| row.get(0),
    )?;
    if !work_exists {
        return Err(StorageError::PevcapImportInProgress);
    }
    if let Some(ride_id) = ride_id {
        transition_ride(
            &transaction,
            *ride_id,
            RideEvent::Import,
            *imported_at_ms,
            None,
        )?;
        transaction.execute(
            "UPDATE rides SET completed_duration_ms = MAX(completed_duration_ms, ?1) WHERE id = ?2",
            params![duration_milliseconds, ride_id.uuid().to_string()],
        )?;
    }
    transaction.execute(
        "INSERT INTO pevcap_imports
            (artifact_digest, artifact_path, ride_id, outcome, artifact_size,
             record_count, location_count, imported_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            digest,
            Option::<String>::None,
            ride_id.map(|id| id.uuid().to_string()),
            outcome.as_db(),
            artifact_size,
            record_count,
            location_count,
            imported_at_ms,
        ],
    )?;
    capture_data::publish(&transaction, digest)?;
    if let Some(recording) = publication.recording() {
        recorded_capture::insert(&transaction, digest, recording)?;
    }
    transaction.execute(
        "DELETE FROM pevcap_import_work WHERE artifact_digest = ?1",
        [digest],
    )?;
    transaction.commit()?;
    Ok(PevcapImportReceipt {
        recording: publication.recording().cloned(),
        ride_id: *ride_id,
        artifact_digest: digest.to_owned(),
        outcome: outcome.as_public(),
        record_count: *record_count,
        location_count: *location_count,
        duplicate: false,
    })
}

fn abort_pevcap_import(
    connection: &mut Connection,
    digest: &str,
    ride_id: Option<RideId>,
) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    capture_data::abort(&transaction, digest)?;
    transaction.execute(
        "DELETE FROM pevcap_import_work WHERE artifact_digest = ?1",
        [digest],
    )?;
    if let Some(ride_id) = ride_id {
        transaction.execute(
            "DELETE FROM rides WHERE id = ?1 AND source = 'pevcap_import' AND state = 'draft'",
            [ride_id.uuid().to_string()],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

fn save_selected_device(
    connection: &Connection,
    platform_identifier: &str,
    updated_at_ms: u64,
) -> Result<(), StorageError> {
    connection.execute(
        "INSERT INTO selected_device (singleton_key, platform_identifier, updated_at_ms)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(singleton_key) DO UPDATE SET platform_identifier = excluded.platform_identifier,
             updated_at_ms = excluded.updated_at_ms",
        params![SelectedDeviceKey::VALUE.blob(), platform_identifier, updated_at_ms],
    )?;
    Ok(())
}

fn selected_device(connection: &Connection) -> Result<Option<String>, StorageError> {
    connection
        .query_row(
            "SELECT platform_identifier FROM selected_device WHERE singleton_key = ?1",
            [SelectedDeviceKey::VALUE.blob()],
            |row| row.get(0),
        )
        .optional()
        .map_err(StorageError::from)
}

fn remember_last_connected_device(
    connection: &Connection,
    platform_identifier: &str,
    updated_at_ms: u64,
) -> Result<(), StorageError> {
    connection.execute(
        "INSERT INTO last_connected_device (singleton_key, platform_identifier, updated_at_ms)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(singleton_key) DO UPDATE SET platform_identifier = excluded.platform_identifier,
             updated_at_ms = excluded.updated_at_ms",
        params![
            LastConnectedDeviceKey::VALUE.blob(),
            platform_identifier,
            updated_at_ms
        ],
    )?;
    Ok(())
}

fn apply_verified_connection_admission(
    connection: &mut Connection,
    platform_identifier: &str,
    updated_at_ms: u64,
    lifecycle_mutations: &[VerifiedConnectionLifecycleMutation],
    metadata: Option<&VerifiedConnectionRideMetadata>,
    unselected_metadata: Option<&VerifiedConnectionRideMetadata>,
) -> Result<VerifiedConnectionAdmissionOutcome, StorageError> {
    let transaction = connection.transaction()?;
    remember_last_connected_device(&transaction, platform_identifier, updated_at_ms)?;
    let selected_device_matches =
        selected_device(&transaction)?.as_deref() == Some(platform_identifier);
    let mut created_ride_id = None;
    let mut final_ride = None;

    if selected_device_matches {
        for mutation in lifecycle_mutations {
            match mutation {
                VerifiedConnectionLifecycleMutation::Transition {
                    ride_id,
                    event,
                    occurred_at_ms,
                    monotonic_at_ms,
                } => {
                    let lifecycle = transition_ride(
                        &transaction,
                        *ride_id,
                        *event,
                        *occurred_at_ms,
                        *monotonic_at_ms,
                    )?;
                    final_ride = Some((*ride_id, lifecycle));
                }
                VerifiedConnectionLifecycleMutation::StartLive {
                    created_at_ms,
                    monotonic_created_at_ms,
                    candidate_vehicle,
                    music_history_policy,
                } => {
                    if created_ride_id.is_some() {
                        return Err(StorageError::InvalidConnectionAdmissionPlan);
                    }
                    let ride_id = create_started_live_ride_in_transaction(
                        &transaction,
                        *created_at_ms,
                        *monotonic_created_at_ms,
                        candidate_vehicle.as_deref(),
                    )?;
                    apply_music_history_policy(&transaction, ride_id, *music_history_policy)?;
                    created_ride_id = Some(ride_id);
                    final_ride = Some((ride_id, RideLifecycleState::Active));
                }
            }
        }
    }

    let metadata = if selected_device_matches {
        metadata
    } else {
        unselected_metadata
    };
    if let Some(metadata) = metadata {
        let ride_id = match metadata.target {
            VerifiedConnectionRideTarget::Existing(ride_id) => Some(ride_id),
            VerifiedConnectionRideTarget::Created => created_ride_id,
        };
        if let Some(ride_id) = ride_id {
            update_ride_map_metadata_with_moving_time_in_transaction(
                &transaction,
                ride_id,
                metadata.candidate_vehicle.as_deref(),
                metadata.associated_vehicle.as_deref(),
                metadata.associated_at_ms,
                metadata.last_telemetry_at_ms,
                None,
                None,
                metadata
                    .moving_time_source_boundary_at_ms
                    .map(MonotonicMilliseconds::new),
                None,
            )?;
            let state = transaction.query_row(
                "SELECT state FROM rides WHERE id = ?1",
                [ride_id.uuid().to_string()],
                |row| row.get::<_, String>(0),
            )?;
            final_ride = Some((ride_id, state_from_db(&state)?));
        }
    }

    let committed_ride_checkpoint = final_ride
        .map(|(ride_id, lifecycle)| {
            committed_ride_moving_time_checkpoint(&transaction, ride_id, lifecycle)
        })
        .transpose()?;
    transaction.commit()?;
    Ok(VerifiedConnectionAdmissionOutcome {
        selected_device_matches,
        created_ride_id,
        committed_ride_checkpoint,
    })
}

fn last_connected_device(connection: &Connection) -> Result<Option<String>, StorageError> {
    connection
        .query_row(
            "SELECT platform_identifier FROM last_connected_device WHERE singleton_key = ?1",
            [LastConnectedDeviceKey::VALUE.blob()],
            |row| row.get(0),
        )
        .optional()
        .map_err(StorageError::from)
}

fn clear_last_connected_device(connection: &Connection) -> Result<(), StorageError> {
    connection.execute(
        "DELETE FROM last_connected_device WHERE singleton_key = ?1",
        [LastConnectedDeviceKey::VALUE.blob()],
    )?;
    Ok(())
}

fn save_phone_alarm_preferences(
    connection: &Connection,
    device_identity: &str,
    preferences: PhoneAlarmPreferencesRecord,
) -> Result<(), StorageError> {
    connection.execute(
        "INSERT INTO phone_alarm_preferences (device_identity, enabled, pwm_duty_percent)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(device_identity) DO UPDATE SET enabled = excluded.enabled,
             pwm_duty_percent = excluded.pwm_duty_percent",
        params![
            device_identity,
            preferences.enabled(),
            preferences.pwm_duty_percent()
        ],
    )?;
    Ok(())
}

fn phone_alarm_preferences(
    connection: &Connection,
    device_identity: &str,
) -> Result<Option<PhoneAlarmPreferencesRecord>, StorageError> {
    connection
        .query_row(
            "SELECT enabled, pwm_duty_percent FROM phone_alarm_preferences
             WHERE device_identity = ?1",
            [device_identity],
            |row| {
                PhoneAlarmPreferencesRecord::new(row.get(0)?, row.get(1)?).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Integer,
                        Box::new(error),
                    )
                })
            },
        )
        .optional()
        .map_err(StorageError::from)
}

/// Durable music retention state, separate from whether any events exist.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MusicHistoryStatus {
    /// The ride has no music retention choice.
    Missing,
    /// History was explicitly disabled.
    Disabled,
    /// History retains only opaque metadata.
    Redacted,
    /// Human-readable history is enabled.
    Available,
    /// Music history was explicitly forgotten.
    Deleted,
}

/// An authoritative bounded history projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MusicHistory {
    /// Retention status; storage unavailability is reported as an error.
    pub status: MusicHistoryStatus,
    /// Retained events in sequence order.
    pub events: Vec<MusicRideEvent>,
}

/// Result of appending one live music transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MusicTimelineRecordResult {
    /// Admission outcome for the transition.
    pub outcome: MusicTimelineOutcome,
    /// Durable ride-local sequence when the transition was recorded.
    pub sequence: Option<u64>,
    /// Authoritative policy applied by the actual ordered SQLite transaction.
    pub effective_policy: MusicHistoryPolicy,
}

fn music_history(connection: &Connection, ride_id: RideId) -> Result<MusicHistory, StorageError> {
    ensure_visible_ride(connection, ride_id)?;
    music_history_unfiltered(connection, ride_id)
}

fn music_history_unfiltered(
    connection: &Connection,
    ride_id: RideId,
) -> Result<MusicHistory, StorageError> {
    ensure_ride_exists(connection, ride_id)?;
    let retention: Option<(String, bool)> = connection
        .query_row(
            "SELECT policy, deleted FROM ride_music_history WHERE ride_id = ?1",
            [ride_id.uuid().to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let status = match retention {
        None => MusicHistoryStatus::Missing,
        Some((_, true)) => MusicHistoryStatus::Deleted,
        Some((policy, false)) => match parse_policy(&policy)? {
            MusicHistoryPolicy::Disabled => MusicHistoryStatus::Disabled,
            MusicHistoryPolicy::OpaqueItem => MusicHistoryStatus::Redacted,
            MusicHistoryPolicy::HumanReadable => MusicHistoryStatus::Available,
        },
    };
    let events = match status {
        MusicHistoryStatus::Missing
        | MusicHistoryStatus::Disabled
        | MusicHistoryStatus::Deleted => Vec::new(),
        MusicHistoryStatus::Redacted | MusicHistoryStatus::Available => {
            music_events(connection, ride_id)?
        }
    };
    let events = if status == MusicHistoryStatus::Redacted {
        events
            .into_iter()
            .map(|mut event| {
                event.redact_display_metadata();
                event
            })
            .collect()
    } else {
        events
    };
    Ok(MusicHistory { status, events })
}

fn music_last_observed_at(
    connection: &Connection,
    ride_id: RideId,
) -> Result<Option<i64>, StorageError> {
    Ok(connection
        .query_row(
            "SELECT last_observed_at_ms FROM ride_music_history WHERE ride_id = ?1",
            [ride_id.uuid().to_string()],
            |row| row.get(0),
        )
        .optional()?
        .flatten())
}

fn music_observation_out_of_order(
    connection: &Connection,
    ride_id: RideId,
    event: &MusicRideEvent,
) -> Result<bool, StorageError> {
    let started_at: Option<u64> = connection.query_row(
        "SELECT monotonic_created_at_ms FROM rides WHERE id = ?1",
        [ride_id.uuid().to_string()],
        |row| row.get(0),
    )?;
    let observed = event
        .observed_at()
        .map(|at| music_sqlite_integer(at.as_milliseconds(), "music observation timestamp"))
        .transpose()?;
    let last_observed = music_last_observed_at(connection, ride_id)?;
    let before_ride_start = match (started_at, observed) {
        (Some(start), Some(observed)) => observed < i64::try_from(start).unwrap_or(i64::MAX),
        (Some(start), None) => event.monotonic_at().as_milliseconds() < start,
        _ => false,
    };
    Ok(observed
        .zip(last_observed)
        .is_some_and(|(at, last)| at < last)
        || before_ride_start)
}

fn update_music_observation(
    connection: &Connection,
    ride_id: RideId,
    observed: Option<i64>,
) -> Result<(), StorageError> {
    if let Some(observed) = observed {
        connection.execute(
            "UPDATE ride_music_history SET last_observed_at_ms = ?2 WHERE ride_id = ?1",
            params![ride_id.uuid().to_string(), observed],
        )?;
    }
    Ok(())
}

fn record_music_event(
    connection: &mut Connection,
    ride_id: RideId,
    event: &MusicRideEvent,
) -> Result<MusicTimelineRecordResult, StorageError> {
    let transaction = connection.transaction()?;
    let lifecycle = load_ride_write_state(&transaction, ride_id)?.lifecycle();
    if lifecycle != RideLifecycleState::Active {
        return Err(StorageError::InvalidRideState(lifecycle));
    }
    let policy = music_history_policy(&transaction, ride_id)?;
    if policy == MusicHistoryPolicy::Disabled {
        return Ok(MusicTimelineRecordResult {
            outcome: MusicTimelineOutcome::Disabled,
            sequence: None,
            effective_policy: policy,
        });
    }
    // Keep live writes closed while the existing durable prefix is unreadable or non-contiguous.
    let _ = music_events(&transaction, ride_id)?;
    let mut event = event.clone();
    if policy == MusicHistoryPolicy::OpaqueItem {
        event.redact_display_metadata();
    }
    let timestamps = music_event_sql_timestamps(&event)?;
    let observed = timestamps.observed_at;
    if music_observation_out_of_order(&transaction, ride_id, &event)? {
        return Ok(MusicTimelineRecordResult {
            outcome: MusicTimelineOutcome::OutOfOrder,
            sequence: None,
            effective_policy: policy,
        });
    }
    let previous = transaction
        .query_row(
            "SELECT provider, item_identifier, title, artist, kind, monotonic_at_ms,
                wall_clock_at_ms, clock_uncertainty_milliseconds, observed_at_ms, sequence
         FROM ride_music_event WHERE ride_id = ?1 ORDER BY sequence DESC LIMIT 1",
            [ride_id.uuid().to_string()],
            |row| Ok((decode_music_event(row)?, row.get::<_, u64>(9)?)),
        )
        .optional()?;
    let count = previous
        .as_ref()
        .map_or(0, |(_, sequence)| sequence.saturating_add(1));
    let outcome = MusicTimeline::admission(
        previous.as_ref().map(|(event, _)| event),
        usize::try_from(count).unwrap_or(usize::MAX),
        &event,
    );
    match outcome {
        MusicTimelineOutcome::Recorded => insert_music_event(
            &transaction,
            ride_id,
            policy,
            i64::try_from(count).unwrap_or(i64::MAX),
            &event,
        )?,
        MusicTimelineOutcome::Duplicate => {
            update_music_observation(&transaction, ride_id, observed)?;
        }
        MusicTimelineOutcome::OutOfOrder
        | MusicTimelineOutcome::Full
        | MusicTimelineOutcome::Disabled => {}
    }
    transaction.commit()?;
    Ok(MusicTimelineRecordResult {
        sequence: (outcome == MusicTimelineOutcome::Recorded).then_some(count),
        outcome,
        effective_policy: policy,
    })
}

fn save_music_history_policy(
    connection: &mut Connection,
    ride_id: RideId,
    policy: MusicHistoryPolicy,
) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    let lifecycle = load_ride_write_state(&transaction, ride_id)?.lifecycle();
    let live_lifecycle = lifecycle.is_recording();
    let historical_redaction = policy == MusicHistoryPolicy::OpaqueItem && lifecycle.is_terminal();
    if !live_lifecycle && !historical_redaction {
        return Err(StorageError::InvalidRideState(lifecycle));
    }
    if historical_redaction {
        let existing = transaction
            .query_row(
                "SELECT policy, deleted FROM ride_music_history WHERE ride_id = ?1",
                [ride_id.uuid().to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
            )
            .optional()?;
        let monotonic_redaction = existing.is_some_and(|(stored, deleted)| {
            !deleted
                && (match parse_policy(&stored) {
                    Ok(MusicHistoryPolicy::HumanReadable | MusicHistoryPolicy::OpaqueItem) => true,
                    _ => false,
                })
        });
        if !monotonic_redaction {
            return Err(StorageError::MusicPolicyConflict);
        }
    }
    apply_music_history_policy(&transaction, ride_id, policy)?;
    // Validate the post-policy projection before committing so callers never observe a failed
    // policy update with a durable policy mutation.
    let _ = music_history_unfiltered(&transaction, ride_id)?;
    transaction.commit()?;
    Ok(())
}

fn apply_music_history_policy(
    transaction: &rusqlite::Transaction<'_>,
    ride_id: RideId,
    policy: MusicHistoryPolicy,
) -> Result<(), StorageError> {
    let existing_policy = transaction
        .query_row(
            "SELECT policy, deleted FROM ride_music_history WHERE ride_id = ?1",
            [ride_id.uuid().to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
        )
        .optional()?;
    if existing_policy.is_none() {
        // A missing retention row is an absence-of-consent boundary. Quarantine any orphaned
        // event rows before creating a new policy so they cannot reappear on re-enable.
        transaction.execute(
            "DELETE FROM ride_music_event WHERE ride_id = ?1",
            [ride_id.uuid().to_string()],
        )?;
    } else if let Some((value, deleted)) = existing_policy.as_ref()
        && !deleted
        && parse_policy(value)? == MusicHistoryPolicy::OpaqueItem
        && policy == MusicHistoryPolicy::HumanReadable
    {
        // Opaque history must never become readable merely because a user opts in later. Drop
        // all fields that were not validly retained under the prior policy.
        transaction.execute(
            "UPDATE ride_music_event SET item_identifier = CASE
                     WHEN provider = 'spotify' AND item_identifier LIKE 'spotify:local:%' THEN NULL
                     ELSE item_identifier END,
                     title = NULL, artist = NULL
                 WHERE ride_id = ?1",
            [ride_id.uuid().to_string()],
        )?;
    }
    transaction.execute(
        "INSERT INTO ride_music_history (ride_id, policy, state)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(ride_id) DO UPDATE SET policy = excluded.policy, state = excluded.state, deleted = 0,
             last_observed_at_ms = CASE WHEN excluded.policy = 'disabled' THEN NULL ELSE last_observed_at_ms END",
        params![
            ride_id.uuid().to_string(),
            policy_name(policy),
            history_state_name(policy),
        ],
    )?;
    if policy == MusicHistoryPolicy::Disabled {
        transaction.execute(
            "DELETE FROM ride_music_event WHERE ride_id = ?1",
            [ride_id.uuid().to_string()],
        )?;
    } else if policy == MusicHistoryPolicy::OpaqueItem {
        transaction.execute(
            "UPDATE ride_music_event
             SET item_identifier = CASE
                     WHEN item_identifier IS NOT NULL AND trim(item_identifier) = ''
                          THEN NULL
                     WHEN provider = 'spotify' AND item_identifier LIKE 'spotify:local:%'
                     THEN NULL ELSE item_identifier END,
                 title = NULL,
                 artist = NULL
             WHERE ride_id = ?1",
            [ride_id.uuid().to_string()],
        )?;
    }
    Ok(())
}

fn save_music_event(
    connection: &mut Connection,
    ride_id: RideId,
    policy: MusicHistoryPolicy,
    sequence: i64,
    event: &MusicRideEvent,
) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    let lifecycle = load_ride_write_state(&transaction, ride_id)?.lifecycle();
    if !lifecycle.is_recording() {
        return Err(StorageError::InvalidRideState(lifecycle));
    }
    insert_music_event(&transaction, ride_id, policy, sequence, event)?;
    transaction.commit()?;
    Ok(())
}

fn insert_music_event(
    transaction: &rusqlite::Transaction<'_>,
    ride_id: RideId,
    policy: MusicHistoryPolicy,
    sequence: i64,
    event: &MusicRideEvent,
) -> Result<(), StorageError> {
    ensure_ride_exists(transaction, ride_id)?;
    let policy = music_event_policy(transaction, ride_id, policy)?;
    if policy == MusicHistoryPolicy::Disabled {
        return Ok(());
    }
    // Keep the durable privacy invariant in this primitive as well as in the live writer.
    // Importers and restore code must not be able to persist metadata-bearing identifiers under
    // an opaque policy.
    let mut event = event.clone();
    if policy == MusicHistoryPolicy::OpaqueItem {
        event.redact_display_metadata();
    }
    let existing = transaction
        .query_row(
            "SELECT provider, item_identifier, title, artist, kind, monotonic_at_ms,
                    wall_clock_at_ms, clock_uncertainty_milliseconds, observed_at_ms
             FROM ride_music_event WHERE ride_id = ?1 AND sequence = ?2",
            params![ride_id.uuid().to_string(), sequence],
            decode_music_event,
        )
        .optional()?;
    if let Some(existing) = existing {
        if existing == event {
            return Ok(());
        }
        return Err(StorageError::MusicSequenceConflict { sequence });
    }
    let count: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM ride_music_event WHERE ride_id = ?1",
        [ride_id.uuid().to_string()],
        |row| row.get(0),
    )?;
    if count >= i64::try_from(cutout_music::MAX_MUSIC_TIMELINE_EVENTS).unwrap_or(i64::MAX) {
        return Err(StorageError::MusicTimelineFull);
    }
    if sequence != count {
        return Err(StorageError::MusicSequenceGap {
            sequence,
            expected: count,
        });
    }
    let timestamps = music_event_sql_timestamps(&event)?;
    let monotonic_ms = timestamps.monotonic;
    let wall_clock_ms = timestamps.wall_clock;
    let clock_uncertainty_ms = timestamps.uncertainty;
    let previous_monotonic_ms: Option<i64> = transaction
        .query_row(
            "SELECT monotonic_at_ms FROM ride_music_event WHERE ride_id = ?1
             ORDER BY sequence DESC LIMIT 1",
            [ride_id.uuid().to_string()],
            |row| row.get(0),
        )
        .optional()?;
    let observed_at_ms = timestamps.observed_at;
    if music_observation_out_of_order(transaction, ride_id, &event)?
        || previous_monotonic_ms.is_some_and(|previous| monotonic_ms < previous)
    {
        return Err(StorageError::MusicEventOutOfOrder { sequence });
    }
    let item_identifier = event
        .item_identifier()
        .map(cutout_music::MusicIdentifier::as_str);
    let title = (policy == MusicHistoryPolicy::HumanReadable)
        .then(|| event.title())
        .flatten();
    let artist = (policy == MusicHistoryPolicy::HumanReadable)
        .then(|| event.artist())
        .flatten();
    transaction.execute(
        "INSERT INTO ride_music_event
            (ride_id, sequence, provider, item_identifier, title, artist, kind,
             monotonic_at_ms, wall_clock_at_ms, clock_uncertainty_milliseconds, observed_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            ride_id.uuid().to_string(),
            sequence,
            provider_name(event.provider()),
            item_identifier,
            title,
            artist,
            event_kind_name(event.kind()),
            monotonic_ms,
            wall_clock_ms,
            clock_uncertainty_ms,
            observed_at_ms,
        ],
    )?;
    update_music_observation(transaction, ride_id, observed_at_ms)?;
    Ok(())
}

struct MusicEventSqlTimestamps {
    monotonic: i64,
    wall_clock: i64,
    uncertainty: i64,
    observed_at: Option<i64>,
}

fn music_event_sql_timestamps(
    event: &MusicRideEvent,
) -> Result<MusicEventSqlTimestamps, StorageError> {
    Ok(MusicEventSqlTimestamps {
        monotonic: music_sqlite_integer(
            event.monotonic_at().as_milliseconds(),
            "music monotonic timestamp",
        )?,
        wall_clock: music_sqlite_integer(
            event.wall_clock_at().as_milliseconds(),
            "music wall clock timestamp",
        )?,
        uncertainty: music_sqlite_integer(
            event.clock_uncertainty_milliseconds(),
            "music clock uncertainty",
        )?,
        observed_at: event
            .observed_at()
            .map(|at| music_sqlite_integer(at.as_milliseconds(), "music observation timestamp"))
            .transpose()?,
    })
}

fn music_sqlite_integer(value: u64, field: &'static str) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::InvalidStoredValue {
        field,
        value: value.to_string(),
    })
}

fn music_event_policy(
    transaction: &rusqlite::Transaction<'_>,
    ride_id: RideId,
    requested: MusicHistoryPolicy,
) -> Result<MusicHistoryPolicy, StorageError> {
    let stored = transaction
        .query_row(
            "SELECT policy, deleted FROM ride_music_history WHERE ride_id = ?1",
            [ride_id.uuid().to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
        )
        .optional()?
        .map(|(value, deleted)| {
            if deleted {
                Ok(MusicHistoryPolicy::Disabled)
            } else {
                parse_policy(&value)
            }
        })
        .transpose()?;
    match stored {
        Some(policy) if policy != requested => Err(StorageError::MusicPolicyConflict),
        Some(policy) => Ok(policy),
        None => {
            apply_music_history_policy(transaction, ride_id, requested)?;
            Ok(requested)
        }
    }
}

fn delete_music_history(connection: &mut Connection, ride_id: RideId) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    ensure_ride_exists(&transaction, ride_id)?;
    transaction.execute(
        "DELETE FROM ride_music_event WHERE ride_id = ?1",
        [ride_id.uuid().to_string()],
    )?;
    transaction.execute(
        "INSERT INTO ride_music_history (ride_id, policy, state, deleted)
         VALUES (?1, 'disabled', 'deleted', 1)
         ON CONFLICT(ride_id) DO UPDATE SET policy = 'disabled', state = 'deleted', deleted = 1, last_observed_at_ms = NULL",
        [ride_id.uuid().to_string()],
    )?;
    transaction.commit()?;
    Ok(())
}

fn music_events(
    connection: &Connection,
    ride_id: RideId,
) -> Result<Vec<MusicRideEvent>, StorageError> {
    ensure_ride_exists(connection, ride_id)?;
    let mut statement = connection.prepare(
        "SELECT provider, item_identifier, title, artist, kind, monotonic_at_ms,
                wall_clock_at_ms, clock_uncertainty_milliseconds, observed_at_ms, sequence
         FROM ride_music_event WHERE ride_id = ?1 ORDER BY sequence LIMIT ?2",
    )?;
    let events = statement
        .query_map(
            params![
                ride_id.uuid().to_string(),
                cutout_music::MAX_MUSIC_TIMELINE_EVENTS + 1
            ],
            |row| {
                let sequence = row.get::<_, i64>(9)?;
                Ok((sequence, decode_music_event(row)?))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    if events.len() > cutout_music::MAX_MUSIC_TIMELINE_EVENTS {
        return Err(StorageError::MusicTimelineFull);
    }
    for (index, (sequence, _)) in events.iter().enumerate() {
        let expected = i64::try_from(index).unwrap_or(i64::MAX);
        if *sequence != expected {
            return Err(StorageError::MusicSequenceGap {
                sequence: *sequence,
                expected,
            });
        }
    }
    for (index, pair) in events.windows(2).enumerate() {
        if pair[1].1.monotonic_at() < pair[0].1.monotonic_at() {
            return Err(StorageError::MusicEventOutOfOrder {
                sequence: i64::try_from(index + 1).unwrap_or(i64::MAX),
            });
        }
        if let (Some(previous), Some(current)) = (pair[0].1.observed_at(), pair[1].1.observed_at())
            && current < previous
        {
            return Err(StorageError::MusicEventOutOfOrder {
                sequence: i64::try_from(index + 1).unwrap_or(i64::MAX),
            });
        }
    }
    let max_observed = events
        .iter()
        .filter_map(|(_, event)| event.observed_at())
        .map(cutout_core::MonotonicTimestamp::as_milliseconds)
        .max();
    let last_observed = music_last_observed_at(connection, ride_id)?
        .map(|timestamp| u64::try_from(timestamp).unwrap_or(u64::MAX));
    if max_observed.is_some_and(|maximum| last_observed.is_none_or(|last| last < maximum)) {
        return Err(StorageError::MusicEventOutOfOrder {
            sequence: i64::try_from(events.len().saturating_sub(1)).unwrap_or(i64::MAX),
        });
    }
    Ok(events.into_iter().map(|(_, event)| event).collect())
}

fn music_history_policy(
    connection: &Connection,
    ride_id: RideId,
) -> Result<MusicHistoryPolicy, StorageError> {
    ensure_ride_exists(connection, ride_id)?;
    let value = connection
        .query_row(
            "SELECT policy, deleted FROM ride_music_history WHERE ride_id = ?1",
            [ride_id.uuid().to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
        )
        .optional()?;
    match value {
        None | Some((_, true)) => Ok(MusicHistoryPolicy::Disabled),
        Some((policy, false)) => parse_policy(&policy),
    }
}

fn music_history_state(
    connection: &Connection,
    ride_id: RideId,
) -> Result<MusicHistoryState, StorageError> {
    ensure_visible_ride(connection, ride_id)?;
    let value = connection
        .query_row(
            "SELECT state FROM ride_music_history WHERE ride_id = ?1",
            [ride_id.uuid().to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    value
        .map(|value| parse_music_history_state(&value))
        .transpose()
        .map(|state| state.unwrap_or(MusicHistoryState::Missing))
}

fn ensure_ride_exists(connection: &Connection, ride_id: RideId) -> Result<(), StorageError> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM rides WHERE id = ?1)",
        [ride_id.uuid().to_string()],
        |row| row.get(0),
    )?;
    exists.then_some(()).ok_or(StorageError::NotFound)
}

fn ensure_visible_ride(connection: &Connection, ride_id: RideId) -> Result<(), StorageError> {
    let visible: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM rides WHERE id = ?1 AND state != 'discarded')",
        [ride_id.uuid().to_string()],
        |row| row.get(0),
    )?;
    visible.then_some(()).ok_or(StorageError::NotFound)
}

fn policy_name(policy: MusicHistoryPolicy) -> &'static str {
    match policy {
        MusicHistoryPolicy::Disabled => "disabled",
        MusicHistoryPolicy::OpaqueItem => "opaque_item",
        MusicHistoryPolicy::HumanReadable => "human_readable",
    }
}

fn history_state_name(policy: MusicHistoryPolicy) -> &'static str {
    match policy {
        MusicHistoryPolicy::Disabled => "disabled",
        MusicHistoryPolicy::OpaqueItem => "opaque_item",
        MusicHistoryPolicy::HumanReadable => "human_readable",
    }
}

fn parse_music_history_state(value: &str) -> Result<MusicHistoryState, StorageError> {
    match value {
        "disabled" => Ok(MusicHistoryState::Disabled),
        "opaque_item" => Ok(MusicHistoryState::Redacted),
        "human_readable" => Ok(MusicHistoryState::HumanReadable),
        "deleted" => Ok(MusicHistoryState::Deleted),
        other => Err(StorageError::InvalidStoredValue {
            field: "music history state",
            value: other.to_owned(),
        }),
    }
}

fn parse_policy(value: &str) -> Result<MusicHistoryPolicy, StorageError> {
    match value {
        "disabled" => Ok(MusicHistoryPolicy::Disabled),
        "opaque_item" => Ok(MusicHistoryPolicy::OpaqueItem),
        "human_readable" => Ok(MusicHistoryPolicy::HumanReadable),
        other => Err(StorageError::InvalidStoredValue {
            field: "music history policy",
            value: other.to_owned(),
        }),
    }
}

fn provider_name(provider: MusicProvider) -> &'static str {
    match provider {
        MusicProvider::AppleMusic => "apple_music",
        MusicProvider::Spotify => "spotify",
        // SoundCloud listening events are rejected by MusicRideEvent::new.
        MusicProvider::SoundCloud => "soundcloud",
    }
}

fn parse_provider(value: &str) -> Result<MusicProvider, String> {
    match value {
        "apple_music" => Ok(MusicProvider::AppleMusic),
        "spotify" => Ok(MusicProvider::Spotify),
        other => Err(format!("invalid music provider: {other}")),
    }
}

fn event_kind_name(kind: MusicRideEventKind) -> &'static str {
    match kind {
        MusicRideEventKind::Play => "play",
        MusicRideEventKind::Pause => "pause",
        MusicRideEventKind::Skip => "skip",
        MusicRideEventKind::ItemChanged => "item_changed",
        MusicRideEventKind::Stopped => "stopped",
        MusicRideEventKind::ProviderDisconnected => "provider_disconnected",
    }
}

fn parse_event_kind(value: &str) -> Result<MusicRideEventKind, String> {
    match value {
        "play" => Ok(MusicRideEventKind::Play),
        "pause" => Ok(MusicRideEventKind::Pause),
        "skip" => Ok(MusicRideEventKind::Skip),
        "item_changed" => Ok(MusicRideEventKind::ItemChanged),
        "stopped" => Ok(MusicRideEventKind::Stopped),
        "provider_disconnected" => Ok(MusicRideEventKind::ProviderDisconnected),
        other => Err(format!("invalid music event kind: {other}")),
    }
}

fn music_conversion_error<T: std::fmt::Display>(column: usize, error: T) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            error.to_string(),
        )),
    )
}

fn decode_music_event(row: &rusqlite::Row<'_>) -> Result<MusicRideEvent, rusqlite::Error> {
    let provider = row.get::<_, String>(0)?;
    let item_identifier = row.get::<_, Option<String>>(1)?;
    let title = row.get::<_, Option<String>>(2)?;
    let artist = row.get::<_, Option<String>>(3)?;
    let kind = row.get::<_, String>(4)?;
    let monotonic_ms =
        u64::try_from(row.get::<_, i64>(5)?).map_err(|error| music_conversion_error(5, error))?;
    let wall_clock_ms =
        u64::try_from(row.get::<_, i64>(6)?).map_err(|error| music_conversion_error(6, error))?;
    let clock_uncertainty_ms =
        u64::try_from(row.get::<_, i64>(7)?).map_err(|error| music_conversion_error(7, error))?;
    MusicRideEvent::new(
        parse_provider(&provider).map_err(|error| music_conversion_error(0, error))?,
        item_identifier,
        title,
        artist,
        parse_event_kind(&kind).map_err(|error| music_conversion_error(4, error))?,
        MusicEventTiming {
            observed_at: row
                .get::<_, Option<u64>>(8)?
                .map(cutout_core::MonotonicTimestamp::from_milliseconds),
            monotonic_at: cutout_core::MonotonicTimestamp::from_milliseconds(monotonic_ms),
            wall_clock_at: cutout_core::WallClockUnixTimestamp::from_milliseconds(wall_clock_ms),
            clock_uncertainty_milliseconds: clock_uncertainty_ms,
        },
    )
    .map_err(|error| {
        let column = match error {
            MusicValidationError::BlankIdentifier | MusicValidationError::IdentifierTooLong => 1,
            MusicValidationError::PositionAfterDuration => 5,
            MusicValidationError::ObservationAfterEvent => 8,
            MusicValidationError::EventKindStateMismatch => 4,
            MusicValidationError::ProviderObservationUnsupported => 0,
        };
        music_conversion_error(column, error)
    })
}

fn remember_selected_device(
    connection: &mut Connection,
    platform_identifier: &str,
    display_name: Option<&str>,
    updated_at_ms: u64,
) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    if let Some(display_name) = display_name {
        transaction.execute(
            "INSERT INTO devices (platform_identifier, display_name, updated_at_ms)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(platform_identifier) DO UPDATE SET display_name = excluded.display_name,
                 updated_at_ms = excluded.updated_at_ms",
            params![platform_identifier, display_name, updated_at_ms],
        )?;
    }
    transaction.execute(
        "INSERT INTO selected_device (singleton_key, platform_identifier, updated_at_ms)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(singleton_key) DO UPDATE SET platform_identifier = excluded.platform_identifier,
             updated_at_ms = excluded.updated_at_ms",
        params![SelectedDeviceKey::VALUE.blob(), platform_identifier, updated_at_ms],
    )?;
    transaction.commit()?;
    Ok(())
}

fn clear_selected_device(connection: &Connection) -> Result<(), StorageError> {
    connection.execute(
        "DELETE FROM selected_device WHERE singleton_key = ?1",
        [SelectedDeviceKey::VALUE.blob()],
    )?;
    Ok(())
}
fn save_device_name(
    connection: &Connection,
    platform_identifier: &str,
    display_name: &str,
    updated_at_ms: u64,
) -> Result<(), StorageError> {
    connection.execute(
        "INSERT INTO devices (platform_identifier, display_name, updated_at_ms)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(platform_identifier) DO UPDATE SET display_name = excluded.display_name,
             updated_at_ms = excluded.updated_at_ms",
        params![platform_identifier, display_name, updated_at_ms],
    )?;
    Ok(())
}

fn migrate_device_name(
    connection: &Connection,
    platform_identifier: &str,
    display_name: &str,
    updated_at_ms: u64,
) -> Result<Option<String>, StorageError> {
    save_device_name(connection, platform_identifier, display_name, updated_at_ms)?;
    Ok(Some(display_name.to_owned()))
}

fn device_name(
    connection: &Connection,
    platform_identifier: &str,
) -> Result<Option<String>, StorageError> {
    connection
        .query_row(
            "SELECT display_name FROM devices WHERE platform_identifier = ?1",
            params![platform_identifier],
            |row| row.get(0),
        )
        .optional()
        .map_err(StorageError::from)
}

fn save_voltage_sag_model(
    connection: &Connection,
    device_identity: &str,
    model: VoltageSagModelRecord,
) -> Result<(), StorageError> {
    connection.execute(
        "INSERT INTO voltage_sag_models
            (device_identity, schema_version, effective_resistance_milliohms, observations,
             hardware_verified, last_learned_wall_clock_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(device_identity) DO UPDATE SET schema_version = excluded.schema_version,
             effective_resistance_milliohms = excluded.effective_resistance_milliohms,
             observations = excluded.observations,
             hardware_verified = excluded.hardware_verified,
             last_learned_wall_clock_ms = excluded.last_learned_wall_clock_ms",
        params![
            device_identity,
            model.schema_version,
            model.effective_resistance_milliohms,
            model.observations,
            model.hardware_verified,
            model.last_learned_wall_clock_milliseconds,
        ],
    )?;
    Ok(())
}

fn voltage_sag_model(
    connection: &Connection,
    device_identity: &str,
) -> Result<Option<VoltageSagModelRecord>, StorageError> {
    connection
        .query_row(
            "SELECT schema_version, effective_resistance_milliohms, observations,
                    hardware_verified, last_learned_wall_clock_ms
             FROM voltage_sag_models WHERE device_identity = ?1",
            params![device_identity],
            |row| {
                Ok(VoltageSagModelRecord {
                    schema_version: row.get(0)?,
                    effective_resistance_milliohms: row.get(1)?,
                    observations: row.get(2)?,
                    hardware_verified: row.get(3)?,
                    last_learned_wall_clock_milliseconds: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(StorageError::from)
}

fn remove_voltage_sag_model(
    connection: &Connection,
    device_identity: &str,
) -> Result<(), StorageError> {
    connection.execute(
        "DELETE FROM voltage_sag_models WHERE device_identity = ?1",
        params![device_identity],
    )?;
    Ok(())
}

fn save_ride_session_marker(connection: &Connection, marker: &[u8]) -> Result<(), StorageError> {
    if marker.is_empty() {
        return clear_ride_session_marker(connection);
    }
    connection.execute(
        "INSERT INTO ride_session_marker (singleton_key, marker) VALUES (?1, ?2)
         ON CONFLICT(singleton_key) DO UPDATE SET marker = excluded.marker
         WHERE ride_session_marker.marker != excluded.marker",
        params![RideSessionMarkerKey::VALUE.blob(), marker],
    )?;
    Ok(())
}

fn ride_session_marker(connection: &Connection) -> Result<Option<Vec<u8>>, StorageError> {
    connection
        .query_row(
            "SELECT marker FROM ride_session_marker WHERE singleton_key = ?1",
            [RideSessionMarkerKey::VALUE.blob()],
            |row| row.get(0),
        )
        .optional()
        .map_err(StorageError::from)
}

fn clear_ride_session_marker(connection: &Connection) -> Result<(), StorageError> {
    connection.execute(
        "DELETE FROM ride_session_marker WHERE singleton_key = ?1",
        [RideSessionMarkerKey::VALUE.blob()],
    )?;
    Ok(())
}

fn transition_ride(
    connection: &Connection,
    ride_id: RideId,
    event: RideEvent,
    occurred_at_ms: u64,
    monotonic_at_ms: Option<u64>,
) -> Result<RideLifecycleState, StorageError> {
    transition_ride_in_transaction(
        connection,
        ride_id,
        event,
        occurred_at_ms,
        monotonic_at_ms,
        None,
    )
}

fn transition_ride_with_telemetry(
    connection: &mut Connection,
    ride_id: RideId,
    event: RideEvent,
    occurred_at_ms: u64,
    monotonic_at_ms: Option<u64>,
    last_telemetry_at_ms: Option<u64>,
) -> Result<CommittedRideMovingTimeCheckpoint, StorageError> {
    let transaction = connection.transaction()?;
    let lifecycle = transition_ride_in_transaction(
        &transaction,
        ride_id,
        event,
        occurred_at_ms,
        monotonic_at_ms,
        last_telemetry_at_ms,
    )?;
    let committed = committed_ride_moving_time_checkpoint(&transaction, ride_id, lifecycle)?;
    transaction.commit()?;
    Ok(committed)
}

fn transition_ride_in_transaction(
    connection: &Connection,
    ride_id: RideId,
    event: RideEvent,
    occurred_at_ms: u64,
    monotonic_at_ms: Option<u64>,
    last_telemetry_at_ms: Option<u64>,
) -> Result<RideLifecycleState, StorageError> {
    let write_state = load_ride_write_state(connection, ride_id)?;
    let update = write_state.transition_at(event, occurred_at_ms, monotonic_at_ms)?;
    connection.execute(
        "UPDATE rides SET state = ?2, monotonic_created_at_ms = ?3,
                monotonic_last_event_ms = ?4, paused_at_ms = ?5,
                paused_duration_ms = ?6, completed_duration_ms = ?7,
                updated_at_ms = ?8,
                last_telemetry_at_ms = CASE WHEN ?9 IS NULL THEN last_telemetry_at_ms
                    ELSE MAX(COALESCE(last_telemetry_at_ms, ?9), ?9) END
             WHERE id = ?1",
        params![
            ride_id.uuid().to_string(),
            state_to_db(update.lifecycle()),
            update.monotonic_created_at_milliseconds(),
            update.monotonic_last_event_milliseconds(),
            update.paused_at_milliseconds(),
            update.paused_duration_milliseconds(),
            update.completed_duration_milliseconds(),
            update.updated_at_milliseconds(),
            last_telemetry_at_ms,
        ],
    )?;
    let checkpoint = load_moving_time_checkpoint(connection, ride_id)?;
    let mut continuation = checkpoint.continuation;
    let mut elapsed_time = checkpoint.elapsed_time;
    let elapsed_anchor = elapsed_time.state_parts().anchor_milliseconds;
    let boundary = monotonic_at_ms.or(elapsed_anchor);
    match event {
        RideEvent::Start => {
            if monotonic_at_ms.is_some()
                && connection.query_row(
                    "SELECT source = 'live' FROM rides WHERE id = ?1",
                    [ride_id.uuid().to_string()],
                    |row| row.get::<_, bool>(0),
                )?
            {
                continuation = MovingTimeContinuation::new_live(MonotonicMilliseconds::new(
                    monotonic_at_ms.unwrap_or_default(),
                ));
                elapsed_time = ElapsedTimeContinuation::started(MonotonicMilliseconds::new(
                    update
                        .monotonic_last_event_milliseconds()
                        .unwrap_or_default(),
                ));
            }
        }
        RideEvent::Resume => {
            if let Some(at) = monotonic_at_ms {
                continuation
                    .resume_at(MonotonicMilliseconds::new(at))
                    .map_err(moving_time_storage_error)?;
                elapsed_time.resumed(MonotonicMilliseconds::new(at));
            }
        }
        RideEvent::Pause => {
            if let Some(at) = boundary {
                continuation
                    .close_at(MonotonicMilliseconds::new(at))
                    .map_err(moving_time_storage_error)?;
                elapsed_time.paused(MonotonicMilliseconds::new(at));
            }
        }
        RideEvent::Interrupt => {
            if let Some(at) = boundary {
                continuation
                    .close_at(MonotonicMilliseconds::new(at))
                    .map_err(moving_time_storage_error)?;
                elapsed_time.interrupted(MonotonicMilliseconds::new(at));
            }
        }
        RideEvent::Stop | RideEvent::Discard | RideEvent::Save => {
            if let Some(at) = boundary {
                continuation
                    .close_at(MonotonicMilliseconds::new(at))
                    .map_err(moving_time_storage_error)?;
                elapsed_time.stopped(MonotonicMilliseconds::new(at));
            } else {
                elapsed_time.stopped(MonotonicMilliseconds::new(0));
            }
        }
        RideEvent::Import => {}
    }
    let revision =
        checkpoint
            .revision
            .checked_add(1)
            .ok_or_else(|| StorageError::InvalidStoredValue {
                field: "moving-time revision",
                value: checkpoint.revision.to_string(),
            })?;
    save_moving_time_checkpoint(
        connection,
        ride_id,
        revision,
        continuation,
        elapsed_time,
        None,
        None,
    )?;
    Ok(update.lifecycle())
}

fn moving_time_storage_error(error: cutout_ride_maps::MovingTimeError) -> StorageError {
    StorageError::InvalidStoredValue {
        field: "moving-time lifecycle boundary",
        value: error.to_string(),
    }
}

fn append_location(
    connection: &mut Connection,
    ride_id: RideId,
    sample: LocationSample,
    segment_id: RideMapSegmentId,
    telemetry_state: RouteTelemetryState,
) -> Result<LocationAdmission, StorageError> {
    let result = append_location_with_result(
        connection,
        ride_id,
        sample,
        segment_id,
        telemetry_state,
        SegmentStartReasonInput::Infer,
    )?;
    Ok(result.admission())
}

fn append_location_with_result(
    connection: &mut Connection,
    ride_id: RideId,
    sample: LocationSample,
    segment_id: RideMapSegmentId,
    telemetry_state: RouteTelemetryState,
    start_reason: SegmentStartReasonInput,
) -> Result<LocationWriteResult, StorageError> {
    append_location_with_moving_time(
        connection,
        ride_id,
        sample,
        segment_id,
        telemetry_state,
        start_reason,
        None,
        None,
        None,
    )
}

#[allow(
    clippy::too_many_arguments,
    reason = "this wrapper forwards the complete accepted-location transaction payload"
)]
fn append_location_with_moving_time(
    connection: &mut Connection,
    ride_id: RideId,
    sample: LocationSample,
    segment_id: RideMapSegmentId,
    telemetry_state: RouteTelemetryState,
    start_reason: SegmentStartReasonInput,
    moving_time_observation: Option<MovingTimeObservation>,
    latest_vehicle_moving_time_observation: Option<MovingTimeObservation>,
    inactivity_evidence: Option<RideInactivityEvidence>,
) -> Result<LocationWriteResult, StorageError> {
    validate_paired_moving_time_observations(
        sample,
        moving_time_observation,
        latest_vehicle_moving_time_observation,
    )?;
    let transaction = connection.transaction()?;
    let result = append_location_in_transaction(
        &transaction,
        ride_id,
        sample,
        segment_id,
        telemetry_state,
        start_reason,
        LocationWriteMode::Live,
        moving_time_observation,
        latest_vehicle_moving_time_observation,
        inactivity_evidence,
    )?;
    transaction.commit()?;
    Ok(result)
}

fn validate_paired_moving_time_observations(
    sample: LocationSample,
    moving_time_observation: Option<MovingTimeObservation>,
    latest_vehicle_moving_time_observation: Option<MovingTimeObservation>,
) -> Result<(), StorageError> {
    if let (Some(phone), Some(vehicle)) = (
        moving_time_observation,
        latest_vehicle_moving_time_observation,
    ) && phone.receipt_at != vehicle.receipt_at
    {
        return Err(StorageError::InvalidStoredValue {
            field: "moving-time observation receipt",
            value: "paired phone and vehicle observations must share a receipt".to_owned(),
        });
    }
    if moving_time_observation.is_some_and(|observation| {
        observation.source_at != sample.monotonic_milliseconds()
            || observation.source != MovingTimeSource::PhoneGps
    }) {
        return Err(StorageError::InvalidStoredValue {
            field: "phone moving-time observation",
            value: "source must match the accepted location timestamp".to_owned(),
        });
    }
    if latest_vehicle_moving_time_observation
        .is_some_and(|observation| observation.source != MovingTimeSource::Vehicle)
    {
        return Err(StorageError::InvalidStoredValue {
            field: "verified vehicle moving-time observation",
            value: "source must be verified vehicle evidence".to_owned(),
        });
    }
    Ok(())
}

fn load_previous_ride_point(
    connection: &Connection,
    ride_id: RideId,
) -> Result<Option<PreviousRidePoint>, StorageError> {
    connection
        .query_row(
            "SELECT point.sequence, point.segment_id,
                    CASE WHEN ride.stationary_latitude_e7 IS NULL
                         AND ride.last_location_observed_monotonic_ms >= point.monotonic_ms
                         AND ride.last_location_observed_wall_clock_ms > 0
                         THEN ride.last_location_observed_monotonic_ms ELSE point.monotonic_ms END,
                    CASE WHEN ride.stationary_latitude_e7 IS NULL
                         AND ride.last_location_observed_monotonic_ms >= point.monotonic_ms
                         AND ride.last_location_observed_wall_clock_ms > 0
                         THEN ride.last_location_observed_wall_clock_ms ELSE point.wall_clock_ms END,
                    point.latitude_e7, point.longitude_e7, point.horizontal_accuracy_mm, point.source,
                    ride.last_location_observed_monotonic_ms, ride.last_location_observed_wall_clock_ms
             FROM ride_points AS point JOIN rides AS ride ON ride.id = point.ride_id
             WHERE point.ride_id = ?1 ORDER BY point.sequence DESC LIMIT 1",
            params![ride_id.uuid().to_string()],
            |row| {
                read_location_observation_clocks(row, 8, 9)?;
                let coordinate =
                    Coordinate::from_fixed_parts(row.get(4)?, row.get(5)?).map_err(|_| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Integer,
                            Box::new(StorageError::InvalidStoredValue {
                                field: "coordinate",
                                value: "out of range".to_owned(),
                            }),
                        )
                    })?;
                let source =
                    source_from_db(row.get::<_, String>(7)?.as_str()).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            7,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
                Ok(PreviousRidePoint {
                    sequence: row.get::<_, i64>(0)?,
                    segment_id: RideMapSegmentId::new(row.get::<_, u64>(1)?),
                    sample: LocationSample::new(
                        coordinate,
                        row.get::<_, u64>(2)?,
                        row.get::<_, u64>(3)?,
                        row.get::<_, Option<u32>>(6)?,
                        source,
                    ),
                })
            },
        )
        .optional()
        .map_err(StorageError::from)
}

fn checkpoint_stationary_location_observation(
    connection: &Connection,
    checkpoint: RideStationaryLocationCheckpoint,
) -> Result<RideLocationObservationCheckpointOutcome, StorageError> {
    use RideLocationObservationCheckpointOutcome::{AlreadyObserved, Applied, PointChanged};
    let transaction = connection.unchecked_transaction()?;
    let ride = find_ride(&transaction, checkpoint.ride_id)?.ok_or(StorageError::NotFound)?;
    let observation = checkpoint.observation;
    if ride.state() != RideLifecycleState::Active
        || ride.source() != RideSource::Live
        || ride.summary().point_count().as_u64() != checkpoint.expected_point_count
        || observation.source() != LocationSource::Live
        || observation.wall_clock_unix_milliseconds().as_u64() == 0
        || ride
            .monotonic_last_event_milliseconds()
            .is_some_and(|at| observation.monotonic_milliseconds().as_u64() < at)
        || ride.motion_observation().is_none_or(|motion| {
            !motion.parked
                && motion.observed_at_milliseconds <= observation.monotonic_milliseconds().as_u64()
        })
    {
        return Ok(PointChanged);
    }
    let previous_point = load_previous_ride_point(&transaction, checkpoint.ride_id)?;
    let previous = ride
        .stationary_location_observation()
        .or_else(|| previous_point.map(|point| point.sample));
    if previous == Some(observation) {
        return Ok(AlreadyObserved);
    }
    if route_admission(
        ride.monotonic_created_at_milliseconds()
            .map(MonotonicMilliseconds::new),
        previous.as_ref(),
        &observation,
    ) != LocationAdmission::Accepted
    {
        return Ok(PointChanged);
    }
    transaction.execute(
        "UPDATE rides SET last_location_observed_monotonic_ms = ?2,
             last_location_observed_wall_clock_ms = ?3,
             stationary_latitude_e7 = ?4, stationary_longitude_e7 = ?5,
             stationary_horizontal_accuracy_mm = ?6,
             monotonic_last_event_ms = MAX(COALESCE(monotonic_last_event_ms, ?2), ?2),
             updated_at_ms = MAX(updated_at_ms, ?3) WHERE id = ?1",
        params![
            checkpoint.ride_id.uuid().to_string(),
            observation.monotonic_milliseconds().as_u64(),
            observation.wall_clock_unix_milliseconds().as_u64(),
            observation.coordinate().latitude().as_i32(),
            observation.coordinate().longitude().as_i32(),
            observation.horizontal_accuracy_millimetres()
        ],
    )?;
    transaction.commit()?;
    Ok(Applied)
}

fn checkpoint_location_observation(
    connection: &Connection,
    checkpoint: RideLocationObservationCheckpoint,
) -> Result<RideLocationObservationCheckpointOutcome, StorageError> {
    use RideLocationObservationCheckpointOutcome::{AlreadyObserved, Applied, PointChanged};
    let RideLocationObservationCheckpoint {
        ride_id,
        expected_point_sequence,
        observation,
    } = checkpoint;
    // Revalidation and the row update share a snapshot even if another connection writes.
    let transaction = connection.unchecked_transaction()?;
    let ride = find_ride(&transaction, ride_id)?.ok_or(StorageError::NotFound)?;
    if ride.stationary_location_observation().is_some() {
        return Ok(PointChanged);
    }
    let Some(previous) = load_previous_ride_point(&transaction, ride_id)? else {
        return Ok(PointChanged);
    };
    if ride.state() != RideLifecycleState::Active
        || u64::try_from(previous.sequence).ok() != Some(expected_point_sequence)
        || previous.sample.coordinate() != observation.coordinate()
        || previous.sample.horizontal_accuracy_millimetres()
            != observation.horizontal_accuracy_millimetres()
        || previous.sample.source() != observation.source()
        || observation.source() != LocationSource::Live
        || observation.wall_clock_unix_milliseconds().as_u64() == 0
        || observation.monotonic_milliseconds() < previous.sample.monotonic_milliseconds()
    {
        return Ok(PointChanged);
    }
    if observation.monotonic_milliseconds() == previous.sample.monotonic_milliseconds() {
        return Ok(
            if observation.wall_clock_unix_milliseconds()
                == previous.sample.wall_clock_unix_milliseconds()
            {
                AlreadyObserved
            } else {
                PointChanged
            },
        );
    }
    transaction.execute(
        "UPDATE rides SET last_location_observed_monotonic_ms = ?2,
             last_location_observed_wall_clock_ms = ?3,
             monotonic_last_event_ms = MAX(COALESCE(monotonic_last_event_ms, ?2), ?2),
             updated_at_ms = MAX(updated_at_ms, ?3)
         WHERE id = ?1",
        params![
            ride_id.uuid().to_string(),
            observation.monotonic_milliseconds().as_u64(),
            observation.wall_clock_unix_milliseconds().as_u64()
        ],
    )?;
    transaction.commit()?;
    Ok(Applied)
}

#[allow(
    clippy::too_many_arguments,
    reason = "point admission forwards the complete route and metric transaction payload"
)]
fn append_location_in_transaction(
    connection: &Connection,
    ride_id: RideId,
    sample: LocationSample,
    segment_id: RideMapSegmentId,
    telemetry_state: RouteTelemetryState,
    start_reason: SegmentStartReasonInput,
    mode: LocationWriteMode,
    moving_time_observation: Option<MovingTimeObservation>,
    latest_vehicle_moving_time_observation: Option<MovingTimeObservation>,
    inactivity_evidence: Option<RideInactivityEvidence>,
) -> Result<LocationWriteResult, StorageError> {
    let write_state = load_ride_write_state(connection, ride_id)?;
    let previous = load_previous_ride_point(connection, ride_id)?;
    let decision = write_state
        .decide_location(
            previous
                .as_ref()
                .map(|previous| (previous.segment_id.value(), previous.sample)),
            sample,
            segment_id,
            mode,
        )
        .map_err(StorageError::InvalidRideState)?;
    match decision {
        LocationWriteDecision::Accepted {
            distance_millimetres,
            updated_at_ms,
        } => {
            ensure_ride_segment(
                connection,
                ride_id,
                previous.as_ref(),
                sample,
                segment_id,
                start_reason,
            )?;
            let sequence = previous.map_or(0, |previous| previous.sequence + 1);
            let durable_sequence =
                u64::try_from(sequence).map_err(|_| StorageError::InvalidStoredValue {
                    field: "ride_points.sequence",
                    value: sequence.to_string(),
                })?;
            let insert = LocationInsert {
                ride_id,
                sequence,
                sample,
                segment_id,
                telemetry_state,
                distance_millimetres,
                updated_at_ms,
                last_recorded_point_duration_milliseconds: match mode {
                    LocationWriteMode::Live => {
                        write_state.duration_at_live_point(sample.monotonic_milliseconds())
                    }
                    LocationWriteMode::PevcapImport => None,
                },
            };
            // Admission already identifies duplicates. An insert or summary update
            // failure must abort the transaction rather than commit a partial point.
            insert_location(connection, &insert)?;
            persist_location_moving_time_checkpoint(
                connection,
                ride_id,
                mode,
                write_state.lifecycle(),
                sample,
                moving_time_observation,
                latest_vehicle_moving_time_observation,
            )?;
            if let Some(evidence) = inactivity_evidence {
                ride_inactivity::persist_snapshot(connection, ride_id, evidence)?;
            }
            Ok(LocationWriteResult {
                admission: LocationAdmission::Accepted,
                sequence: Some(durable_sequence),
                moving_time_checkpoint: load_moving_time_checkpoint(connection, ride_id)?,
            })
        }
        LocationWriteDecision::Rejected(admission) => Ok(LocationWriteResult {
            admission,
            sequence: None,
            moving_time_checkpoint: load_moving_time_checkpoint(connection, ride_id)?,
        }),
    }
}

fn elapsed_prefix_for_live_point(
    elapsed_time: &mut ElapsedTimeContinuation,
    lifecycle: RideLifecycleState,
    at: MonotonicMilliseconds,
) -> ElapsedTimeSummary {
    let parts = elapsed_time.state_parts();
    if parts.running {
        if parts
            .anchor_milliseconds
            .is_some_and(|anchor| at.as_u64() >= anchor)
        {
            let prefix = elapsed_time.projected_at(at);
            elapsed_time.observe(at);
            return prefix;
        }
    } else if lifecycle == RideLifecycleState::Active || lifecycle == RideLifecycleState::Paused {
        elapsed_time.resumed(at);
        return elapsed_time.summary();
    }
    ElapsedTimeSummary::Unavailable
}

fn apply_location_vehicle_observation(
    connection: &Connection,
    ride_id: RideId,
    continuation: &mut MovingTimeContinuation,
    mut observation: MovingTimeObservation,
) -> Result<(), StorageError> {
    if observation.source != MovingTimeSource::Vehicle {
        return Err(StorageError::InvalidStoredValue {
            field: "moving-time source",
            value: "expected verified vehicle evidence".to_owned(),
        });
    }
    let ride = find_ride(connection, ride_id)?.ok_or(StorageError::NotFound)?;
    let parts = continuation.state_parts();
    let duplicate =
        parts.latest_vehicle_source_milliseconds == Some(observation.source_at.as_u64());
    if duplicate
        && parts.vehicle.is_some_and(|value| {
            value.source_at_milliseconds == observation.source_at.as_u64()
                && Some(value.speed_millimetres_per_second)
                    == observation.speed_millimetres_per_second
        })
    {
        // Replaying cached evidence retains its original source time and expiry.
    } else if duplicate {
        return Err(StorageError::InvalidStoredValue {
            field: "verified vehicle moving-time observation",
            value: "repeated source time changed its speed".to_owned(),
        });
    } else if parts
        .latest_vehicle_source_milliseconds
        .is_some_and(|latest| latest > observation.source_at.as_u64())
    {
        // A delayed cached value is obsolete and cannot move the source watermark.
    } else {
        if ride.associated_vehicle().is_none()
            || ride
                .last_telemetry_at_milliseconds()
                .is_none_or(|latest| latest < observation.source_at.as_u64())
        {
            return Err(StorageError::InvalidStoredValue {
                field: "verified vehicle moving-time observation",
                value: "source is not current for an associated vehicle".to_owned(),
            });
        }
        if observation.speed_millimetres_per_second.is_none() {
            return Err(StorageError::InvalidStoredValue {
                field: "verified vehicle moving-time observation",
                value: "vehicle speed is required".to_owned(),
            });
        }
        if !moving_time_observation_fresh(observation) {
            observation.speed_millimetres_per_second = None;
        }
        if observation.speed_millimetres_per_second.is_some() {
            continuation
                .observe(observation)
                .map_err(moving_time_storage_error)?;
        }
    }
    Ok(())
}

fn persist_location_moving_time_checkpoint(
    connection: &Connection,
    ride_id: RideId,
    mode: LocationWriteMode,
    lifecycle: RideLifecycleState,
    sample: LocationSample,
    moving_time_observation: Option<MovingTimeObservation>,
    latest_vehicle_moving_time_observation: Option<MovingTimeObservation>,
) -> Result<(), StorageError> {
    let current = load_moving_time_checkpoint(connection, ride_id)?;
    let mut continuation = current.continuation;
    let mut elapsed_time = current.elapsed_time;
    let prefix = continuation.prefix_before_source_time(sample.monotonic_milliseconds());
    let is_live = match mode {
        LocationWriteMode::Live => true,
        LocationWriteMode::PevcapImport => false,
    };
    let elapsed_point_prefix = if is_live {
        elapsed_prefix_for_live_point(
            &mut elapsed_time,
            lifecycle,
            sample.monotonic_milliseconds(),
        )
    } else {
        ElapsedTimeSummary::Unavailable
    };
    let elapsed_changed = elapsed_time != current.elapsed_time;
    if moving_time_observation.is_none()
        && latest_vehicle_moving_time_observation.is_none()
        && !elapsed_changed
        && !is_live
    {
        return Ok(());
    }
    if let Some(observation) = latest_vehicle_moving_time_observation {
        apply_location_vehicle_observation(connection, ride_id, &mut continuation, observation)?;
    }
    if let Some(mut observation) = moving_time_observation {
        if !moving_time_observation_fresh(observation) {
            observation.speed_millimetres_per_second = None;
        }
        continuation
            .observe(observation)
            .map_err(moving_time_storage_error)?;
    }
    let revision =
        current
            .revision
            .checked_add(1)
            .ok_or_else(|| StorageError::InvalidStoredValue {
                field: "moving-time revision",
                value: current.revision.to_string(),
            })?;
    save_moving_time_checkpoint(
        connection,
        ride_id,
        revision,
        continuation,
        elapsed_time,
        Some(prefix.unwrap_or(MovingTimeSummary {
            moving: cutout_ride_maps::RideDurationMilliseconds::new(0),
            observed: cutout_ride_maps::RideDurationMilliseconds::new(0),
            unknown: cutout_ride_maps::RideDurationMilliseconds::new(0),
            availability: MovingTimeAvailability::Unavailable,
        })),
        is_live.then_some(elapsed_point_prefix),
    )
}

fn load_moving_time_checkpoint(
    connection: &Connection,
    ride_id: RideId,
) -> Result<MovingTimeCheckpoint, StorageError> {
    connection
        .query_row(
            "SELECT moving_time_revision, moving_time_ms, moving_time_observed_ms,
                    moving_time_unknown_ms, moving_time_cursor_ms,
                    moving_time_phone_speed_mm_s, moving_time_phone_source_ms,
                    moving_time_vehicle_speed_mm_s, moving_time_vehicle_source_ms,
                    moving_time_phone_watermark_ms, moving_time_vehicle_watermark_ms,
                    moving_time_incomplete_history, moving_time_availability,
                    elapsed_time_ms, elapsed_time_anchor_ms,
                    elapsed_time_running, elapsed_time_partial
             FROM rides WHERE id = ?1",
            [ride_id.uuid().to_string()],
            |row| {
                let revision: u64 = row.get(0)?;
                let phone_speed: Option<i32> = row.get(5)?;
                let phone_source_at: Option<u64> = row.get(6)?;
                let vehicle_speed: Option<i32> = row.get(7)?;
                let vehicle_source_at: Option<u64> = row.get(8)?;
                let availability_value: String = row.get(12)?;
                let availability =
                    moving_time_availability_from_db(&availability_value).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            12,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
                let phone_gps = match (phone_speed, phone_source_at) {
                    (Some(speed_millimetres_per_second), Some(source_at_milliseconds)) => {
                        Some(MovingTimeSourceStateParts {
                            speed_millimetres_per_second,
                            source_at_milliseconds,
                        })
                    }
                    (None, None) => None,
                    _ => return Err(rusqlite::Error::InvalidQuery),
                };
                let vehicle = match (vehicle_speed, vehicle_source_at) {
                    (Some(speed_millimetres_per_second), Some(source_at_milliseconds)) => {
                        Some(MovingTimeSourceStateParts {
                            speed_millimetres_per_second,
                            source_at_milliseconds,
                        })
                    }
                    (None, None) => None,
                    _ => return Err(rusqlite::Error::InvalidQuery),
                };
                let parts = MovingTimeStateParts {
                    moving_milliseconds: row.get(1)?,
                    observed_milliseconds: row.get(2)?,
                    unknown_milliseconds: row.get(3)?,
                    cursor_milliseconds: row.get(4)?,
                    phone_gps,
                    vehicle,
                    latest_phone_gps_source_milliseconds: row.get(9)?,
                    latest_vehicle_source_milliseconds: row.get(10)?,
                    incomplete_history: row.get::<_, bool>(11)?,
                    availability,
                };
                let continuation =
                    MovingTimeContinuation::from_state_parts(parts).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Integer,
                            Box::new(error),
                        )
                    })?;
                let elapsed_parts = ElapsedTimeStateParts {
                    duration_milliseconds: row.get(13)?,
                    anchor_milliseconds: row.get(14)?,
                    running: row.get(15)?,
                    partial: row.get(16)?,
                };
                let elapsed_time = ElapsedTimeContinuation::from_state_parts(elapsed_parts)
                    .map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            13,
                            rusqlite::types::Type::Integer,
                            Box::new(error),
                        )
                    })?;
                Ok(MovingTimeCheckpoint {
                    revision,
                    continuation,
                    elapsed_time,
                })
            },
        )
        .optional()?
        .ok_or(StorageError::NotFound)
}

fn moving_time_availability_from_db(value: &str) -> Result<MovingTimeAvailability, StorageError> {
    match value {
        "unavailable" => Ok(MovingTimeAvailability::Unavailable),
        "partial" => Ok(MovingTimeAvailability::Partial),
        "complete" => Ok(MovingTimeAvailability::Complete),
        _ => Err(StorageError::InvalidStoredValue {
            field: "moving-time availability",
            value: value.to_owned(),
        }),
    }
}

fn moving_time_availability_to_db(value: MovingTimeAvailability) -> &'static str {
    match value {
        MovingTimeAvailability::Unavailable => "unavailable",
        MovingTimeAvailability::Partial => "partial",
        MovingTimeAvailability::Complete => "complete",
    }
}

fn moving_time_observation_fresh(observation: MovingTimeObservation) -> bool {
    observation.receipt_at >= observation.source_at
        && observation.receipt_at.as_u64() - observation.source_at.as_u64()
            <= cutout_ride_maps::TELEMETRY_FRESHNESS_MILLISECONDS
}

fn save_moving_time_checkpoint(
    connection: &Connection,
    ride_id: RideId,
    revision: u64,
    continuation: MovingTimeContinuation,
    elapsed_time: ElapsedTimeContinuation,
    last_recorded_point: Option<MovingTimeSummary>,
    last_recorded_point_elapsed_time: Option<ElapsedTimeSummary>,
) -> Result<(), StorageError> {
    let parts = continuation.state_parts();
    let elapsed_parts = elapsed_time.state_parts();
    connection.execute(
        "UPDATE rides SET moving_time_revision = ?2, moving_time_ms = ?3,
             moving_time_observed_ms = ?4, moving_time_unknown_ms = ?5,
             moving_time_cursor_ms = ?6, moving_time_phone_speed_mm_s = ?7,
             moving_time_phone_source_ms = ?8, moving_time_vehicle_speed_mm_s = ?9,
             moving_time_vehicle_source_ms = ?10, moving_time_phone_watermark_ms = ?11,
             moving_time_vehicle_watermark_ms = ?12, moving_time_incomplete_history = ?13,
             moving_time_availability = ?14,
             elapsed_time_ms = ?15, elapsed_time_anchor_ms = ?16,
             elapsed_time_running = ?17, elapsed_time_partial = ?18
         WHERE id = ?1",
        params![
            ride_id.uuid().to_string(),
            revision,
            parts.moving_milliseconds,
            parts.observed_milliseconds,
            parts.unknown_milliseconds,
            parts.cursor_milliseconds,
            parts
                .phone_gps
                .map(|state| state.speed_millimetres_per_second),
            parts.phone_gps.map(|state| state.source_at_milliseconds),
            parts
                .vehicle
                .map(|state| state.speed_millimetres_per_second),
            parts.vehicle.map(|state| state.source_at_milliseconds),
            parts.latest_phone_gps_source_milliseconds,
            parts.latest_vehicle_source_milliseconds,
            parts.incomplete_history,
            moving_time_availability_to_db(parts.availability),
            elapsed_parts.duration_milliseconds,
            elapsed_parts.anchor_milliseconds,
            elapsed_parts.running,
            elapsed_parts.partial,
        ],
    )?;
    if let Some(prefix) = last_recorded_point {
        connection.execute(
            "UPDATE rides SET last_recorded_point_moving_time_ms = ?2,
                 last_recorded_point_moving_observed_ms = ?3,
                 last_recorded_point_moving_unknown_ms = ?4,
                 last_recorded_point_moving_availability = ?5 WHERE id = ?1",
            params![
                ride_id.uuid().to_string(),
                prefix.moving.as_u64(),
                prefix.observed.as_u64(),
                prefix.unknown.as_u64(),
                moving_time_availability_to_db(prefix.availability),
            ],
        )?;
    }
    if let Some(summary) = last_recorded_point_elapsed_time {
        let (duration, partial) = match summary {
            ElapsedTimeSummary::Unavailable => (None, false),
            ElapsedTimeSummary::Complete { duration } => (Some(duration.as_u64()), false),
            ElapsedTimeSummary::Partial { duration } => (Some(duration.as_u64()), true),
        };
        connection.execute(
            "UPDATE rides SET last_recorded_point_elapsed_time_ms = ?2,
                 last_recorded_point_elapsed_partial = ?3 WHERE id = ?1",
            params![ride_id.uuid().to_string(), duration, partial],
        )?;
    }
    Ok(())
}

fn ensure_ride_segment(
    connection: &Connection,
    ride_id: RideId,
    previous: Option<&PreviousRidePoint>,
    sample: LocationSample,
    segment_id: RideMapSegmentId,
    start_reason: SegmentStartReasonInput,
) -> Result<(), StorageError> {
    let stored_segment_id =
        i64::try_from(segment_id.value()).map_err(|_| StorageError::InvalidStoredValue {
            field: "ride segment identifier",
            value: segment_id.value().to_string(),
        })?;
    let existing = connection
        .query_row(
            "SELECT start_reason, source FROM ride_segments
             WHERE ride_id = ?1 AND segment_id = ?2",
            params![ride_id.uuid().to_string(), stored_segment_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    if let Some((reason, source)) = existing {
        segment_start_reason_from_db(&reason)?;
        let expected_source = source_to_db(sample.source());
        if source != expected_source {
            return Err(StorageError::InvalidStoredValue {
                field: "ride segment source",
                value: source,
            });
        }
        connection.execute(
            "UPDATE ride_segments
             SET ended_monotonic_ms = MAX(COALESCE(ended_monotonic_ms, started_monotonic_ms), ?3),
                 ended_wall_clock_ms = MAX(COALESCE(ended_wall_clock_ms, started_wall_clock_ms), ?4)
             WHERE ride_id = ?1 AND segment_id = ?2",
            params![
                ride_id.uuid().to_string(),
                stored_segment_id,
                sample.monotonic_milliseconds().as_u64(),
                sample.wall_clock_unix_milliseconds().as_u64(),
            ],
        )?;
        return Ok(());
    }

    let start_reason = match start_reason {
        SegmentStartReasonInput::Infer => segment_start_reason(previous, sample, segment_id),
        SegmentStartReasonInput::Recorded(reason) => reason,
    };
    let start_reason = match start_reason {
        RideSegmentStartReason::Initial => "initial",
        RideSegmentStartReason::Resume => "resume",
        RideSegmentStartReason::BackgroundGap => "background_gap",
        RideSegmentStartReason::ImportBoundary => "import_boundary",
        RideSegmentStartReason::Stationary => "stationary",
    };
    connection.execute(
        "INSERT INTO ride_segments
            (ride_id, segment_id, point_count, sequence, start_reason, source,
             started_monotonic_ms, ended_monotonic_ms, started_wall_clock_ms, ended_wall_clock_ms)
         VALUES (?1, ?2, 0, ?2, ?3, ?4, ?5, ?5, ?6, ?6)",
        params![
            ride_id.uuid().to_string(),
            stored_segment_id,
            start_reason,
            source_to_db(sample.source()),
            sample.monotonic_milliseconds().as_u64(),
            sample.wall_clock_unix_milliseconds().as_u64(),
        ],
    )?;
    Ok(())
}

fn segment_start_reason(
    previous: Option<&PreviousRidePoint>,
    sample: LocationSample,
    segment_id: RideMapSegmentId,
) -> RideSegmentStartReason {
    let Some(previous) = previous else {
        return match sample.source() {
            LocationSource::Live => RideSegmentStartReason::Initial,
            LocationSource::PevcapImport => RideSegmentStartReason::ImportBoundary,
        };
    };
    if previous.segment_id == segment_id {
        return RideSegmentStartReason::Initial;
    }
    let gap_started = sample
        .monotonic_milliseconds()
        .as_u64()
        .saturating_sub(previous.sample.monotonic_milliseconds().as_u64())
        > cutout_ride_maps::MAX_GAP_MILLISECONDS;
    if gap_started {
        RideSegmentStartReason::BackgroundGap
    } else {
        RideSegmentStartReason::Resume
    }
}

struct StoredRideWriteState {
    source: String,
    lifecycle: String,
    monotonic_created_at_ms: Option<u64>,
    monotonic_last_event_ms: Option<u64>,
    latest_observed_monotonic_ms: Option<u64>,
    stationary_location_observation: Option<LocationSample>,
    paused_at_ms: Option<u64>,
    paused_duration_ms: u64,
    completed_duration_ms: u64,
    last_recorded_point_duration_ms: Option<u64>,
    updated_at_ms: u64,
}

fn load_ride_write_state(
    connection: &Connection,
    ride_id: RideId,
) -> Result<RideWriteState, StorageError> {
    let stored = connection
        .query_row(
            "SELECT source, state, monotonic_created_at_ms, monotonic_last_event_ms,
                    (SELECT MAX(monotonic_ms) FROM ride_points WHERE ride_id = rides.id),
                    paused_at_ms, paused_duration_ms, completed_duration_ms, updated_at_ms,
                    stationary_latitude_e7, stationary_longitude_e7, stationary_horizontal_accuracy_mm,
                    last_location_observed_monotonic_ms, last_location_observed_wall_clock_ms,
                    last_recorded_point_duration_ms
             FROM rides WHERE id = ?1",
            params![ride_id.uuid().to_string()],
            |row| {
                Ok(StoredRideWriteState {
                    source: row.get(0)?,
                    lifecycle: row.get(1)?,
                    monotonic_created_at_ms: row.get(2)?,
                    monotonic_last_event_ms: row.get(3)?,
                    latest_observed_monotonic_ms: row.get(4)?,
                    paused_at_ms: row.get(5)?,
                    paused_duration_ms: row.get(6)?,
                    completed_duration_ms: row.get(7)?,
                    updated_at_ms: row.get(8)?,
                    stationary_location_observation: read_stationary_observation(row, 9, 10, 11, 12, 13)?,
                    last_recorded_point_duration_ms: row.get(14)?,
                })
            },
        )
        .optional()?
        .ok_or(StorageError::NotFound)?;
    Ok(RideWriteState::from_parts(&RideWriteStateParts {
        source: ride_source_from_db(&stored.source)?,
        lifecycle: state_from_db(&stored.lifecycle)?,
        monotonic_created_at_ms: stored.monotonic_created_at_ms,
        monotonic_last_event_ms: stored.monotonic_last_event_ms,
        latest_observed_monotonic_ms: stored.latest_observed_monotonic_ms,
        stationary_location_observation: stored.stationary_location_observation,
        paused_at_ms: stored.paused_at_ms,
        paused_duration_ms: stored.paused_duration_ms,
        completed_duration_ms: stored.completed_duration_ms,
        last_recorded_point_duration_ms: stored.last_recorded_point_duration_ms,
        updated_at_ms: stored.updated_at_ms,
    }))
}

#[derive(Clone, Copy)]
struct LocationInsert {
    ride_id: RideId,
    sequence: i64,
    sample: LocationSample,
    segment_id: RideMapSegmentId,
    telemetry_state: RouteTelemetryState,
    distance_millimetres: u64,
    updated_at_ms: u64,
    last_recorded_point_duration_milliseconds: Option<cutout_ride_maps::RideDurationMilliseconds>,
}

fn insert_location(connection: &Connection, insert: &LocationInsert) -> Result<(), StorageError> {
    let LocationInsert {
        ride_id,
        sequence,
        sample,
        segment_id,
        telemetry_state,
        distance_millimetres,
        updated_at_ms,
        last_recorded_point_duration_milliseconds,
    } = *insert;
    connection.execute(
        "INSERT INTO ride_points
            (ride_id, sequence, segment_id, telemetry_state, monotonic_ms, wall_clock_ms, latitude_e7, longitude_e7, horizontal_accuracy_mm, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            ride_id.uuid().to_string(),
            sequence,
            segment_id.value(),
            telemetry_state.storage_value(),
            sample.monotonic_milliseconds().as_u64(),
            sample.wall_clock_unix_milliseconds().as_u64(),
            sample.coordinate().latitude().as_i32(),
            sample.coordinate().longitude().as_i32(),
            sample.horizontal_accuracy_millimetres(),
            source_to_db(sample.source()),
        ],
    )?;
    connection.execute(
        "UPDATE rides SET point_count = point_count + 1, distance_mm = distance_mm + ?2,
         monotonic_last_event_ms = CASE
             WHEN monotonic_last_event_ms IS NULL THEN ?4
             ELSE MAX(monotonic_last_event_ms, ?4)
         END,
         updated_at_ms = ?3,
         last_location_observed_monotonic_ms = ?4,
         last_location_observed_wall_clock_ms = ?5,
         last_recorded_point_duration_ms = ?6,
         stationary_latitude_e7 = NULL, stationary_longitude_e7 = NULL,
         stationary_horizontal_accuracy_mm = NULL WHERE id = ?1",
        params![
            ride_id.uuid().to_string(),
            distance_millimetres,
            updated_at_ms,
            sample.monotonic_milliseconds().as_u64(),
            sample.wall_clock_unix_milliseconds().as_u64(),
            last_recorded_point_duration_milliseconds
                .map(cutout_ride_maps::RideDurationMilliseconds::as_u64),
        ],
    )?;
    connection.execute(
        "UPDATE ride_segments
         SET point_count = point_count + 1
         WHERE ride_id = ?1 AND segment_id = ?2",
        params![ride_id.uuid().to_string(), segment_id.value()],
    )?;
    Ok(())
}

fn load_summary(connection: &Connection, ride_id: RideId) -> Result<RideSummary, StorageError> {
    connection
        .query_row(
            "SELECT point_count, distance_mm FROM rides WHERE id = ?1",
            params![ride_id.uuid().to_string()],
            |row| {
                Ok(RideSummary::from_stored(
                    row.get::<_, u64>(0)?.into(),
                    row.get::<_, u64>(1)?,
                ))
            },
        )
        .optional()?
        .ok_or(StorageError::NotFound)
}

fn load_summary_with_duration(
    connection: &Connection,
    ride_id: RideId,
) -> Result<(RideSummary, u64), StorageError> {
    let ride = find_ride(connection, ride_id)?.ok_or(StorageError::NotFound)?;
    Ok((ride.summary(), ride.duration_milliseconds()))
}

fn find_ride(connection: &Connection, ride_id: RideId) -> Result<Option<RideRecord>, StorageError> {
    connection
        .query_row(
            "SELECT id, source, state, created_at_ms, monotonic_created_at_ms,
                    monotonic_last_event_ms, paused_at_ms, paused_duration_ms,
                    completed_duration_ms, updated_at_ms,
                    CASE
                        WHEN state IN ('stopped', 'interrupted', 'saved', 'discarded', 'imported')
                            THEN completed_duration_ms
                        WHEN monotonic_created_at_ms IS NOT NULL
                            AND monotonic_last_event_ms IS NOT NULL
                            THEN MAX(0, monotonic_last_event_ms - monotonic_created_at_ms
                                - paused_duration_ms
                                - CASE WHEN state = 'paused' AND paused_at_ms IS NOT NULL
                                    THEN monotonic_last_event_ms - paused_at_ms ELSE 0 END)
                        WHEN (SELECT MAX(monotonic_ms) FROM ride_points WHERE ride_id = rides.id)
                                 IS NULL
                            OR (monotonic_created_at_ms IS NOT NULL
                                AND (SELECT MAX(monotonic_ms) FROM ride_points WHERE ride_id = rides.id)
                                    < monotonic_created_at_ms)
                            THEN 0
                        WHEN monotonic_created_at_ms IS NOT NULL
                            THEN (SELECT MAX(monotonic_ms) FROM ride_points WHERE ride_id = rides.id)
                                 - monotonic_created_at_ms
                        ELSE (SELECT MAX(monotonic_ms) - MIN(monotonic_ms)
                              FROM ride_points WHERE ride_id = rides.id)
                    END,
                    point_count, distance_mm,
                    (SELECT COUNT(*) FROM ride_segments
                     WHERE ride_id = rides.id AND point_count > 0),
                    candidate_vehicle, associated_vehicle, associated_at_ms, last_telemetry_at_ms,
                    (SELECT display_name FROM devices
                     WHERE platform_identifier = rides.candidate_vehicle),
                    (SELECT display_name FROM devices
                     WHERE platform_identifier = rides.associated_vehicle),
                    (SELECT COUNT(*) FROM ride_segments
                     WHERE ride_id = rides.id AND point_count > 0 AND start_reason = 'background_gap'),
                    rides.last_location_observed_monotonic_ms, rides.last_location_observed_wall_clock_ms,
                    rides.motion_parked, rides.motion_observed_monotonic_ms, rides.stationary_latitude_e7,
                    rides.stationary_longitude_e7, rides.stationary_horizontal_accuracy_mm, rides.last_stationary_motion_monotonic_ms,
                    rides.last_recorded_point_duration_ms,
                    rides.moving_time_revision, rides.moving_time_ms, rides.moving_time_observed_ms,
                    rides.moving_time_unknown_ms, rides.moving_time_cursor_ms,
                    rides.moving_time_phone_speed_mm_s, rides.moving_time_phone_source_ms,
                    rides.moving_time_vehicle_speed_mm_s, rides.moving_time_vehicle_source_ms,
                    rides.moving_time_phone_watermark_ms, rides.moving_time_vehicle_watermark_ms,
                    rides.moving_time_incomplete_history, rides.moving_time_availability,
                    rides.last_recorded_point_moving_time_ms,
                    rides.last_recorded_point_moving_observed_ms,
                    rides.last_recorded_point_moving_unknown_ms,
                    rides.last_recorded_point_moving_availability,
                    rides.elapsed_time_ms, rides.elapsed_time_anchor_ms,
                    rides.elapsed_time_running, rides.elapsed_time_partial,
                    rides.last_recorded_point_elapsed_time_ms,
                    rides.last_recorded_point_elapsed_partial
             FROM rides
             WHERE state NOT IN ('draft', 'discarded') AND id = ?1",
            params![ride_id.uuid().to_string()],
            ride_record_from_row,
        )
        .optional()
        .map_err(StorageError::from)
}

fn find_ride_filtered(
    connection: &Connection,
    ride_id: RideId,
    query: &RideHistoryQuery,
) -> Result<Option<RideRecord>, StorageError> {
    let (filters_sql, mut parameters) = history_filter_predicates(query, 2);
    let sql = format!(
        "SELECT 1 FROM rides
         LEFT JOIN devices AS associated_device
             ON associated_device.platform_identifier = rides.associated_vehicle
         LEFT JOIN devices AS candidate_device
             ON candidate_device.platform_identifier = rides.candidate_vehicle
         WHERE rides.id = ?1 AND rides.state NOT IN ('draft', 'discarded'){filters_sql}"
    );
    parameters.insert(0, Value::Text(ride_id.uuid().to_string()));
    let visible = connection
        .query_row(&sql, params_from_iter(parameters.iter()), |_| Ok(()))
        .optional()?
        .is_some();
    if visible {
        find_ride(connection, ride_id)
    } else {
        Ok(None)
    }
}

fn history_filter_predicates(
    query: &RideHistoryQuery,
    first_parameter_index: usize,
) -> (String, Vec<Value>) {
    let mut predicates = Vec::new();
    let mut parameters = Vec::new();
    if let Some(created_after) = query.created_after_ms {
        let parameter_index = first_parameter_index + parameters.len();
        predicates.push(format!(" AND rides.created_at_ms >= ?{parameter_index}"));
        parameters.push(Value::Integer(
            i64::try_from(created_after.as_u64()).unwrap_or(i64::MAX),
        ));
    }
    if let Some(vehicle_identity) = query.vehicle_identity.as_ref() {
        let parameter_index = first_parameter_index + parameters.len();
        predicates.push(format!(
            " AND (rides.associated_vehicle = ?{parameter_index} OR rides.candidate_vehicle = ?{parameter_index})"
        ));
        parameters.push(Value::Text(vehicle_identity.as_str().to_owned()));
    }
    if let Some(search_text) = query.search_text.as_deref() {
        let parameter_index = first_parameter_index + parameters.len();
        let escaped = search_text
            .to_lowercase()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        predicates.push(format!(
            " AND (lower(rides.id) LIKE ?{parameter_index} ESCAPE '\\'
               OR lower(COALESCE(rides.associated_vehicle, '')) LIKE ?{parameter_index} ESCAPE '\\'
               OR lower(COALESCE(rides.candidate_vehicle, '')) LIKE ?{parameter_index} ESCAPE '\\'
               OR lower(COALESCE(associated_device.display_name, '')) LIKE ?{parameter_index} ESCAPE '\\'
               OR lower(COALESCE(candidate_device.display_name, '')) LIKE ?{parameter_index} ESCAPE '\\'
               OR CAST(rides.created_at_ms AS TEXT) LIKE ?{parameter_index} ESCAPE '\\'
               OR strftime('%Y-%m-%d', rides.created_at_ms / 1000, 'unixepoch') LIKE ?{parameter_index} ESCAPE '\\')"
        ));
        parameters.push(Value::Text(format!("%{escaped}%")));
    }
    match query.distance {
        RideHistoryDistanceFilter::AllRides => {}
        RideHistoryDistanceFilter::OverFiveHundredFeet => {
            let parameter_index = first_parameter_index + parameters.len();
            predicates.push(format!(" AND rides.distance_mm > ?{parameter_index}"));
            parameters.push(Value::Integer(152_400));
        }
    }
    (predicates.concat(), parameters)
}

fn newest_recoverable_ride(connection: &Connection) -> Result<Option<RideRecord>, StorageError> {
    connection
        .query_row(
            "SELECT id, source, state, created_at_ms, monotonic_created_at_ms,
                    monotonic_last_event_ms, paused_at_ms, paused_duration_ms,
                    completed_duration_ms, updated_at_ms,
                    CASE
                        WHEN state IN ('stopped', 'interrupted', 'saved', 'discarded')
                            THEN completed_duration_ms
                        WHEN monotonic_created_at_ms IS NOT NULL
                            AND monotonic_last_event_ms IS NOT NULL
                            THEN MAX(0, monotonic_last_event_ms - monotonic_created_at_ms
                                - paused_duration_ms
                                - CASE WHEN state = 'paused' AND paused_at_ms IS NOT NULL
                                    THEN monotonic_last_event_ms - paused_at_ms ELSE 0 END)
                        WHEN (SELECT MAX(monotonic_ms) FROM ride_points WHERE ride_id = rides.id)
                                 IS NULL
                            OR (monotonic_created_at_ms IS NOT NULL
                                AND (SELECT MAX(monotonic_ms) FROM ride_points WHERE ride_id = rides.id)
                                    < monotonic_created_at_ms)
                            THEN 0
                        WHEN monotonic_created_at_ms IS NOT NULL
                            THEN (SELECT MAX(monotonic_ms) FROM ride_points WHERE ride_id = rides.id)
                                 - monotonic_created_at_ms
                        ELSE (SELECT MAX(monotonic_ms) - MIN(monotonic_ms)
                              FROM ride_points WHERE ride_id = rides.id)
                    END,
                    point_count, distance_mm,
                    (SELECT COUNT(*) FROM ride_segments WHERE ride_id = rides.id AND point_count > 0),
                    candidate_vehicle, associated_vehicle, associated_at_ms, last_telemetry_at_ms,
                    (SELECT display_name FROM devices
                     WHERE platform_identifier = rides.candidate_vehicle),
                    (SELECT display_name FROM devices
                     WHERE platform_identifier = rides.associated_vehicle),
                    (SELECT COUNT(*) FROM ride_segments
                     WHERE ride_id = rides.id AND point_count > 0 AND start_reason = 'background_gap'),
                    rides.last_location_observed_monotonic_ms, rides.last_location_observed_wall_clock_ms,
                    rides.motion_parked, rides.motion_observed_monotonic_ms, rides.stationary_latitude_e7,
                    rides.stationary_longitude_e7, rides.stationary_horizontal_accuracy_mm, rides.last_stationary_motion_monotonic_ms,
                    rides.last_recorded_point_duration_ms,
                    rides.moving_time_revision, rides.moving_time_ms, rides.moving_time_observed_ms,
                    rides.moving_time_unknown_ms, rides.moving_time_cursor_ms,
                    rides.moving_time_phone_speed_mm_s, rides.moving_time_phone_source_ms,
                    rides.moving_time_vehicle_speed_mm_s, rides.moving_time_vehicle_source_ms,
                    rides.moving_time_phone_watermark_ms, rides.moving_time_vehicle_watermark_ms,
                    rides.moving_time_incomplete_history, rides.moving_time_availability,
                    rides.last_recorded_point_moving_time_ms,
                    rides.last_recorded_point_moving_observed_ms,
                    rides.last_recorded_point_moving_unknown_ms,
                    rides.last_recorded_point_moving_availability,
                    rides.elapsed_time_ms, rides.elapsed_time_anchor_ms,
                    rides.elapsed_time_running, rides.elapsed_time_partial,
                    rides.last_recorded_point_elapsed_time_ms,
                    rides.last_recorded_point_elapsed_partial
             FROM rides
             WHERE state IN ('active', 'paused', 'interrupted')
             ORDER BY created_at_ms DESC, id DESC
             LIMIT 1",
            [],
            ride_record_from_row,
        )
        .optional()
        .map_err(StorageError::from)
}

fn load_ride_restore_snapshot(
    connection: &mut Connection,
) -> Result<RideRestoreSnapshot, StorageError> {
    let transaction = connection.transaction()?;
    let last_connected_device = last_connected_device(&transaction)?;
    let ride_id = newest_recoverable_ride(&transaction)?.map(|ride| ride.id());
    let inactivity_evidence = ride_id
        .map(|ride_id| ride_inactivity::read(&transaction, ride_id))
        .transpose()?
        .flatten();
    if let Some(ride_id) = ride_id {
        let checkpoint = load_moving_time_checkpoint(&transaction, ride_id)?;
        let parts = checkpoint.continuation.state_parts();
        let moving_changed = parts.cursor_milliseconds.is_some()
            || parts.phone_gps.is_some()
            || parts.vehicle.is_some()
            || parts.latest_phone_gps_source_milliseconds.is_some()
            || parts.latest_vehicle_source_milliseconds.is_some();
        let elapsed_parts = checkpoint.elapsed_time.state_parts();
        let elapsed_changed = elapsed_parts.running || elapsed_parts.anchor_milliseconds.is_some();
        if moving_changed || elapsed_changed {
            let mut continuation = checkpoint.continuation;
            if moving_changed {
                let at = transaction.query_row(
                    "SELECT COALESCE(monotonic_last_event_ms, 0) FROM rides WHERE id = ?1",
                    [ride_id.uuid().to_string()],
                    |row| row.get::<_, u64>(0),
                )?;
                continuation = MovingTimeContinuation::restored_after_restart(
                    parts,
                    MonotonicMilliseconds::new(at),
                )
                .map_err(moving_time_storage_error)?;
            }
            let revision = checkpoint.revision.checked_add(1).ok_or_else(|| {
                StorageError::InvalidStoredValue {
                    field: "moving-time revision",
                    value: checkpoint.revision.to_string(),
                }
            })?;
            save_moving_time_checkpoint(
                &transaction,
                ride_id,
                revision,
                continuation,
                checkpoint.elapsed_time.restored_after_restart(),
                None,
                None,
            )?;
        }
    }
    let ride = match ride_id {
        Some(ride_id) => find_ride(&transaction, ride_id)?,
        None => None,
    };
    let Some(ride) = ride else {
        transaction.commit()?;
        return Ok(RideRestoreSnapshot {
            last_connected_device,
            ride: None,
            route_points: Vec::new(),
            music_history_policy: None,
            music_history_available: false,
            inactivity_evidence: None,
        });
    };

    let point_count = ride.summary().point_count().as_u64();
    let first_sequence = point_count.saturating_sub(MAX_LIVE_ROUTE_POINTS as u64);
    let mut cursor = first_sequence.checked_sub(1).map(RoutePointCursor::new);
    let limit = QueryLimit::new(MAX_QUERY_LIMIT)?;
    let mut restored_points = Vec::new();
    loop {
        let page = route_points(&transaction, ride.id(), cursor, limit)?;
        restored_points.extend(page.points);
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    let music_history_policy = music_history_policy(&transaction, ride.id()).ok();
    let music_history_available = music_history(&transaction, ride.id()).is_ok();
    transaction.commit()?;
    Ok(RideRestoreSnapshot {
        last_connected_device,
        ride: Some(ride),
        route_points: restored_points,
        music_history_policy,
        music_history_available,
        inactivity_evidence,
    })
}

#[allow(
    clippy::too_many_lines,
    reason = "bounded SQL projection and cursor handling"
)]
fn list_rides(
    connection: &Connection,
    cursor: Option<RideCursor>,
    limit: QueryLimit,
    query: &RideHistoryQuery,
) -> Result<RidePage, StorageError> {
    let fetch_limit = i64::from(limit.get()) + 1;
    let (filters_sql, mut parameters) = history_filter_predicates(query, 1);
    let base_sql = "SELECT rides.id, rides.source, rides.state, rides.created_at_ms,
                           rides.monotonic_created_at_ms, rides.monotonic_last_event_ms,
                           rides.paused_at_ms, rides.paused_duration_ms,
                           rides.completed_duration_ms, rides.updated_at_ms,
                           CASE
                               WHEN rides.state IN ('stopped', 'interrupted', 'saved', 'discarded')
                                   THEN rides.completed_duration_ms
                               WHEN rides.monotonic_created_at_ms IS NOT NULL
                                   AND rides.monotonic_last_event_ms IS NOT NULL
                                   THEN MAX(0, rides.monotonic_last_event_ms - rides.monotonic_created_at_ms
                                       - rides.paused_duration_ms
                                       - CASE WHEN rides.state = 'paused' AND rides.paused_at_ms IS NOT NULL
                                           THEN rides.monotonic_last_event_ms - rides.paused_at_ms ELSE 0 END)
                               WHEN (SELECT MAX(monotonic_ms) FROM ride_points WHERE ride_id = rides.id)
                                        IS NULL
                                   OR (rides.monotonic_created_at_ms IS NOT NULL
                                       AND (SELECT MAX(monotonic_ms) FROM ride_points WHERE ride_id = rides.id)
                                           < rides.monotonic_created_at_ms)
                                   THEN 0
                               WHEN rides.monotonic_created_at_ms IS NOT NULL
                                   THEN (SELECT MAX(monotonic_ms) FROM ride_points WHERE ride_id = rides.id)
                                        - rides.monotonic_created_at_ms
                               ELSE (SELECT MAX(monotonic_ms) - MIN(monotonic_ms)
                                     FROM ride_points WHERE ride_id = rides.id)
                           END,
                           rides.point_count, rides.distance_mm,
                           (SELECT COUNT(*) FROM ride_segments WHERE ride_id = rides.id AND point_count > 0),
                           rides.candidate_vehicle, rides.associated_vehicle, rides.associated_at_ms,
                           rides.last_telemetry_at_ms,
                           candidate_device.display_name, associated_device.display_name,
                           (SELECT COUNT(*) FROM ride_segments
                            WHERE ride_id = rides.id AND point_count > 0 AND start_reason = 'background_gap'),
                    rides.last_location_observed_monotonic_ms, rides.last_location_observed_wall_clock_ms,
                    rides.motion_parked, rides.motion_observed_monotonic_ms, rides.stationary_latitude_e7,
                    rides.stationary_longitude_e7, rides.stationary_horizontal_accuracy_mm, rides.last_stationary_motion_monotonic_ms,
                    rides.last_recorded_point_duration_ms,
                    rides.moving_time_revision, rides.moving_time_ms, rides.moving_time_observed_ms,
                    rides.moving_time_unknown_ms, rides.moving_time_cursor_ms,
                    rides.moving_time_phone_speed_mm_s, rides.moving_time_phone_source_ms,
                    rides.moving_time_vehicle_speed_mm_s, rides.moving_time_vehicle_source_ms,
                    rides.moving_time_phone_watermark_ms, rides.moving_time_vehicle_watermark_ms,
                    rides.moving_time_incomplete_history, rides.moving_time_availability,
                    rides.last_recorded_point_moving_time_ms,
                    rides.last_recorded_point_moving_observed_ms,
                    rides.last_recorded_point_moving_unknown_ms,
                    rides.last_recorded_point_moving_availability,
                    rides.elapsed_time_ms, rides.elapsed_time_anchor_ms,
                    rides.elapsed_time_running, rides.elapsed_time_partial,
                    rides.last_recorded_point_elapsed_time_ms,
                    rides.last_recorded_point_elapsed_partial
                    FROM rides
                    LEFT JOIN devices AS associated_device
                        ON associated_device.platform_identifier = rides.associated_vehicle
                    LEFT JOIN devices AS candidate_device
                        ON candidate_device.platform_identifier = rides.candidate_vehicle
                    WHERE rides.state NOT IN ('draft', 'discarded')";
    let base_sql = format!("{base_sql}{filters_sql}");
    let mut rides = Vec::new();
    if let Some(cursor) = cursor {
        let cursor_created_at_parameter = parameters.len() + 1;
        let cursor_ride_id_parameter = parameters.len() + 2;
        let limit_parameter = parameters.len() + 3;
        let sql = format!(
            "{base_sql}
                      AND (rides.created_at_ms < ?{cursor_created_at_parameter} OR
                           (rides.created_at_ms = ?{cursor_created_at_parameter} AND
                            rides.id < ?{cursor_ride_id_parameter}))
                    ORDER BY rides.created_at_ms DESC, rides.id DESC
                    LIMIT ?{limit_parameter}"
        );
        parameters.extend([
            Value::Integer(
                i64::try_from(cursor.created_at_ms).map_err(|_| rusqlite::Error::InvalidQuery)?,
            ),
            Value::Text(cursor.ride_id.uuid().to_string()),
            Value::Integer(fetch_limit),
        ]);
        let mut statement = connection.prepare(&sql)?;
        let rows =
            statement.query_map(params_from_iter(parameters.iter()), ride_record_from_row)?;
        for row in rows {
            rides.push(row?);
        }
    } else {
        let limit_parameter = parameters.len() + 1;
        let sql = format!(
            "{base_sql}
                    ORDER BY rides.created_at_ms DESC, rides.id DESC
                    LIMIT ?{limit_parameter}"
        );
        parameters.push(Value::Integer(fetch_limit));
        let mut statement = connection.prepare(&sql)?;
        let rows =
            statement.query_map(params_from_iter(parameters.iter()), ride_record_from_row)?;
        for row in rows {
            rides.push(row?);
        }
    }
    let has_more = rides.len() > limit.get() as usize;
    if has_more {
        rides.pop();
    }
    let next_cursor = has_more
        .then(|| rides.last())
        .flatten()
        .map(|last| RideCursor {
            created_at_ms: last.created_at_ms,
            ride_id: last.id,
        });
    Ok(RidePage { rides, next_cursor })
}

fn list_ride_history_vehicle_options(
    connection: &Connection,
) -> Result<Vec<RideHistoryVehicleOption>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT identities.platform_identifier, devices.display_name
         FROM (
             SELECT associated_vehicle AS platform_identifier
             FROM rides
             WHERE state NOT IN ('draft', 'discarded') AND associated_vehicle IS NOT NULL
             UNION
             SELECT candidate_vehicle AS platform_identifier
             FROM rides
             WHERE state NOT IN ('draft', 'discarded') AND candidate_vehicle IS NOT NULL
         ) AS identities
         LEFT JOIN devices ON devices.platform_identifier = identities.platform_identifier
         ORDER BY lower(COALESCE(devices.display_name, identities.platform_identifier)),
                  identities.platform_identifier",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(RideHistoryVehicleOption {
            platform_identifier: row.get(0)?,
            display_name: row.get(1)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StorageError::from)
}

fn read_location_observation_clocks(
    row: &rusqlite::Row<'_>,
    monotonic_column: usize,
    wall_clock_column: usize,
) -> rusqlite::Result<Option<LocationObservationClocks>> {
    let monotonic_ms: Option<u64> = row.get(monotonic_column)?;
    let wall_clock_ms: Option<u64> = row.get(wall_clock_column)?;
    match (monotonic_ms, wall_clock_ms) {
        (None, None) => Ok(None),
        (Some(monotonic_ms), Some(wall_clock_ms)) if wall_clock_ms > 0 => {
            Ok(Some(LocationObservationClocks {
                monotonic_ms,
                wall_clock_ms,
            }))
        }
        _ => Err(rusqlite::Error::FromSqlConversionFailure(
            monotonic_column,
            rusqlite::types::Type::Integer,
            Box::new(StorageError::InvalidStoredValue {
                field: "location observation clocks",
                value: format!("{monotonic_ms:?}/{wall_clock_ms:?}"),
            }),
        )),
    }
}

fn read_motion_observation(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<Option<RideMotionObservation>> {
    let parked: Option<bool> = row.get(23)?;
    let at: Option<u64> = row.get(24)?;
    match (parked, at) {
        (None, None) => Ok(None),
        (Some(parked), Some(observed_at_milliseconds)) => Ok(Some(RideMotionObservation {
            parked,
            observed_at_milliseconds,
            last_stationary_at_milliseconds: row.get(28)?,
        })),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

fn read_stationary_observation(
    row: &rusqlite::Row<'_>,
    latitude_column: usize,
    longitude_column: usize,
    accuracy_column: usize,
    monotonic_column: usize,
    wall_clock_column: usize,
) -> rusqlite::Result<Option<LocationSample>> {
    let latitude: Option<i32> = row.get(latitude_column)?;
    let longitude: Option<i32> = row.get(longitude_column)?;
    let accuracy: Option<u32> = row.get(accuracy_column)?;
    match (
        latitude,
        longitude,
        accuracy,
        read_location_observation_clocks(row, monotonic_column, wall_clock_column)?,
    ) {
        (None, None, None, _) => Ok(None),
        (Some(latitude), Some(longitude), accuracy, Some(clocks)) => {
            let coordinate = Coordinate::from_fixed_parts(latitude, longitude)
                .map_err(|_| rusqlite::Error::InvalidQuery)?;
            Ok(Some(LocationSample::new(
                coordinate,
                clocks.monotonic_ms,
                clocks.wall_clock_ms,
                accuracy,
                LocationSource::Live,
            )))
        }
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

fn ride_record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RideRecord> {
    let id_value: String = row.get(0)?;
    let id = Uuid::parse_str(&id_value)
        .map(RideId::from_uuid)
        .map_err(|_| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(StorageError::InvalidStoredValue {
                    field: "ride identifier",
                    value: id_value,
                }),
            )
        })?;
    let source_value: String = row.get(1)?;
    let source = ride_source_from_db(&source_value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let state_value: String = row.get(2)?;
    let state = state_from_db(&state_value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let moving_time_checkpoint = read_moving_time_checkpoint_from_row(row, 30, 47)?;
    let prefix_availability_value: String = row.get(46)?;
    let prefix_availability = moving_time_availability_from_db(&prefix_availability_value)
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                46,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    let last_recorded_point_moving_time = MovingTimeSummary {
        moving: cutout_ride_maps::RideDurationMilliseconds::new(row.get(43)?),
        observed: cutout_ride_maps::RideDurationMilliseconds::new(row.get(44)?),
        unknown: cutout_ride_maps::RideDurationMilliseconds::new(row.get(45)?),
        availability: prefix_availability,
    };
    let last_recorded_point_elapsed_time = read_elapsed_summary_from_row(row, 51, 52)?;
    Ok(RideRecord {
        id,
        source,
        state,
        created_at_ms: row.get(3)?,
        monotonic_created_at_ms: row.get(4)?,
        monotonic_last_event_ms: row.get(5)?,
        paused_at_ms: row.get(6)?,
        paused_duration_ms: row.get(7)?,
        completed_duration_ms: row.get(8)?,
        updated_at_ms: row.get(9)?,
        duration_ms: row.get(10)?,
        summary: RideSummary::from_stored(row.get::<_, u64>(11)?.into(), row.get(12)?),
        segment_count: row.get(13)?,
        background_gap_count: row.get(20)?,
        candidate_vehicle: row.get(14)?,
        associated_vehicle: row.get(15)?,
        associated_at_ms: row.get(16)?,
        last_telemetry_at_ms: row.get(17)?,
        last_location_observation: read_location_observation_clocks(row, 21, 22)?,
        candidate_vehicle_name: row.get(18)?,
        associated_vehicle_name: row.get(19)?,
        motion_observation: read_motion_observation(row)?,
        stationary_location_observation: read_stationary_observation(row, 25, 26, 27, 21, 22)?,
        last_recorded_point_duration_ms: row.get(29)?,
        moving_time_checkpoint,
        last_recorded_point_moving_time,
        last_recorded_point_elapsed_time,
    })
}

fn read_moving_time_checkpoint_from_row(
    row: &rusqlite::Row<'_>,
    base: usize,
    elapsed_base: usize,
) -> rusqlite::Result<MovingTimeCheckpoint> {
    let revision: u64 = row.get(base)?;
    let phone_speed: Option<i32> = row.get(base + 5)?;
    let phone_source_at: Option<u64> = row.get(base + 6)?;
    let vehicle_speed: Option<i32> = row.get(base + 7)?;
    let vehicle_source_at: Option<u64> = row.get(base + 8)?;
    let availability_value: String = row.get(base + 12)?;
    let availability = moving_time_availability_from_db(&availability_value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            base + 12,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })?;
    let phone_gps = match (phone_speed, phone_source_at) {
        (Some(speed_millimetres_per_second), Some(source_at_milliseconds)) => {
            Some(MovingTimeSourceStateParts {
                speed_millimetres_per_second,
                source_at_milliseconds,
            })
        }
        (None, None) => None,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    let vehicle = match (vehicle_speed, vehicle_source_at) {
        (Some(speed_millimetres_per_second), Some(source_at_milliseconds)) => {
            Some(MovingTimeSourceStateParts {
                speed_millimetres_per_second,
                source_at_milliseconds,
            })
        }
        (None, None) => None,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    let continuation = MovingTimeContinuation::from_state_parts(MovingTimeStateParts {
        moving_milliseconds: row.get(base + 1)?,
        observed_milliseconds: row.get(base + 2)?,
        unknown_milliseconds: row.get(base + 3)?,
        cursor_milliseconds: row.get(base + 4)?,
        phone_gps,
        vehicle,
        latest_phone_gps_source_milliseconds: row.get(base + 9)?,
        latest_vehicle_source_milliseconds: row.get(base + 10)?,
        incomplete_history: row.get(base + 11)?,
        availability,
    })
    .map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            base,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })?;
    let elapsed_time = read_elapsed_continuation_from_row(row, elapsed_base)?;
    Ok(MovingTimeCheckpoint {
        revision,
        continuation,
        elapsed_time,
    })
}

fn read_elapsed_continuation_from_row(
    row: &rusqlite::Row<'_>,
    base: usize,
) -> rusqlite::Result<ElapsedTimeContinuation> {
    ElapsedTimeContinuation::from_state_parts(ElapsedTimeStateParts {
        duration_milliseconds: row.get(base)?,
        anchor_milliseconds: row.get(base + 1)?,
        running: row.get(base + 2)?,
        partial: row.get(base + 3)?,
    })
    .map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            base,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })
}

fn read_elapsed_summary_from_row(
    row: &rusqlite::Row<'_>,
    duration_index: usize,
    partial_index: usize,
) -> rusqlite::Result<ElapsedTimeSummary> {
    let duration: Option<u64> = row.get(duration_index)?;
    let partial: bool = row.get(partial_index)?;
    match (duration, partial) {
        (None, false) => Ok(ElapsedTimeSummary::Unavailable),
        (Some(milliseconds), false) => Ok(ElapsedTimeSummary::Complete {
            duration: ElapsedTimeMilliseconds::new(milliseconds),
        }),
        (Some(milliseconds), true) => Ok(ElapsedTimeSummary::Partial {
            duration: ElapsedTimeMilliseconds::new(milliseconds),
        }),
        (None, true) => Err(rusqlite::Error::InvalidQuery),
    }
}

fn route_points(
    connection: &Connection,
    ride_id: RideId,
    cursor: Option<RoutePointCursor>,
    limit: QueryLimit,
) -> Result<RoutePointPage, StorageError> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM rides WHERE id = ?1)",
        [ride_id.uuid().to_string()],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(StorageError::NotFound);
    }
    let after = cursor.map_or(-1_i64, |cursor| i64::try_from(cursor.0).unwrap_or(i64::MAX));
    let fetch_limit = i64::from(limit.get()) + 1;
    let mut statement = connection.prepare(
        "SELECT points.sequence, points.segment_id, points.telemetry_state, points.monotonic_ms,
                points.wall_clock_ms, points.latitude_e7, points.longitude_e7,
                points.horizontal_accuracy_mm, points.source, segments.start_reason
         FROM ride_points AS points
         JOIN ride_segments AS segments
           ON segments.ride_id = points.ride_id AND segments.segment_id = points.segment_id
         WHERE points.ride_id = ?1 AND points.sequence > ?2
         ORDER BY points.sequence ASC
         LIMIT ?3",
    )?;
    let rows = statement.query_map(
        params![ride_id.uuid().to_string(), after, fetch_limit],
        route_point_from_row,
    )?;
    let mut points = Vec::new();
    for row in rows {
        points.push(row?);
    }
    let has_more = points.len() > limit.get() as usize;
    if has_more {
        points.pop();
    }
    let next_cursor = has_more
        .then(|| points.last())
        .flatten()
        .map(|point| RoutePointCursor(point.sequence));
    Ok(RoutePointPage {
        points,
        next_cursor,
    })
}

fn project_history_context(
    connection: &Connection,
    query: &RideHistoryQuery,
    selected_ride: Option<RideId>,
    budget: HistoryContextBudget,
    viewport: Option<RouteViewport>,
    privacy: RoutePrivacyPolicy,
    cancellation: &RouteProjectionCancellation,
) -> Result<HistoryContextProjection, StorageError> {
    projection_checkpoint(Some(cancellation))?;
    let page = list_rides(connection, None, budget.history_page_limit, query)?;
    projection_checkpoint(Some(cancellation))?;

    let source_history_route_count = u64::try_from(page.rides.len()).unwrap_or(u64::MAX);
    let context_route_count = u64::try_from(
        page.rides
            .iter()
            .filter(|ride| Some(ride.id) != selected_ride)
            .count(),
    )
    .unwrap_or(u64::MAX);
    let mut routes = Vec::with_capacity(budget.max_routes);
    let mut total_display_point_count = 0usize;
    let mut routes_omitted_by_budget = false;

    for ride in &page.rides {
        if Some(ride.id) == selected_ride {
            continue;
        }
        if routes.len() == budget.max_routes {
            routes_omitted_by_budget = true;
            break;
        }
        let remaining = budget
            .total_point_budget
            .saturating_sub(total_display_point_count);
        if remaining == 0 {
            routes_omitted_by_budget = true;
            break;
        }
        projection_checkpoint(Some(cancellation))?;
        let route_budget = RouteDisplayBudget::new(
            budget.per_route_budget.as_usize().min(remaining),
        )
        .ok_or(StorageError::InvalidHistoryContextBudget {
            field: "aggregate point budget",
            value: u32::try_from(remaining).unwrap_or(u32::MAX),
        })?;
        let projection = project_route_points(
            connection,
            ride.id,
            viewport,
            route_budget,
            privacy,
            Some(cancellation),
        )?;
        routes_omitted_by_budget |= route_budget.as_usize() < budget.per_route_budget.as_usize()
            && projection.candidate_point_count()
                > u64::try_from(projection.points().len()).unwrap_or(u64::MAX);
        total_display_point_count =
            total_display_point_count.saturating_add(projection.points().len());
        routes.push(HistoryContextRoute {
            ride_id: ride.id,
            projection,
        });
    }

    Ok(HistoryContextProjection {
        routes,
        source_history_route_count,
        context_route_count,
        total_display_point_count: u64::try_from(total_display_point_count).unwrap_or(u64::MAX),
        routes_omitted_by_budget,
        history_page_has_more: page.next_cursor.is_some(),
    })
}

fn project_route_points(
    connection: &Connection,
    ride_id: RideId,
    viewport: Option<RouteViewport>,
    budget: RouteDisplayBudget,
    privacy: RoutePrivacyPolicy,
    cancellation: Option<&RouteProjectionCancellation>,
) -> Result<RoutePointProjection, StorageError> {
    projection_checkpoint(cancellation)?;
    let ride_id = ride_id.uuid().to_string();
    let Some(counts) = route_projection_counts(connection, &ride_id, viewport, cancellation)?
    else {
        return Err(StorageError::NotFound);
    };
    projection_checkpoint(cancellation)?;
    let candidate_count = usize::try_from(counts.candidate_point_count).unwrap_or(usize::MAX);
    let endpoint_metadata = route_endpoint_metadata_from_storage(
        connection,
        &ride_id,
        counts.source_start_sequence,
        counts.source_end_sequence,
        viewport,
        cancellation,
    )?;
    if candidate_count == 0 {
        return Ok(RoutePointProjection {
            points: Vec::new(),
            camera_region: None,
            canonical_camera_region: canonical_route_camera_region(
                connection,
                &ride_id,
                privacy,
                cancellation,
            )?,
            source_revision: counts.source_revision,
            source_point_count: counts.source_point_count,
            source_segment_count: counts.source_segment_count,
            candidate_point_count: counts.candidate_point_count,
            candidate_segment_count: counts.candidate_segment_count,
            displayed_segment_count: 0,
            background_gap_count: counts.background_gap_count,
            endpoint_metadata,
            segments: Vec::new(),
        });
    }

    let projected = project_route_candidates(
        connection,
        &ride_id,
        &counts,
        viewport,
        budget,
        privacy,
        cancellation,
    )?;
    Ok(RoutePointProjection {
        points: projected.points,
        camera_region: projected.camera_region,
        canonical_camera_region: canonical_route_camera_region(
            connection,
            &ride_id,
            privacy,
            cancellation,
        )?,
        source_revision: counts.source_revision,
        source_point_count: counts.source_point_count,
        source_segment_count: counts.source_segment_count,
        candidate_point_count: counts.candidate_point_count,
        candidate_segment_count: counts.candidate_segment_count,
        displayed_segment_count: projected.displayed_segment_count,
        background_gap_count: counts.background_gap_count,
        endpoint_metadata,
        segments: projected.segments,
    })
}

fn canonical_route_camera_region(
    connection: &Connection,
    ride_id: &str,
    privacy: RoutePrivacyPolicy,
    cancellation: Option<&RouteProjectionCancellation>,
) -> Result<Option<RouteCameraRegion>, StorageError> {
    projection_checkpoint(cancellation)?;
    let mut statement = projection_sqlite(
        connection.prepare(
            "SELECT latitude_e7, longitude_e7
             FROM ride_points WHERE ride_id = ?1 ORDER BY sequence ASC",
        ),
        cancellation,
    )?;
    let mut rows = projection_sqlite(statement.query([ride_id]), cancellation)?;
    let mut bounds = RouteCameraBounds::new();
    while let Some(row) = projection_sqlite(rows.next(), cancellation)? {
        projection_checkpoint(cancellation)?;
        let coordinate =
            Coordinate::from_fixed_parts(row.get(0)?, row.get(1)?).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Integer,
                    Box::new(error),
                )
            })?;
        bounds.include_with_privacy(coordinate, privacy);
    }
    Ok(bounds.region())
}

fn project_route_candidates(
    connection: &Connection,
    ride_id: &str,
    counts: &RouteProjectionCounts,
    viewport: Option<RouteViewport>,
    budget: RouteDisplayBudget,
    privacy: RoutePrivacyPolicy,
    cancellation: Option<&RouteProjectionCancellation>,
) -> Result<ProjectedRouteCandidates, StorageError> {
    let segment_metadata = route_segment_projection_metadata(connection, ride_id, cancellation)?;
    let select = format!(
        "SELECT points.sequence, points.segment_id, points.telemetry_state, points.monotonic_ms,
                points.wall_clock_ms, points.latitude_e7, points.longitude_e7,
                points.horizontal_accuracy_mm, points.source
         FROM ride_points AS points
         WHERE points.ride_id = ?1{}
         ORDER BY points.sequence ASC",
        counts.viewport_predicate
    );
    let mut statement = projection_sqlite(connection.prepare(&select), cancellation)?;
    projection_checkpoint(cancellation)?;
    let rows: Box<dyn Iterator<Item = rusqlite::Result<(RoutePoint, u64)>>> =
        if let Some(viewport) = viewport {
            Box::new(projection_sqlite(
                statement.query_map(
                    params![
                        ride_id,
                        viewport.minimum_latitude().as_i32(),
                        viewport.maximum_latitude().as_i32(),
                        viewport.minimum_longitude().as_i32(),
                        viewport.maximum_longitude().as_i32(),
                    ],
                    |row| projected_route_point_from_row(row, &segment_metadata),
                ),
                cancellation,
            )?)
        } else {
            Box::new(projection_sqlite(
                statement.query_map([ride_id], |row| {
                    projected_route_point_from_row(row, &segment_metadata)
                }),
                cancellation,
            )?)
        };
    let candidate_count = usize::try_from(counts.candidate_point_count).unwrap_or(usize::MAX);
    let mut accumulator = RouteProjectionAccumulator::new(candidate_count, budget, privacy);
    for (candidate_ordinal, row) in rows.enumerate() {
        projection_checkpoint(cancellation)?;
        let (point, canonical_point_count) = projection_sqlite(row, cancellation)?;
        accumulator.push_with_canonical_point_count(
            candidate_ordinal,
            point.sequence(),
            RideMapPoint::new_with_start_reason(
                point.sample(),
                RideMapSegmentId::new(point.segment_id()),
                point.telemetry_state(),
                point.start_reason(),
            ),
            Some(RidePointCount::new(canonical_point_count)),
        );
        if accumulator.is_complete() {
            break;
        }
    }
    projection_checkpoint(cancellation)?;
    let points = accumulator.finish();
    let camera_region = route_camera_region(points.iter().map(|point| point.coordinate()));
    let segments = route_segment_display_metadata(points.iter().copied());
    let displayed_segment_count = u64::try_from(count_segment_runs(
        points.iter().copied().map(RouteDisplayPoint::segment_id),
    ))
    .unwrap_or(u64::MAX);
    Ok(ProjectedRouteCandidates {
        points,
        camera_region,
        displayed_segment_count,
        segments,
    })
}

fn route_segment_projection_metadata(
    connection: &Connection,
    ride_id: &str,
    cancellation: Option<&RouteProjectionCancellation>,
) -> Result<HashMap<u64, RouteSegmentProjectionMetadata>, StorageError> {
    let mut statement = projection_sqlite(
        connection.prepare(
            "SELECT segment_id, start_reason, point_count
             FROM ride_segments
             WHERE ride_id = ?1 AND point_count > 0",
        ),
        cancellation,
    )?;
    let rows = projection_sqlite(
        statement.query_map([ride_id], |row| {
            let start_reason =
                segment_start_reason_from_db(row.get_ref(1)?.as_str()?).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
            Ok((
                row.get::<_, u64>(0)?,
                RouteSegmentProjectionMetadata {
                    start_reason,
                    point_count: row.get(2)?,
                },
            ))
        }),
        cancellation,
    )?;
    let mut metadata = HashMap::new();
    for row in rows {
        projection_checkpoint(cancellation)?;
        let (segment_id, segment) = projection_sqlite(row, cancellation)?;
        metadata.insert(segment_id, segment);
    }
    Ok(metadata)
}

fn route_endpoint_metadata_from_storage(
    connection: &Connection,
    ride_id: &str,
    source_start_sequence: Option<u64>,
    source_end_sequence: Option<u64>,
    viewport: Option<RouteViewport>,
    cancellation: Option<&RouteProjectionCancellation>,
) -> Result<RouteEndpointMetadata, StorageError> {
    Ok(RouteEndpointMetadata::new(
        source_start_sequence.map(RidePointSequence::new),
        source_end_sequence.map(RidePointSequence::new),
        endpoint_is_visible(
            connection,
            ride_id,
            source_start_sequence,
            viewport,
            cancellation,
        )?,
        endpoint_is_visible(
            connection,
            ride_id,
            source_end_sequence,
            viewport,
            cancellation,
        )?,
    ))
}

struct RouteProjectionCounts {
    source_revision: u64,
    source_point_count: u64,
    source_segment_count: u64,
    background_gap_count: u64,
    candidate_point_count: u64,
    candidate_segment_count: u64,
    viewport_predicate: String,
    source_start_sequence: Option<u64>,
    source_end_sequence: Option<u64>,
}

fn route_projection_counts(
    connection: &Connection,
    ride_id: &str,
    viewport: Option<RouteViewport>,
    cancellation: Option<&RouteProjectionCancellation>,
) -> Result<Option<RouteProjectionCounts>, StorageError> {
    let Some((source_revision, source_point_count)) = projection_sqlite(
        connection
            .query_row(
                // Canonical route points are append-only, so point_count is a monotonic route
                // revision. updated_at_ms is wall-clock metadata and may repeat.
                "SELECT point_count, point_count FROM rides WHERE id = ?1",
                [ride_id],
                |row| Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?)),
            )
            .optional(),
        cancellation,
    )?
    else {
        return Ok(None);
    };
    projection_checkpoint(cancellation)?;
    let (source_segment_count, background_gap_count) = projection_sqlite(
        connection.query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(CASE WHEN start_reason = 'background_gap' THEN 1 ELSE 0 END), 0)
             FROM ride_segments
             WHERE ride_id = ?1 AND point_count > 0",
            [ride_id],
            |row| Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?)),
        ),
        cancellation,
    )?;
    projection_checkpoint(cancellation)?;
    let (source_start_sequence, source_end_sequence) = projection_sqlite(
        connection.query_row(
            "SELECT MIN(sequence), MAX(sequence) FROM ride_points WHERE ride_id = ?1",
            [ride_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ),
        cancellation,
    )?;
    projection_checkpoint(cancellation)?;
    let viewport_predicate = route_point_viewport_predicate(viewport);
    let (candidate_point_count, candidate_segment_count) = if let Some(viewport) = viewport {
        let parameters = params![
            ride_id,
            viewport.minimum_latitude().as_i32(),
            viewport.maximum_latitude().as_i32(),
            viewport.minimum_longitude().as_i32(),
            viewport.maximum_longitude().as_i32(),
        ];
        let candidate_point_count = projection_sqlite(
            connection.query_row(
                &format!("SELECT COUNT(*) FROM ride_points WHERE ride_id = ?1{viewport_predicate}"),
                parameters,
                |row| row.get::<_, u64>(0),
            ),
            cancellation,
        )?;
        projection_checkpoint(cancellation)?;
        let candidate_segment_count = projection_sqlite(
            connection.query_row(
                &format!(
                    "SELECT COUNT(DISTINCT segment_id) FROM ride_points WHERE ride_id = ?1{viewport_predicate}"
                ),
                params![
                    ride_id,
                    viewport.minimum_latitude().as_i32(),
                    viewport.maximum_latitude().as_i32(),
                    viewport.minimum_longitude().as_i32(),
                    viewport.maximum_longitude().as_i32(),
                ],
                |row| row.get::<_, u64>(0),
            ),
            cancellation,
        )?;
        projection_checkpoint(cancellation)?;
        (candidate_point_count, candidate_segment_count)
    } else {
        (source_point_count, source_segment_count)
    };
    Ok(Some(RouteProjectionCounts {
        source_revision,
        source_point_count,
        source_segment_count,
        background_gap_count,
        candidate_point_count,
        candidate_segment_count,
        viewport_predicate,
        source_start_sequence,
        source_end_sequence,
    }))
}

fn endpoint_is_visible(
    connection: &Connection,
    ride_id: &str,
    sequence: Option<u64>,
    viewport: Option<RouteViewport>,
    cancellation: Option<&RouteProjectionCancellation>,
) -> Result<bool, StorageError> {
    let Some(sequence) = sequence else {
        return Ok(false);
    };
    let sequence = i64::try_from(sequence).unwrap_or(i64::MAX);
    let Some(viewport) = viewport else {
        return Ok(true);
    };
    let visible = projection_sqlite(
        if viewport.crosses_antimeridian() {
            connection.query_row(
                "SELECT EXISTS(
                SELECT 1 FROM ride_points
                WHERE ride_id = ?1 AND sequence = ?2
                  AND latitude_e7 BETWEEN ?3 AND ?4
                  AND (longitude_e7 >= ?5 OR longitude_e7 <= ?6)
            )",
                params![
                    ride_id,
                    sequence,
                    viewport.minimum_latitude().as_i32(),
                    viewport.maximum_latitude().as_i32(),
                    viewport.minimum_longitude().as_i32(),
                    viewport.maximum_longitude().as_i32(),
                ],
                |row| row.get(0),
            )
        } else {
            connection.query_row(
                "SELECT EXISTS(
                SELECT 1 FROM ride_points
                WHERE ride_id = ?1 AND sequence = ?2
                  AND latitude_e7 BETWEEN ?3 AND ?4
                  AND longitude_e7 BETWEEN ?5 AND ?6
            )",
                params![
                    ride_id,
                    sequence,
                    viewport.minimum_latitude().as_i32(),
                    viewport.maximum_latitude().as_i32(),
                    viewport.minimum_longitude().as_i32(),
                    viewport.maximum_longitude().as_i32(),
                ],
                |row| row.get(0),
            )
        },
        cancellation,
    )?;
    projection_checkpoint(cancellation)?;
    Ok(visible)
}

fn route_point_viewport_predicate(viewport: Option<RouteViewport>) -> String {
    let Some(viewport) = viewport else {
        return String::new();
    };
    if viewport.crosses_antimeridian() {
        " AND latitude_e7 BETWEEN ?2 AND ?3 AND (longitude_e7 >= ?4 OR longitude_e7 <= ?5)"
            .to_owned()
    } else {
        " AND latitude_e7 BETWEEN ?2 AND ?3 AND longitude_e7 BETWEEN ?4 AND ?5".to_owned()
    }
}

fn route_point_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RoutePoint> {
    let start_reason =
        segment_start_reason_from_db(row.get_ref(9)?.as_str()?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                9,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    route_point_from_row_with_start_reason(row, start_reason)
}

fn route_point_from_row_with_start_reason(
    row: &rusqlite::Row<'_>,
    start_reason: RideSegmentStartReason,
) -> rusqlite::Result<RoutePoint> {
    let source = source_from_db(row.get_ref(8)?.as_str()?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let coordinate = Coordinate::from_fixed_parts(row.get(5)?, row.get(6)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })?;
    Ok(RoutePoint {
        sequence: row.get(0)?,
        segment_id: row.get(1)?,
        start_reason,
        telemetry_state: RouteTelemetryState::from_storage(row.get(2)?).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Integer,
                Box::new(StorageError::InvalidStoredValue {
                    field: "telemetry state",
                    value: "unknown".to_owned(),
                }),
            )
        })?,
        sample: LocationSample::new(
            coordinate,
            row.get::<_, u64>(3)?,
            row.get::<_, u64>(4)?,
            row.get(7)?,
            source,
        ),
    })
}

fn projected_route_point_from_row(
    row: &rusqlite::Row<'_>,
    segment_metadata: &HashMap<u64, RouteSegmentProjectionMetadata>,
) -> rusqlite::Result<(RoutePoint, u64)> {
    let segment_id = row.get::<_, u64>(1)?;
    let segment = segment_metadata.get(&segment_id).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Integer,
            Box::new(StorageError::InvalidStoredValue {
                field: "ride point segment",
                value: segment_id.to_string(),
            }),
        )
    })?;
    Ok((
        route_point_from_row_with_start_reason(row, segment.start_reason)?,
        segment.point_count,
    ))
}

fn segment_start_reason_from_db(value: &str) -> Result<RideSegmentStartReason, StorageError> {
    match value {
        "initial" => Ok(RideSegmentStartReason::Initial),
        "resume" => Ok(RideSegmentStartReason::Resume),
        "background_gap" => Ok(RideSegmentStartReason::BackgroundGap),
        "import_boundary" => Ok(RideSegmentStartReason::ImportBoundary),
        "stationary" => Ok(RideSegmentStartReason::Stationary),
        other => Err(StorageError::InvalidStoredValue {
            field: "ride segment start reason",
            value: other.to_owned(),
        }),
    }
}

fn ride_source_from_db(value: &str) -> Result<RideSource, StorageError> {
    match value {
        "live" => Ok(RideSource::Live),
        "pevcap_import" => Ok(RideSource::PevcapImport),
        other => Err(StorageError::InvalidStoredValue {
            field: "ride source",
            value: other.to_owned(),
        }),
    }
}

fn state_to_db(state: RideLifecycleState) -> &'static str {
    match state {
        RideLifecycleState::Draft => "draft",
        RideLifecycleState::Active => "active",
        RideLifecycleState::Paused => "paused",
        RideLifecycleState::Stopped => "stopped",
        RideLifecycleState::Interrupted => "interrupted",
        RideLifecycleState::Discarded => "discarded",
        RideLifecycleState::Saved => "saved",
        RideLifecycleState::Imported => "imported",
    }
}

fn state_from_db(value: &str) -> Result<RideLifecycleState, StorageError> {
    match value {
        "draft" => Ok(RideLifecycleState::Draft),
        "active" => Ok(RideLifecycleState::Active),
        "paused" => Ok(RideLifecycleState::Paused),
        "stopped" => Ok(RideLifecycleState::Stopped),
        "interrupted" => Ok(RideLifecycleState::Interrupted),
        "discarded" => Ok(RideLifecycleState::Discarded),
        "saved" => Ok(RideLifecycleState::Saved),
        "imported" => Ok(RideLifecycleState::Imported),
        other => Err(StorageError::InvalidStoredValue {
            field: "ride state",
            value: other.to_owned(),
        }),
    }
}

fn source_to_db(source: LocationSource) -> &'static str {
    match source {
        LocationSource::Live => "live",
        LocationSource::PevcapImport => "pevcap_import",
    }
}

fn source_from_db(value: &str) -> Result<LocationSource, StorageError> {
    match value {
        "live" => Ok(LocationSource::Live),
        "pevcap_import" => Ok(LocationSource::PevcapImport),
        other => Err(StorageError::InvalidStoredValue {
            field: "location source",
            value: other.to_owned(),
        }),
    }
}

#[cfg(test)]
mod location_observation_checkpoint_tests {
    use super::*;

    fn sample(at: u64, latitude: f64, accuracy: u32) -> LocationSample {
        LocationSample::new(
            Coordinate::from_degrees(latitude, -105.0).unwrap(),
            at,
            1_700_000_000_000 + at,
            Some(accuracy),
            LocationSource::Live,
        )
    }

    fn fixture() -> (Connection, RideId) {
        let mut connection = Connection::open_in_memory().unwrap();
        migrate(&mut connection).unwrap();
        let id = create_started_live_ride(&mut connection, 1_700_000_000_000, 1_000, None, None)
            .unwrap()
            .ride_id();
        assert_eq!(
            append_location(
                &mut connection,
                id,
                sample(1_000, 40.0, 3_000),
                RideMapSegmentId::new(0),
                RouteTelemetryState::GpsOnly
            )
            .unwrap(),
            LocationAdmission::Accepted
        );
        (connection, id)
    }

    #[test]
    fn location_observation_checkpoint_preserves_point_and_advances_exact_clock_pair() {
        let (mut connection, ride_id) = fixture();
        let before =
            route_points(&connection, ride_id, None, QueryLimit::new(10).unwrap()).unwrap();
        let checkpoint = RideLocationObservationCheckpoint {
            ride_id,
            expected_point_sequence: 0,
            observation: sample(60_000, 40.0, 3_000),
        };
        let changes = connection.total_changes();
        assert_eq!(
            checkpoint_location_observation(&connection, checkpoint).unwrap(),
            RideLocationObservationCheckpointOutcome::Applied
        );
        assert_eq!(connection.total_changes(), changes + 1);
        let ride = find_ride(&connection, ride_id).unwrap().unwrap();
        assert_eq!(
            ride.last_location_observed_monotonic_milliseconds(),
            Some(60_000)
        );
        assert_eq!(
            ride.last_location_observed_wall_clock_milliseconds(),
            Some(1_700_000_060_000)
        );
        assert_eq!(ride.monotonic_last_event_milliseconds(), Some(60_000));
        assert_eq!(ride.updated_at_milliseconds(), 1_700_000_060_000);
        assert_eq!(
            route_points(&connection, ride_id, None, QueryLimit::new(10).unwrap())
                .unwrap()
                .points,
            before.points
        );
        assert_eq!(
            checkpoint_location_observation(&connection, checkpoint).unwrap(),
            RideLocationObservationCheckpointOutcome::AlreadyObserved
        );
        assert_eq!(connection.total_changes(), changes + 1);
        assert_eq!(
            append_location(
                &mut connection,
                ride_id,
                sample(61_000, 40.00001, 3_000),
                RideMapSegmentId::new(0),
                RouteTelemetryState::GpsOnly
            )
            .unwrap(),
            LocationAdmission::Accepted
        );
        let points = route_points(&connection, ride_id, None, QueryLimit::new(10).unwrap())
            .unwrap()
            .points;
        assert_eq!(points.len(), 2);
        assert_eq!(points[1].start_reason(), RideSegmentStartReason::Initial);
        assert_eq!(
            find_ride(&connection, ride_id)
                .unwrap()
                .unwrap()
                .last_location_observed_monotonic_milliseconds(),
            Some(61_000)
        );
    }

    #[test]
    fn location_observation_checkpoint_rejects_corrupt_partial_clock_pair() {
        let (mut connection, ride_id) = fixture();
        for (monotonic_ms, wall_clock_ms) in [
            (Some(60_000), None),
            (None, Some(1_700_000_060_000_u64)),
            (Some(60_000), Some(0)),
        ] {
            connection.execute(
                "UPDATE rides SET last_location_observed_monotonic_ms = ?2, last_location_observed_wall_clock_ms = ?3 WHERE id = ?1",
                params![ride_id.uuid().to_string(), monotonic_ms, wall_clock_ms],
            ).unwrap();
            assert!(find_ride(&connection, ride_id).is_err());
            assert!(load_previous_ride_point(&connection, ride_id).is_err());
            assert!(
                append_location(
                    &mut connection,
                    ride_id,
                    sample(61_000, 40.00001, 3_000),
                    RideMapSegmentId::new(0),
                    RouteTelemetryState::GpsOnly
                )
                .is_err()
            );
        }
    }

    #[test]
    fn location_observation_checkpoint_preserves_backwards_wall_clock() {
        let (connection, ride_id) = fixture();
        let observation = LocationSample::new(
            Coordinate::from_degrees(40.0, -105.0).unwrap(),
            60_000,
            1_699_999_000_000,
            Some(3_000),
            LocationSource::Live,
        );
        let checkpoint = RideLocationObservationCheckpoint {
            ride_id,
            expected_point_sequence: 0,
            observation,
        };
        assert_eq!(
            checkpoint_location_observation(&connection, checkpoint).unwrap(),
            RideLocationObservationCheckpointOutcome::Applied
        );
        assert_eq!(
            checkpoint_location_observation(&connection, checkpoint).unwrap(),
            RideLocationObservationCheckpointOutcome::AlreadyObserved
        );
        let ride = find_ride(&connection, ride_id).unwrap().unwrap();
        assert_eq!(
            ride.last_location_observed_monotonic_milliseconds(),
            Some(60_000)
        );
        assert_eq!(
            ride.last_location_observed_wall_clock_milliseconds(),
            Some(1_699_999_000_000)
        );
        assert_eq!(ride.updated_at_milliseconds(), 1_700_000_001_000);
        assert_eq!(
            load_previous_ride_point(&connection, ride_id)
                .unwrap()
                .unwrap()
                .sample,
            observation
        );
        assert_eq!(
            checkpoint_location_observation(
                &connection,
                RideLocationObservationCheckpoint {
                    observation: sample(60_000, 40.0, 3_000),
                    ..checkpoint
                }
            )
            .unwrap(),
            RideLocationObservationCheckpointOutcome::PointChanged
        );
    }

    #[test]
    fn location_observation_checkpoint_revalidates_after_new_material_point() {
        let (mut connection, ride_id) = fixture();
        let prepared = RideLocationObservationCheckpoint {
            ride_id,
            expected_point_sequence: 0,
            observation: sample(3_000, 40.0, 3_000),
        };
        assert_eq!(
            append_location(
                &mut connection,
                ride_id,
                sample(2_000, 40.00001, 3_000),
                RideMapSegmentId::new(0),
                RouteTelemetryState::GpsOnly
            )
            .unwrap(),
            LocationAdmission::Accepted
        );
        let changes = connection.total_changes();
        assert_eq!(
            checkpoint_location_observation(&connection, prepared).unwrap(),
            RideLocationObservationCheckpointOutcome::PointChanged
        );
        assert_eq!(connection.total_changes(), changes);
        assert_eq!(
            find_ride(&connection, ride_id)
                .unwrap()
                .unwrap()
                .last_location_observed_monotonic_milliseconds(),
            Some(2_000)
        );
    }

    #[test]
    fn location_append_constraint_failure_rolls_back_every_write_and_retries() {
        fn snapshot(connection: &Connection, table: &str) -> Vec<Vec<Value>> {
            let mut statement = connection
                .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
                .unwrap();
            let column_count = statement.column_count();
            statement
                .query_map([], |row| {
                    (0..column_count).map(|index| row.get(index)).collect()
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        }
        for table in ["rides", "ride_segments"] {
            let (mut connection, ride_id) = fixture();
            let before =
                ["rides", "ride_segments", "ride_points"].map(|table| snapshot(&connection, table));
            connection.execute_batch(&format!(
                "CREATE TRIGGER fail_location_update BEFORE UPDATE ON {table} BEGIN SELECT RAISE(ABORT, 'test location summary failure'); END;"
            )).unwrap();
            let retry_sample = sample(40_000, 40.00001, 3_000);
            let result = append_location(
                &mut connection,
                ride_id,
                retry_sample,
                RideMapSegmentId::new(1),
                RouteTelemetryState::GpsOnly,
            );
            let Err(StorageError::Sqlite(error)) = result else {
                panic!("{table} failure must remain a storage error, got {result:?}");
            };
            assert_eq!(
                error.sqlite_error_code(),
                Some(ErrorCode::ConstraintViolation)
            );
            for (table, expected) in ["rides", "ride_segments", "ride_points"]
                .into_iter()
                .zip(before)
            {
                assert_eq!(
                    snapshot(&connection, table),
                    expected,
                    "{table} must roll back"
                );
            }
            connection
                .execute_batch("DROP TRIGGER fail_location_update;")
                .unwrap();
            assert_eq!(
                append_location(
                    &mut connection,
                    ride_id,
                    retry_sample,
                    RideMapSegmentId::new(1),
                    RouteTelemetryState::GpsOnly
                )
                .unwrap(),
                LocationAdmission::Accepted
            );
            assert_eq!(
                append_location(
                    &mut connection,
                    ride_id,
                    retry_sample,
                    RideMapSegmentId::new(1),
                    RouteTelemetryState::GpsOnly
                )
                .unwrap(),
                LocationAdmission::Duplicate
            );
            let ride = find_ride(&connection, ride_id).unwrap().unwrap();
            assert_eq!(ride.summary().point_count().as_u64(), 2);
            assert_eq!(ride.background_gap_count(), 1);
            assert_eq!(ride.summary().distance().as_u64(), 0);
        }
    }

    #[test]
    fn location_observation_checkpoint_rejects_stale_identity_and_retries_sql_failure() {
        let (connection, ride_id) = fixture();
        let valid = RideLocationObservationCheckpoint {
            ride_id,
            expected_point_sequence: 0,
            observation: sample(60_000, 40.0, 3_000),
        };
        let changes = connection.total_changes();
        for checkpoint in [
            RideLocationObservationCheckpoint {
                expected_point_sequence: 1,
                ..valid
            },
            RideLocationObservationCheckpoint {
                observation: sample(60_000, 40.00001, 3_000),
                ..valid
            },
            RideLocationObservationCheckpoint {
                observation: sample(60_000, 40.0, 4_000),
                ..valid
            },
            RideLocationObservationCheckpoint {
                observation: sample(999, 40.0, 3_000),
                ..valid
            },
        ] {
            assert_eq!(
                checkpoint_location_observation(&connection, checkpoint).unwrap(),
                RideLocationObservationCheckpointOutcome::PointChanged
            );
        }
        assert_eq!(connection.total_changes(), changes);
        connection.execute_batch("CREATE TRIGGER deny_gps_clock BEFORE UPDATE OF last_location_observed_monotonic_ms ON rides BEGIN SELECT RAISE(ABORT, 'test GPS checkpoint failure'); END;").unwrap();
        assert!(checkpoint_location_observation(&connection, valid).is_err());
        assert_eq!(
            find_ride(&connection, ride_id)
                .unwrap()
                .unwrap()
                .monotonic_last_event_milliseconds(),
            Some(1_000)
        );
        connection
            .execute_batch("DROP TRIGGER deny_gps_clock;")
            .unwrap();
        assert_eq!(
            checkpoint_location_observation(&connection, valid).unwrap(),
            RideLocationObservationCheckpointOutcome::Applied
        );
        transition_ride(
            &connection,
            ride_id,
            RideEvent::Pause,
            1_700_000_061_000,
            Some(61_000),
        )
        .unwrap();
        assert_eq!(
            checkpoint_location_observation(
                &connection,
                RideLocationObservationCheckpoint {
                    observation: sample(62_000, 40.0, 3_000),
                    ..valid
                }
            )
            .unwrap(),
            RideLocationObservationCheckpointOutcome::PointChanged
        );
        let ride = find_ride(&connection, ride_id).unwrap().unwrap();
        assert_eq!(
            ride.last_location_observed_monotonic_milliseconds(),
            Some(60_000)
        );
        assert_eq!(
            ride.last_location_observed_wall_clock_milliseconds(),
            Some(1_700_000_060_000)
        );
    }
}

#[cfg(test)]
mod ride_session_marker_idempotence_tests {
    use super::{RideSessionMarkerKey, save_ride_session_marker};
    use rusqlite::Connection;

    #[test]
    fn unchanged_marker_does_not_write_and_changed_marker_does() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute(
                "CREATE TABLE ride_session_marker (
                    singleton_key BLOB PRIMARY KEY,
                    marker BLOB NOT NULL
                )",
                [],
            )
            .unwrap();

        save_ride_session_marker(&connection, &[1, 2, 3]).unwrap();
        let inserted_changes = connection.total_changes();

        save_ride_session_marker(&connection, &[1, 2, 3]).unwrap();

        assert_eq!(connection.total_changes(), inserted_changes);
        assert_eq!(
            connection
                .query_row(
                    "SELECT marker FROM ride_session_marker WHERE singleton_key = ?1",
                    [RideSessionMarkerKey::VALUE.blob()],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .unwrap(),
            vec![1, 2, 3]
        );

        save_ride_session_marker(&connection, &[4, 5, 6]).unwrap();

        assert_eq!(connection.total_changes(), inserted_changes + 1);
        assert_eq!(
            connection
                .query_row(
                    "SELECT marker FROM ride_session_marker WHERE singleton_key = ?1",
                    [RideSessionMarkerKey::VALUE.blob()],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .unwrap(),
            vec![4, 5, 6]
        );

        connection
            .execute("DROP TABLE ride_session_marker", [])
            .unwrap();
        assert!(save_ride_session_marker(&connection, &[7]).is_err());
    }
}

#[cfg(test)]
mod stationary_motion_tests;

#[cfg(test)]
mod elapsed_time_persistence_tests {
    use super::*;

    #[test]
    fn elapsed_checkpoint_hydration_rejects_inconsistent_state() {
        let connection = Connection::open_in_memory().unwrap();
        create_current_schema(&connection).unwrap();
        let ride_id = RideId::new();
        connection
            .execute(
                "INSERT INTO rides
                    (id, source, state, created_at_ms, updated_at_ms, point_count, distance_mm,
                     elapsed_time_ms, elapsed_time_running)
                 VALUES (?1, 'live', 'saved', 1, 1, 0, 0, 5, 1)",
                [ride_id.uuid().to_string()],
            )
            .unwrap();

        assert!(load_moving_time_checkpoint(&connection, ride_id).is_err());
    }
}
