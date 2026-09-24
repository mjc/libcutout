//! Shared resource bounds for recordings admitted by the writer and retention preflight.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PevcapLimits {
    pub(crate) artifact_bytes: u64,
    pub(crate) records: u64,
    pub(crate) duration_milliseconds: u64,
}

impl PevcapLimits {
    pub(crate) const DEFAULT: Self = Self {
        artifact_bytes: 512 * 1024 * 1024,
        records: 10_000_000,
        duration_milliseconds: 24 * 60 * 60 * 1_000,
    };

    pub(crate) fn check_artifact_bytes(self, actual: u64) -> Result<(), LimitExceeded> {
        check_limit("artifact bytes", self.artifact_bytes, actual)
    }

    pub(crate) fn check_records(self, actual: u64) -> Result<(), LimitExceeded> {
        check_limit("records", self.records, actual)
    }

    pub(crate) fn check_duration(self, actual: u64) -> Result<(), LimitExceeded> {
        check_limit("duration milliseconds", self.duration_milliseconds, actual)
    }
}

impl Default for PevcapLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LimitExceeded {
    pub(crate) resource: &'static str,
    pub(crate) limit: u64,
    pub(crate) actual: u64,
}

pub(crate) fn check_limit(
    resource: &'static str,
    limit: u64,
    actual: u64,
) -> Result<(), LimitExceeded> {
    (actual <= limit).then_some(()).ok_or(LimitExceeded {
        resource,
        limit,
        actual,
    })
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TimestampRange {
    earliest: Option<u64>,
    latest: Option<u64>,
}

impl TimestampRange {
    pub(crate) fn include(&mut self, timestamp: u64) {
        self.earliest = Some(
            self.earliest
                .map_or(timestamp, |earliest| earliest.min(timestamp)),
        );
        self.latest = Some(
            self.latest
                .map_or(timestamp, |latest| latest.max(timestamp)),
        );
    }

    pub(crate) const fn duration_milliseconds(self) -> u64 {
        match (self.latest, self.earliest) {
            (Some(latest), Some(earliest)) => latest.saturating_sub(earliest),
            _ => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct PevcapUsage {
    record_count: u64,
    record_times: TimestampRange,
    location_count: u64,
    location_times: TimestampRange,
}

impl PevcapUsage {
    pub(crate) fn include_record(&mut self, timestamp: u64, has_phone_location: bool) {
        self.record_count = self.record_count.saturating_add(1);
        self.record_times.include(timestamp);
        if has_phone_location {
            self.include_location(timestamp);
        }
    }

    pub(crate) fn include_location(&mut self, timestamp: u64) {
        self.location_count = self.location_count.saturating_add(1);
        self.location_times.include(timestamp);
    }

    pub(crate) const fn record_count(self) -> u64 {
        self.record_count
    }

    pub(crate) fn check(self, limits: PevcapLimits) -> Result<u64, LimitExceeded> {
        limits.check_records(self.record_count)?;
        let record_duration = self.record_times.duration_milliseconds();
        let location_duration = self.location_times.duration_milliseconds();
        limits.check_duration(record_duration)?;
        limits.check_duration(location_duration)?;
        let duration = if self.location_count == 0 {
            record_duration
        } else {
            location_duration
        };
        limits.check_duration(duration)?;
        Ok(duration)
    }
}
