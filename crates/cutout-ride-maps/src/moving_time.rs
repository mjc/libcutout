//! Receipt-causal moving-time continuation for recorded ride evidence.
//!
//! This module owns the pure interval rules used by the persistence worker and Rust summary DTOs.

use cutout_core::Speed;
use thiserror::Error;

use crate::recording::TELEMETRY_FRESHNESS_MILLISECONDS;
use crate::{MonotonicMilliseconds, RideDurationMilliseconds};

/// The source whose speed evidence is being recorded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MovingTimeSource {
    /// Speed from the currently verified and associated vehicle.
    Vehicle,
    /// Speed supplied by an accepted phone location.
    PhoneGps,
}

/// One speed observation, including optional phone-speed evidence.
///
/// A `None` phone speed advances receipt and same-source ordering but does not replace or refresh
/// the most recent valid GPS speed. Vehicle observations must always contain a speed, including
/// zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MovingTimeObservation {
    /// Origin of this observation.
    pub source: MovingTimeSource,
    /// Signed speed in millimetres per second; `None` means no valid phone speed was supplied.
    pub speed_millimetres_per_second: Option<i32>,
    /// Original source timestamp, not callback receipt time.
    pub source_at: MonotonicMilliseconds,
    /// Monotonic time when this accepted observation reached the recording owner.
    pub receipt_at: MonotonicMilliseconds,
}

/// A valid source value needed to continue the current live interval.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MovingTimeSourceStateParts {
    /// Effective speed in millimetres per second.
    pub speed_millimetres_per_second: i32,
    /// Original timestamp of this effective speed value.
    pub source_at_milliseconds: u64,
}

/// Scalar and source state that can be stored in explicit integer database columns.
///
/// Latest source timestamps are kept separately from effective valid values. An invalid phone
/// callback advances its source-order watermark without extending the last valid phone value's
/// freshness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MovingTimeStateParts {
    /// Accumulated time classified as moving.
    pub moving_milliseconds: u64,
    /// Accumulated time covered by fresh speed evidence, moving or stationary.
    pub observed_milliseconds: u64,
    /// Accumulated active time for which no source was fresh.
    pub unknown_milliseconds: u64,
    /// Causal receipt cursor for the active interval, if one exists.
    pub cursor_milliseconds: Option<u64>,
    /// Latest effective valid phone speed and its original source time.
    pub phone_gps: Option<MovingTimeSourceStateParts>,
    /// Latest effective verified vehicle speed and its original source time.
    pub vehicle: Option<MovingTimeSourceStateParts>,
    /// Latest phone callback source timestamp, including callbacks without valid speed.
    pub latest_phone_gps_source_milliseconds: Option<u64>,
    /// Latest verified vehicle callback source timestamp.
    pub latest_vehicle_source_milliseconds: Option<u64>,
    /// Typed coverage, independent of whether the numeric moving total is zero.
    pub availability: MovingTimeAvailability,
    /// Whether a lifecycle or restore boundary makes complete coverage impossible.
    pub incomplete_history: bool,
}

/// Whether a moving-time value is unavailable, partially observed, or fully covered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MovingTimeAvailability {
    /// No usable recorded interval evidence exists.
    Unavailable,
    /// Some intervals are known, but one or more gaps or forced boundaries remain unknown.
    Partial,
    /// All active intervals represented by this continuation have source coverage.
    Complete,
}

/// A non-mutating moving-time summary projected to one monotonic instant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MovingTimeSummary {
    /// Time classified as moving; availability distinguishes measured zero from no evidence.
    pub moving: RideDurationMilliseconds,
    /// Time covered by fresh moving or stationary evidence.
    pub observed: RideDurationMilliseconds,
    /// Time not covered by any fresh source.
    pub unknown: RideDurationMilliseconds,
    /// Typed evidence coverage for the projected value.
    pub availability: MovingTimeAvailability,
}

