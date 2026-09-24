//! Mechanical mobile mappings for the shared capture annotation vocabulary.
use cutout_core::{CaptureLabelState, CaptureLabelTransition, CaptureSessionLabel};
use std::sync::{Arc, Mutex, PoisonError};

/// One requested change to the active writer's capture labels.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureLabelActionDto {
    /// Start a label, closing any exclusive predecessor atomically.
    Start { label: MobileCaptureLabelDto },
    /// Stop a label that is currently active.
    Stop { label: MobileCaptureLabelDto },
}

/// A label request was not recorded; prior admitted labels remain unchanged.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error, uniffi::Error)]
pub enum MobileCaptureAnnotationError {
    /// This capture cannot fit the change and every required closing boundary.
    #[error("capture annotation capacity reached")]
    CapacityReached,
    /// The requested capture no longer owns a recording writer.
    #[error("capture is not recording")]
    NotRecording,
    /// The writer could not accept the metadata update.
    #[error("capture writer rejected the annotation")]
    WriterFailed,
}

macro_rules! capture_labels {
    ($($name:ident),+ $(,)?) => {
        /// Typed capture label. Display names belong to the native localization catalog.
        #[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
        pub enum MobileCaptureLabelDto { $( $name, )+ }
        impl From<MobileCaptureLabelDto> for CaptureSessionLabel {
            fn from(label: MobileCaptureLabelDto) -> Self {
                match label { $( MobileCaptureLabelDto::$name => Self::$name, )+ }
            }
        }
        impl From<CaptureSessionLabel> for MobileCaptureLabelDto {
            fn from(label: CaptureSessionLabel) -> Self {
                match label { $( CaptureSessionLabel::$name => Self::$name, )+ }
            }
        }
    };
}
capture_labels!(
    PoweredOnStationary,
    RollingForward,
    RollingBackward,
    LiftedWheel,
    Charging,
    HeadlightToggled,
    Horn,
    RideModeChange,
    AlarmChange,
    BmsScreen,
    DisconnectReconnect,
    PowerCycle,
    Ride,
    Balancing,
    LowBeamOn,
    LowBeamOff,
    HighBeamOn,
    HighBeamOff,
    PedalsHard,
    PedalsMedium,
    PedalsSoft,
    ResetTrip,
    SoftwareLock,
    SoftwareUnlock,
    TiltbackSpeed,
    AlarmSpeed,
    AngleAdjustment,
    RideMode,
    PwmPercent
);

/// Returns the stable annotation/localization key from the Rust vocabulary.
#[uniffi::export]
#[must_use]
pub fn capture_label_slug(label: MobileCaptureLabelDto) -> String {
    CaptureSessionLabel::from(label).slug().to_owned()
}

/// Tests observation exclusivity without duplicating policy in mobile code.
#[uniffi::export]
#[must_use]
pub fn capture_labels_are_mutually_exclusive(
    left: MobileCaptureLabelDto,
    right: MobileCaptureLabelDto,
) -> bool {
    CaptureSessionLabel::from(left).is_mutually_exclusive(right.into())
}

/// Capture annotation interval state, independent of localization and view lifetime.
#[derive(Debug, Default, uniffi::Object)]
pub struct MobileCaptureLabels {
    inner: Mutex<CaptureLabelState>,
}

#[uniffi::export]
impl MobileCaptureLabels {
    /// Creates an empty set of intervals for one capture presentation owner.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Returns zero, one, or two ordered annotation values; duplicates are no-ops.
    pub fn start(&self, label: MobileCaptureLabelDto) -> Vec<String> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .start(label.into())
            .map(CaptureLabelTransition::annotation_value)
            .collect()
    }

    /// Returns an annotation only when the interval was active.
    pub fn stop(&self, label: MobileCaptureLabelDto) -> Option<String> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .stop(label.into())
            .map(CaptureLabelTransition::annotation_value)
    }

    /// Current active labels in start order.
    #[must_use]
    pub fn active(&self) -> Vec<MobileCaptureLabelDto> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .active()
            .iter()
            .copied()
            .map(Into::into)
            .collect()
    }

    /// Closes active intervals and returns their ordered stop annotations.
    pub fn close_intervals(&self) -> Vec<String> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .close()
            .map(CaptureLabelTransition::annotation_value)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_label_closure_returns_every_stop_through_ffi() {
        let labels = MobileCaptureLabels::new();
        assert_eq!(labels.start(MobileCaptureLabelDto::Ride), ["ride_start"]);
        assert_eq!(
            labels.start(MobileCaptureLabelDto::Balancing),
            ["balancing_start"]
        );
        assert_eq!(labels.close_intervals(), ["ride_stop", "balancing_stop"]);
        assert!(labels.active().is_empty());
        assert!(labels.close_intervals().is_empty());
        assert!(!capture_labels_are_mutually_exclusive(
            MobileCaptureLabelDto::LowBeamOn,
            MobileCaptureLabelDto::LowBeamOn
        ));
        assert!(capture_labels_are_mutually_exclusive(
            MobileCaptureLabelDto::LowBeamOn,
            MobileCaptureLabelDto::LowBeamOff
        ));
    }
}
