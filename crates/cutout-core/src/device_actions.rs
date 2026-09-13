//! Semantic device actions and their reported progress.

/// Stable action identity independent of protocol commands and model names.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeviceActionId {
    /// Clear the wheel's trip-distance counter.
    ResetTripMeter,
    /// Enter or exit the wheel's gyro-calibration procedure.
    GyroCalibration,
}

/// Device-reported progress for an action with readable phases.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceActionProgress {
    /// The official app is waiting before it enables the calibration step.
    AdjustingAttitude,
    /// The official app permits the rider to start calibration.
    ReadyToCalibrate,
}