/// Invalid or causally unordered moving-time evidence.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum MovingTimeError {
    /// An observation receipt predates its source timestamp.
    #[error("moving-time observation receipt predates its source timestamp")]
    ReceiptBeforeSource,
    /// An observation is older than the continuation's receipt cursor.
    #[error("moving-time observation receipt predates the causal cursor")]
    ReceiptBeforeCursor,
    /// A source observation is stale by the time it is received.
    #[error("moving-time source is stale at receipt")]
    StaleAtReceipt,
    /// A verified vehicle event cannot omit its speed.
    #[error("verified vehicle motion must include a speed value")]
    MissingVehicleSpeed,
    /// Phone GPS speed evidence must be nonnegative.
    #[error("phone GPS speed evidence must be nonnegative")]
    InvalidPhoneGpsSpeed,
    /// An observation repeats or predates the latest source timestamp for that source.
    #[error("moving-time source timestamp is not newer")]
    SourceTimestampNotNewer,
    /// Persisted totals or evidence are internally inconsistent.
    #[error("moving-time continuation state is inconsistent")]
    InvalidState,
    /// A duration counter would overflow its persisted integer representation.
    #[error("moving-time duration counter overflow")]
    DurationOverflow,
}

/// Pure Rust continuation state for moving-time interval accounting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MovingTimeContinuation {
    parts: MovingTimeStateParts,
}

impl MovingTimeContinuation {
    /// Starts a live ride at its active monotonic start time.
    #[must_use]
    pub const fn new_live(start_at: MonotonicMilliseconds) -> Self {
        Self {
            parts: MovingTimeStateParts {
                moving_milliseconds: 0,
                observed_milliseconds: 0,
                unknown_milliseconds: 0,
                cursor_milliseconds: Some(start_at.as_u64()),
                phone_gps: None,
                vehicle: None,
                latest_phone_gps_source_milliseconds: None,
                latest_vehicle_source_milliseconds: None,
                availability: MovingTimeAvailability::Unavailable,
                incomplete_history: false,
            },
        }
    }

    /// Creates a continuation for an imported or historical ride with no interval evidence.
    #[must_use]
    pub const fn unavailable() -> Self {
        Self {
            parts: MovingTimeStateParts {
                moving_milliseconds: 0,
                observed_milliseconds: 0,
                unknown_milliseconds: 0,
                cursor_milliseconds: None,
                phone_gps: None,
                vehicle: None,
                latest_phone_gps_source_milliseconds: None,
                latest_vehicle_source_milliseconds: None,
                availability: MovingTimeAvailability::Unavailable,
                incomplete_history: true,
            },
        }
    }

    /// Rehydrates a continuation in the current writer generation, retaining source state.
    ///
    /// # Errors
    ///
    /// Returns [`MovingTimeError::InvalidState`] when persisted counters, availability, or source
    /// timestamps and cursor are inconsistent.
    pub fn from_state_parts(parts: MovingTimeStateParts) -> Result<Self, MovingTimeError> {
        if parts.moving_milliseconds > parts.observed_milliseconds
            || parts
                .observed_milliseconds
                .checked_add(parts.unknown_milliseconds)
                .is_none()
            || parts
                .phone_gps
                .is_some_and(|source| source.speed_millimetres_per_second < 0)
            || (parts.availability == MovingTimeAvailability::Complete
                && (parts.unknown_milliseconds > 0 || parts.incomplete_history))
            || (parts.availability == MovingTimeAvailability::Unavailable
                && (parts.observed_milliseconds > 0
                    || parts.unknown_milliseconds > 0
                    || parts.phone_gps.is_some()
                    || parts.vehicle.is_some()))
            || (parts.availability == MovingTimeAvailability::Partial
                && parts.unknown_milliseconds == 0
                && !parts.incomplete_history)
        {
            return Err(MovingTimeError::InvalidState);
        }
        if let Some(cursor) = parts.cursor_milliseconds {
            if parts.phone_gps.is_some_and(|source| {
                source.source_at_milliseconds > cursor
                    || parts
                        .latest_phone_gps_source_milliseconds
                        .is_none_or(|latest| latest < source.source_at_milliseconds)
            }) || parts.vehicle.is_some_and(|source| {
                source.source_at_milliseconds > cursor
                    || parts
                        .latest_vehicle_source_milliseconds
                        .is_none_or(|latest| latest < source.source_at_milliseconds)
            }) || parts
                .latest_phone_gps_source_milliseconds
                .is_some_and(|latest| latest > cursor)
                || parts
                    .latest_vehicle_source_milliseconds
                    .is_some_and(|latest| latest > cursor)
            {
                return Err(MovingTimeError::InvalidState);
            }
        } else if parts.phone_gps.is_some()
            || parts.vehicle.is_some()
            || parts.latest_phone_gps_source_milliseconds.is_some()
            || parts.latest_vehicle_source_milliseconds.is_some()
        {
            return Err(MovingTimeError::InvalidState);
        }
        Ok(Self { parts })
    }

