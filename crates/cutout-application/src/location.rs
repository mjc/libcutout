//! Phone location normalization, receipt-time freshness and shared readback ownership.

use cutout_core::{
    PevcapPhoneLocation, SpeedReadingDto, ValueQualityDto, ValueSourceDto, VerificationStatusDto,
    unix_milliseconds_from_seconds,
};
use cutout_ride_maps::{self as ride_maps, TelemetryFreshness};
use std::sync::{Mutex, PoisonError};

/// Source observations before canonicalization of platform sentinel values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhoneLocationSample {
    pub wall_clock_unix_ms: u64,
    pub source_timestamp_unix_seconds: Option<f64>,
    pub latitude_degrees: f64,
    pub longitude_degrees: f64,
    pub altitude_meters: f64,
    pub horizontal_accuracy_meters: Option<f64>,
    pub vertical_accuracy_meters: Option<f64>,
    pub speed_meters_per_second: Option<f64>,
    pub speed_accuracy_meters_per_second: Option<f64>,
    pub course_degrees: Option<f64>,
    pub course_accuracy_degrees: Option<f64>,
}

impl PhoneLocationSample {
    /// Normalizes Core Location sentinel values into typed absence.
    #[must_use]
    pub fn canonical(self) -> Option<Self> {
        let location = PevcapPhoneLocation {
            wall_clock_unix_ms: self.normalized_wall_clock_unix_ms(),
            latitude_degrees: self.latitude_degrees,
            longitude_degrees: self.longitude_degrees,
            altitude_meters: self.altitude_meters,
            horizontal_accuracy_meters: self.horizontal_accuracy_meters,
            vertical_accuracy_meters: self.vertical_accuracy_meters,
            speed_meters_per_second: self.speed_meters_per_second,
            speed_accuracy_meters_per_second: self.speed_accuracy_meters_per_second,
            course_degrees: self.course_degrees,
            course_accuracy_degrees: self.course_accuracy_degrees,
        }
        .canonical()
        .ok()?;
        Some(Self {
            wall_clock_unix_ms: location.wall_clock_unix_ms,
            source_timestamp_unix_seconds: self.source_timestamp_unix_seconds,
            latitude_degrees: location.latitude_degrees,
            longitude_degrees: location.longitude_degrees,
            altitude_meters: location.altitude_meters,
            horizontal_accuracy_meters: location.horizontal_accuracy_meters,
            vertical_accuracy_meters: location.vertical_accuracy_meters,
            speed_meters_per_second: location.speed_meters_per_second,
            speed_accuracy_meters_per_second: location.speed_accuracy_meters_per_second,
            course_degrees: location.course_degrees,
            course_accuracy_degrees: location.course_accuracy_degrees,
        })
    }

    #[must_use]
    pub fn pevcap_location(self) -> PevcapPhoneLocation {
        PevcapPhoneLocation {
            wall_clock_unix_ms: self.normalized_wall_clock_unix_ms(),
            latitude_degrees: self.latitude_degrees,
            longitude_degrees: self.longitude_degrees,
            altitude_meters: self.altitude_meters,
            horizontal_accuracy_meters: self.horizontal_accuracy_meters,
            vertical_accuracy_meters: self.vertical_accuracy_meters,
            speed_meters_per_second: self.speed_meters_per_second,
            speed_accuracy_meters_per_second: self.speed_accuracy_meters_per_second,
            course_degrees: self.course_degrees,
            course_accuracy_degrees: self.course_accuracy_degrees,
        }
    }

    #[must_use]
    pub fn normalized_wall_clock_unix_ms(self) -> u64 {
        self.source_timestamp_unix_seconds
            .map_or(self.wall_clock_unix_ms, |seconds| {
                unix_milliseconds_from_seconds(seconds).unwrap_or_default()
            })
    }
}

/// Canonical location and the speed projection derived from that observation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhoneLocationSnapshot {
    pub latest_sample: Option<PhoneLocationSample>,
    pub gps_speed: Option<SpeedReadingDto>,
}

/// Receipt-clock freshness, independent of the sample's source wall clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhoneLocationReadback {
    pub snapshot: PhoneLocationSnapshot,
    pub received_at_monotonic_ms: Option<u64>,
    pub freshness: TelemetryFreshness,
    pub stale_after_ms: u64,
}

