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