    /// Restores committed totals across a process/monotonic epoch boundary.
    ///
    /// This retains committed totals but clears open evidence without projecting its old-epoch
    /// freshness tail. A previously available metric becomes partial because the restart gap is
    /// not observed.
    ///
    /// # Errors
    ///
    /// Returns [`MovingTimeError::InvalidState`] when the persisted continuation is inconsistent.
    pub fn restored_after_restart(
        parts: MovingTimeStateParts,
        _at: MonotonicMilliseconds,
    ) -> Result<Self, MovingTimeError> {
        let mut restored = Self::from_state_parts(parts)?.parts;
        restored.cursor_milliseconds = None;
        restored.phone_gps = None;
        restored.vehicle = None;
        restored.latest_phone_gps_source_milliseconds = None;
        restored.latest_vehicle_source_milliseconds = None;
        restored.incomplete_history = true;
        if restored.availability != MovingTimeAvailability::Unavailable {
            restored.availability = MovingTimeAvailability::Partial;
        }
        Self::from_state_parts(restored)
    }

    /// Applies a causally accepted source observation.
    ///
    /// # Errors
    ///
    /// Returns an error when the receipt predates the source timestamp or causal cursor, the
    /// source timestamp is repeated or older, the evidence is stale at receipt, the source or
    /// speed is invalid, or advancing a duration counter would overflow.
    pub fn observe(&mut self, observation: MovingTimeObservation) -> Result<(), MovingTimeError> {
        let source_at = observation.source_at.as_u64();
        let receipt_at = observation.receipt_at.as_u64();
        if receipt_at < source_at {
            return Err(MovingTimeError::ReceiptBeforeSource);
        }
        if observation.speed_millimetres_per_second.is_some()
            && receipt_at - source_at > TELEMETRY_FRESHNESS_MILLISECONDS
        {
            return Err(MovingTimeError::StaleAtReceipt);
        }
        if observation.source == MovingTimeSource::Vehicle
            && observation.speed_millimetres_per_second.is_none()
        {
            return Err(MovingTimeError::MissingVehicleSpeed);
        }
        if observation.source == MovingTimeSource::PhoneGps
            && observation
                .speed_millimetres_per_second
                .is_some_and(|speed| speed < 0)
        {
            return Err(MovingTimeError::InvalidPhoneGpsSpeed);
        }

        let mut next = *self;
        if next
            .latest_source_at(observation.source)
            .is_some_and(|latest| source_at <= latest)
        {
            return Err(MovingTimeError::SourceTimestampNotNewer);
        }
        let had_cursor = next.parts.cursor_milliseconds.is_some();
        next.advance_to(receipt_at)?;
        if !had_cursor {
            next.parts.incomplete_history = true;
        }
        match observation.source {
            MovingTimeSource::PhoneGps => {
                next.parts.latest_phone_gps_source_milliseconds = Some(source_at);
                if let Some(speed) = observation.speed_millimetres_per_second {
                    next.parts.phone_gps = Some(MovingTimeSourceStateParts {
                        speed_millimetres_per_second: speed,
                        source_at_milliseconds: source_at,
                    });
                    next.mark_evidence_available();
                }
            }
            MovingTimeSource::Vehicle => {
                next.parts.latest_vehicle_source_milliseconds = Some(source_at);
                let speed = observation
                    .speed_millimetres_per_second
                    .ok_or(MovingTimeError::MissingVehicleSpeed)?;
                next.parts.vehicle = Some(MovingTimeSourceStateParts {
                    speed_millimetres_per_second: speed,
                    source_at_milliseconds: source_at,
                });
                next.mark_evidence_available();
            }
        }
        *self = next;
        Ok(())
    }

    /// Projects a read-only summary through `as_of`, clipping source evidence at its expiry.
    #[must_use]
    pub fn projected_at(&self, as_of: MonotonicMilliseconds) -> MovingTimeSummary {
        let mut projected = *self;
        if projected
            .parts
            .cursor_milliseconds
            .is_some_and(|cursor| as_of.as_u64() < cursor)
        {
            return projected.summary();
        }
        if projected.advance_to(as_of.as_u64()).is_err() {
            projected.parts.availability = MovingTimeAvailability::Partial;
        }
        projected.summary()
    }