#[derive(Clone, Copy, Debug, Default)]
struct LocationState {
    sample: Option<PhoneLocationSample>,
    received_at_monotonic_ms: Option<u64>,
}

/// The one location owner shared by presentation, capture and route operations.
#[derive(Debug, Default)]
pub struct PhoneLocationState {
    latest: Mutex<LocationState>,
}

impl PhoneLocationState {
    /// Begins a new location context without inheriting evidence from the old one.
    pub fn clear(&self) {
        *self.latest.lock().unwrap_or_else(PoisonError::into_inner) = LocationState::default();
    }

    /// Retains a source observation with an optional independent receipt time.
    pub fn ingest(
        &self,
        sample: PhoneLocationSample,
        received_at_monotonic_ms: Option<u64>,
    ) -> PhoneLocationSnapshot {
        *self.latest.lock().unwrap_or_else(PoisonError::into_inner) = LocationState {
            sample: Some(sample),
            received_at_monotonic_ms,
        };
        snapshot(Some(sample))
    }

    #[must_use]
    pub fn current_snapshot(&self) -> PhoneLocationSnapshot {
        snapshot(
            self.latest
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .sample,
        )
    }

    #[must_use]
    pub fn current_readback_at(&self, now_monotonic_ms: u64) -> PhoneLocationReadback {
        let latest = self.latest.lock().unwrap_or_else(PoisonError::into_inner);
        let snapshot = snapshot(latest.sample);
        let received_at_monotonic_ms = snapshot.gps_speed.and(latest.received_at_monotonic_ms);
        let freshness = ride_maps::telemetry_freshness(
            received_at_monotonic_ms.map(ride_maps::MonotonicMilliseconds::new),
            ride_maps::MonotonicMilliseconds::new(now_monotonic_ms),
        );
        PhoneLocationReadback {
            snapshot,
            received_at_monotonic_ms,
            freshness,
            stale_after_ms: ride_maps::TELEMETRY_FRESHNESS_MILLISECONDS,
        }
    }
}

/// Normalizes one sample and projects reported/inferred speed with its provenance.
#[must_use]
pub fn snapshot(sample: Option<PhoneLocationSample>) -> PhoneLocationSnapshot {
    let sample = sample.and_then(PhoneLocationSample::canonical);
    PhoneLocationSnapshot {
        latest_sample: sample,
        gps_speed: sample.and_then(phone_location_speed),
    }
}

fn phone_location_speed(sample: PhoneLocationSample) -> Option<SpeedReadingDto> {
    let reported = sample.speed_meters_per_second.is_some();
    let speed = sample.speed_meters_per_second.unwrap_or(0.0);
    if !speed.is_finite() || speed < 0.0 {
        return None;
    }
    Some(SpeedReadingDto {
        value: round_f64_to_i32(speed * 1_000.0),
        source: if reported {
            ValueSourceDto::Reported
        } else {
            ValueSourceDto::Estimated
        },
        quality: if reported {
            ValueQualityDto::Known
        } else {
            ValueQualityDto::Inferred
        },
        verification: if reported {
            VerificationStatusDto::SourceVerified
        } else {
            VerificationStatusDto::Inferred
        },
    })
}

fn round_f64_to_i32(value: f64) -> i32 {
    if value.is_nan() {
        return 0;
    }
    if value <= f64::from(i32::MIN) {
        return i32::MIN;
    }
    if value >= f64::from(i32::MAX) {
        return i32::MAX;
    }
    let rounded = value.round();
    let mut low = i64::from(i32::MIN);
    let mut high = i64::from(i32::MAX);
    while low <= high {
        let midpoint = low + (high - low) / 2;
        let Ok(candidate) = i32::try_from(midpoint) else {
            return 0;
        };
        match f64::from(candidate).total_cmp(&rounded) {
            std::cmp::Ordering::Less => low = midpoint + 1,
            std::cmp::Ordering::Greater => high = midpoint - 1,
            std::cmp::Ordering::Equal => return candidate,
        }
    }
    0
}