    /// Projects the prefix before a recorded point's source time.
    ///
    /// Returns `None` when the point predates the receipt cursor, since that prefix cannot be
    /// reconstructed without retroactively changing already-accounted intervals.
    #[must_use]
    pub fn prefix_before_source_time(
        &self,
        source_at: MonotonicMilliseconds,
    ) -> Option<MovingTimeSummary> {
        let cursor = self.parts.cursor_milliseconds?;
        (source_at.as_u64() >= cursor).then(|| self.projected_at(source_at))
    }

    /// Closes the current active interval at a lifecycle or source-generation boundary.
    /// Calling this on an already-closed continuation is a no-op.
    ///
    /// # Errors
    ///
    /// Returns an error when the close time predates the causal cursor or advancing a duration
    /// counter would overflow.
    pub fn close_at(&mut self, at: MonotonicMilliseconds) -> Result<(), MovingTimeError> {
        if self.parts.cursor_milliseconds.is_none() {
            return Ok(());
        }
        let mut next = *self;
        next.advance_to(at.as_u64())?;
        next.parts.phone_gps = None;
        next.parts.vehicle = None;
        next.parts.latest_phone_gps_source_milliseconds = None;
        next.parts.latest_vehicle_source_milliseconds = None;
        // No cursor represents a closed interval. Keeping the close instant as a cursor would
        // make read-only projection count paused/stopped time as unknown coverage.
        next.parts.cursor_milliseconds = None;
        *self = next;
        Ok(())
    }

    /// Starts a fresh active cursor after a pause without including paused time.
    ///
    /// # Errors
    ///
    /// Returns [`MovingTimeError::ReceiptBeforeCursor`] when the resume time predates the current
    /// causal cursor.
    pub fn resume_at(&mut self, at: MonotonicMilliseconds) -> Result<(), MovingTimeError> {
        let at = at.as_u64();
        if self
            .parts
            .cursor_milliseconds
            .is_some_and(|cursor| at < cursor)
        {
            return Err(MovingTimeError::ReceiptBeforeCursor);
        }
        self.parts.cursor_milliseconds = Some(at);
        self.parts.phone_gps = None;
        self.parts.vehicle = None;
        self.parts.latest_phone_gps_source_milliseconds = None;
        self.parts.latest_vehicle_source_milliseconds = None;
        if self.parts.availability == MovingTimeAvailability::Unavailable {
            self.parts.incomplete_history = true;
        }
        Ok(())
    }

    /// Returns explicit integer state parts for transactional persistence.
    #[must_use]
    pub const fn state_parts(self) -> MovingTimeStateParts {
        self.parts
    }

    fn latest_source_at(self, source: MovingTimeSource) -> Option<u64> {
        match source {
            MovingTimeSource::PhoneGps => self.parts.latest_phone_gps_source_milliseconds,
            MovingTimeSource::Vehicle => self.parts.latest_vehicle_source_milliseconds,
        }
    }

    fn mark_evidence_available(&mut self) {
        if self.parts.availability == MovingTimeAvailability::Unavailable {
            self.parts.availability =
                if self.parts.incomplete_history || self.parts.unknown_milliseconds > 0 {
                    MovingTimeAvailability::Partial
                } else {
                    MovingTimeAvailability::Complete
                };
        }
    }

    fn summary(self) -> MovingTimeSummary {
        MovingTimeSummary {
            moving: RideDurationMilliseconds::new(self.parts.moving_milliseconds),
            observed: RideDurationMilliseconds::new(self.parts.observed_milliseconds),
            unknown: RideDurationMilliseconds::new(self.parts.unknown_milliseconds),
            availability: self.parts.availability,
        }
    }

    fn advance_to(&mut self, at: u64) -> Result<(), MovingTimeError> {
        let Some(mut cursor) = self.parts.cursor_milliseconds else {
            self.parts.cursor_milliseconds = Some(at);
            self.parts.incomplete_history = true;
            return Ok(());
        };
        if at < cursor {
            return Err(MovingTimeError::ReceiptBeforeCursor);
        }
        while cursor < at {
            if let Some(source) = self.selected_source_at(cursor) {
                let expires_at = source
                    .source_at_milliseconds
                    .saturating_add(TELEMETRY_FRESHNESS_MILLISECONDS);
                let end = at.min(expires_at);
                if end > cursor {
                    self.add_observed_interval(
                        end - cursor,
                        Speed::from_millimetres_per_second(source.speed_millimetres_per_second)
                            .is_moving(),
                    )?;
                    cursor = end;
                } else {
                    self.add_unknown_interval(at - cursor)?;
                    cursor = at;
                }
            } else {
                let next_expiration = self
                    .parts
                    .phone_gps
                    .into_iter()
                    .chain(self.parts.vehicle)
                    .map(|source| {
                        source
                            .source_at_milliseconds
                            .saturating_add(TELEMETRY_FRESHNESS_MILLISECONDS)
                    })
                    .filter(|expires_at| *expires_at > cursor)
                    .min();
                let end = next_expiration.map_or(at, |expiry| at.min(expiry));
                self.add_unknown_interval(end - cursor)?;
                cursor = end;
            }
        }
        self.parts.cursor_milliseconds = Some(at);
        Ok(())
    }

    fn selected_source_at(self, at: u64) -> Option<MovingTimeSourceStateParts> {
        let fresh = |source: MovingTimeSourceStateParts| {
            (source.source_at_milliseconds <= at
                && at - source.source_at_milliseconds < TELEMETRY_FRESHNESS_MILLISECONDS)
                .then_some(source)
        };
        self.parts
            .vehicle
            .and_then(fresh)
            .or_else(|| self.parts.phone_gps.and_then(fresh))
    }

    fn add_observed_interval(
        &mut self,
        duration: u64,
        moving: bool,
    ) -> Result<(), MovingTimeError> {
        let observed_milliseconds = self
            .parts
            .observed_milliseconds
            .checked_add(duration)
            .ok_or(MovingTimeError::DurationOverflow)?;
        observed_milliseconds
            .checked_add(self.parts.unknown_milliseconds)
            .ok_or(MovingTimeError::DurationOverflow)?;
        if moving {
            let moving_milliseconds = self
                .parts
                .moving_milliseconds
                .checked_add(duration)
                .ok_or(MovingTimeError::DurationOverflow)?;
            self.parts.moving_milliseconds = moving_milliseconds;
        }
        self.parts.observed_milliseconds = observed_milliseconds;
        Ok(())
    }

    fn add_unknown_interval(&mut self, duration: u64) -> Result<(), MovingTimeError> {
        let unknown_milliseconds = self
            .parts
            .unknown_milliseconds
            .checked_add(duration)
            .ok_or(MovingTimeError::DurationOverflow)?;
        self.parts
            .observed_milliseconds
            .checked_add(unknown_milliseconds)
            .ok_or(MovingTimeError::DurationOverflow)?;
        self.parts.unknown_milliseconds = unknown_milliseconds;
        self.parts.availability = MovingTimeAvailability::Partial;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MovingTimeAvailability, MovingTimeContinuation, MovingTimeError, MovingTimeObservation,
        MovingTimeSource, MovingTimeSourceStateParts,
    };
    use crate::MonotonicMilliseconds;
    use crate::recording::TELEMETRY_FRESHNESS_MILLISECONDS;
    use cutout_core::Speed;

    fn at(milliseconds: u64) -> MonotonicMilliseconds {
        MonotonicMilliseconds::new(milliseconds)
    }

    fn observation(
        source: MovingTimeSource,
        speed: Option<i32>,
        source_at: u64,
        receipt_at: u64,
    ) -> MovingTimeObservation {
        MovingTimeObservation {
            source,
            speed_millimetres_per_second: speed,
            source_at: at(source_at),
            receipt_at: at(receipt_at),
        }
    }

    #[test]
    fn phone_threshold_is_inclusive_and_freshness_covers_exactly_two_seconds() {
        assert!(!Speed::from_millimetres_per_second(499).is_moving());
        assert!(Speed::from_millimetres_per_second(500).is_moving());
        assert!(Speed::from_millimetres_per_second(501).is_moving());
        assert_eq!(TELEMETRY_FRESHNESS_MILLISECONDS, 2_000);

        let mut state = MovingTimeContinuation::new_live(at(0));
        state
            .observe(observation(MovingTimeSource::PhoneGps, Some(499), 0, 0))
            .unwrap();
        let below = state.projected_at(at(1_000));
        assert_eq!(below.moving.as_u64(), 0);
        assert_eq!(below.observed.as_u64(), 1_000);

        state
            .observe(observation(
                MovingTimeSource::PhoneGps,
                Some(500),
                1_000,
                1_000,
            ))
            .unwrap();
        let threshold = state.projected_at(at(1_500));
        assert_eq!(threshold.moving.as_u64(), 500);

        state
            .observe(observation(
                MovingTimeSource::PhoneGps,
                Some(501),
                1_500,
                1_500,
            ))
            .unwrap();
        let fresh_at_boundary = state.projected_at(at(3_500));
        assert_eq!(fresh_at_boundary.moving.as_u64(), 2_500);
        let stale_after_boundary = state.projected_at(at(3_501));
        // Observed includes the stationary 499 mm/s first interval as well as both moving
        // intervals; the last millisecond is unknown after the source expires.
        assert_eq!(stale_after_boundary.observed.as_u64(), 3_500);
        assert_eq!(stale_after_boundary.unknown.as_u64(), 1);
    }

    #[test]
    fn fresh_vehicle_zero_wins_then_gps_fallback_starts_at_vehicle_expiry() {
        let mut state = MovingTimeContinuation::new_live(at(0));
        state
            .observe(observation(MovingTimeSource::Vehicle, Some(0), 0, 0))
            .unwrap();
        state
            .observe(observation(
                MovingTimeSource::PhoneGps,
                Some(700),
                1_000,
                1_000,
            ))
            .unwrap();

        let at_wheel_expiry = state.projected_at(at(2_000));
        assert_eq!(at_wheel_expiry.moving.as_u64(), 0);
        assert_eq!(at_wheel_expiry.observed.as_u64(), 2_000);

        let after_fallback = state.projected_at(at(2_500));
        assert_eq!(after_fallback.moving.as_u64(), 500);
        let after_gps_expiry = state.projected_at(at(3_001));
        assert_eq!(after_gps_expiry.moving.as_u64(), 1_000);
        assert_eq!(after_gps_expiry.unknown.as_u64(), 1);
    }

    #[test]
    fn measured_stationary_zero_is_distinct_from_unavailable_zero() {
        let unavailable = MovingTimeContinuation::unavailable().projected_at(at(1_000));
        assert_eq!(unavailable.moving.as_u64(), 0);
        assert_eq!(
            unavailable.availability,
            MovingTimeAvailability::Unavailable
        );

        let new_live_without_evidence =
            MovingTimeContinuation::new_live(at(0)).projected_at(at(1_000));
        assert_eq!(new_live_without_evidence.unknown.as_u64(), 1_000);
        assert_eq!(
            new_live_without_evidence.availability,
            MovingTimeAvailability::Partial
        );

        let mut stationary = MovingTimeContinuation::new_live(at(0));
        stationary
            .observe(observation(MovingTimeSource::Vehicle, Some(0), 0, 0))
            .unwrap();
        let measured = stationary.projected_at(at(1_000));
        assert_eq!(measured.moving.as_u64(), 0);
        assert_eq!(measured.observed.as_u64(), 1_000);
        assert_eq!(measured.availability, MovingTimeAvailability::Complete);
    }

    #[test]
    fn late_fresh_wheel_zero_applies_from_receipt_not_source_time() {
        let mut state = MovingTimeContinuation::new_live(at(0));
        state
            .observe(observation(
                MovingTimeSource::PhoneGps,
                Some(700),
                2_200,
                2_200,
            ))
            .unwrap();
        state
            .observe(observation(
                MovingTimeSource::Vehicle,
                Some(0),
                1_400,
                2_300,
            ))
            .unwrap();

        let projected = state.projected_at(at(3_000));
        assert_eq!(projected.moving.as_u64(), 100);
        assert_eq!(projected.availability, MovingTimeAvailability::Partial);
    }

    #[test]
    fn same_receipt_source_order_is_irrelevant_and_vehicle_wins() {
        let mut gps_first = MovingTimeContinuation::new_live(at(0));
        gps_first
            .observe(observation(MovingTimeSource::PhoneGps, Some(700), 10, 10))
            .unwrap();
        gps_first
            .observe(observation(MovingTimeSource::Vehicle, Some(0), 10, 10))
            .unwrap();

        let mut vehicle_first = MovingTimeContinuation::new_live(at(0));
        vehicle_first
            .observe(observation(MovingTimeSource::Vehicle, Some(0), 10, 10))
            .unwrap();
        vehicle_first
            .observe(observation(MovingTimeSource::PhoneGps, Some(700), 10, 10))
            .unwrap();

        assert_eq!(gps_first.projected_at(at(1_010)).moving.as_u64(), 0);
        assert_eq!(
            gps_first.projected_at(at(1_010)),
            vehicle_first.projected_at(at(1_010))
        );
    }

    #[test]
    fn missing_phone_speed_does_not_refresh_last_valid_speed() {
        let mut state = MovingTimeContinuation::new_live(at(0));
        state
            .observe(observation(MovingTimeSource::PhoneGps, Some(700), 0, 0))
            .unwrap();
        state
            .observe(observation(MovingTimeSource::PhoneGps, None, 1_500, 1_500))
            .unwrap();

        let projected = state.projected_at(at(2_501));
        assert_eq!(projected.moving.as_u64(), 2_000);
        assert_eq!(projected.unknown.as_u64(), 501);
    }

    #[test]
    fn negative_phone_speed_is_rejected_without_mutating_continuation() {
        let mut state = MovingTimeContinuation::new_live(at(0));
        let before = state;

        assert!(
            state
                .observe(observation(MovingTimeSource::PhoneGps, Some(-700), 0, 0))
                .is_err()
        );
        assert_eq!(state, before);
    }

    #[test]
    fn negative_persisted_phone_speed_is_rejected_during_hydration() {
        let mut state = MovingTimeContinuation::new_live(at(0));
        state
            .observe(observation(MovingTimeSource::PhoneGps, Some(700), 0, 0))
            .unwrap();
        let mut parts = state.state_parts();
        parts.phone_gps = Some(MovingTimeSourceStateParts {
            speed_millimetres_per_second: -700,
            source_at_milliseconds: 0,
        });

        assert_eq!(
            MovingTimeContinuation::from_state_parts(parts),
            Err(MovingTimeError::InvalidState)
        );
    }

    #[test]
    fn negative_verified_vehicle_speed_counts_reverse_motion() {
        let mut state = MovingTimeContinuation::new_live(at(0));
        state
            .observe(observation(MovingTimeSource::Vehicle, Some(-500), 0, 0))
            .unwrap();

        let projected = state.projected_at(at(500));
        assert_eq!(projected.moving.as_u64(), 500);
        assert_eq!(projected.observed.as_u64(), 500);
        assert_eq!(projected.availability, MovingTimeAvailability::Complete);
    }

    #[test]
    fn pause_and_resume_do_not_count_the_paused_gap() {
        let mut state = MovingTimeContinuation::new_live(at(0));
        state
            .observe(observation(MovingTimeSource::PhoneGps, Some(700), 0, 0))
            .unwrap();
        state.close_at(at(1_000)).unwrap();
        state.resume_at(at(5_000)).unwrap();

        let projected = state.projected_at(at(6_000));
        assert_eq!(projected.moving.as_u64(), 1_000);
        assert_eq!(projected.observed.as_u64(), 1_000);
        assert_eq!(projected.unknown.as_u64(), 1_000);
        assert_eq!(projected.availability, MovingTimeAvailability::Partial);
    }

    #[test]
    fn closed_pause_summary_stays_fixed_until_resume_starts_a_new_cursor() {
        let mut state = MovingTimeContinuation::new_live(at(0));
        state
            .observe(observation(MovingTimeSource::PhoneGps, Some(700), 0, 0))
            .unwrap();
        state.close_at(at(1_000)).unwrap();

        let while_paused = state.projected_at(at(9_000));
        assert_eq!(while_paused.moving.as_u64(), 1_000);
        assert_eq!(while_paused.observed.as_u64(), 1_000);
        assert_eq!(while_paused.unknown.as_u64(), 0);
        assert_eq!(while_paused.availability, MovingTimeAvailability::Complete);

        state.resume_at(at(10_000)).unwrap();
        state
            .observe(observation(
                MovingTimeSource::PhoneGps,
                Some(700),
                10_000,
                10_000,
            ))
            .unwrap();
        let after_resume = state.projected_at(at(11_000));
        assert_eq!(after_resume.moving.as_u64(), 2_000);
        assert_eq!(after_resume.observed.as_u64(), 2_000);
        assert_eq!(after_resume.unknown.as_u64(), 0);
        assert_eq!(after_resume.availability, MovingTimeAvailability::Complete);
    }

    #[test]
    fn repeated_close_at_same_instant_preserves_closed_state_and_projection() {
        let mut state = MovingTimeContinuation::new_live(at(0));
        state
            .observe(observation(MovingTimeSource::PhoneGps, Some(700), 0, 0))
            .unwrap();
        state.close_at(at(1_000)).unwrap();
        let closed_parts = state.state_parts();
        let closed_projection = state.projected_at(at(9_000));

        state.close_at(at(1_000)).unwrap();

        assert_eq!(state.state_parts(), closed_parts);
        assert_eq!(state.projected_at(at(9_000)), closed_projection);
    }

    #[test]
    fn repeated_close_later_preserves_closed_state_and_projection() {
        let mut state = MovingTimeContinuation::new_live(at(0));
        state
            .observe(observation(MovingTimeSource::PhoneGps, Some(700), 0, 0))
            .unwrap();
        state.close_at(at(1_000)).unwrap();
        let closed_parts = state.state_parts();
        let closed_projection = state.projected_at(at(9_000));

        state.close_at(at(9_000)).unwrap();

        assert_eq!(state.state_parts(), closed_parts);
        assert_eq!(state.projected_at(at(9_000)), closed_projection);
    }

    #[test]
    fn restore_keeps_totals_but_does_not_project_old_epoch_source_tail() {
        let mut state = MovingTimeContinuation::new_live(at(0));
        state
            .observe(observation(MovingTimeSource::PhoneGps, Some(700), 0, 0))
            .unwrap();
        let restored =
            MovingTimeContinuation::restored_after_restart(state.state_parts(), at(10_000))
                .unwrap();

        let projected = restored.projected_at(at(12_000));
        assert_eq!(projected.moving.as_u64(), 0);
        assert_eq!(projected.observed.as_u64(), 0);
        assert_eq!(projected.unknown.as_u64(), 0);
        assert_eq!(projected.availability, MovingTimeAvailability::Partial);
    }

    #[test]
    fn source_timestamp_order_is_per_source_and_stale_at_receipt_is_rejected() {
        let mut state = MovingTimeContinuation::new_live(at(0));
        state
            .observe(observation(MovingTimeSource::PhoneGps, Some(700), 100, 100))
            .unwrap();
        assert_eq!(
            state.observe(observation(MovingTimeSource::PhoneGps, Some(0), 100, 200)),
            Err(MovingTimeError::SourceTimestampNotNewer)
        );
        assert_eq!(
            state.observe(observation(MovingTimeSource::Vehicle, Some(0), 0, 2_001)),
            Err(MovingTimeError::StaleAtReceipt)
        );
        state
            .observe(observation(MovingTimeSource::Vehicle, Some(0), 0, 1_000))
            .unwrap();
    }

    #[test]
    fn pre_point_prefix_is_unavailable_when_point_predates_the_receipt_cursor() {
        let mut state = MovingTimeContinuation::new_live(at(0));
        state
            .observe(observation(
                MovingTimeSource::PhoneGps,
                Some(700),
                1_000,
                1_000,
            ))
            .unwrap();
        assert!(state.prefix_before_source_time(at(999)).is_none());
        assert_eq!(
            state
                .prefix_before_source_time(at(1_500))
                .unwrap()
                .moving
                .as_u64(),
            500
        );
    }

    #[test]
    fn persisted_counter_invariants_reject_moving_time_without_observed_coverage() {
        let mut parts = MovingTimeContinuation::new_live(at(0)).state_parts();
        parts.moving_milliseconds = 1;
        assert_eq!(
            MovingTimeContinuation::from_state_parts(parts),
            Err(MovingTimeError::InvalidState)
        );

        let mut parts = MovingTimeContinuation::new_live(at(0)).state_parts();
        parts.observed_milliseconds = u64::MAX;
        parts.unknown_milliseconds = 1;
        assert_eq!(
            MovingTimeContinuation::from_state_parts(parts),
            Err(MovingTimeError::InvalidState)
        );
    }
}
