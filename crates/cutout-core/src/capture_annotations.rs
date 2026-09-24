//! Bounded capture annotations with space reserved for every open interval.

use arrayvec::ArrayVec;
use thiserror::Error;

use crate::{
    CaptureLabelState, CaptureLabelTransition, CaptureSessionLabel,
    PEVCAP_CAPTURE_LABEL_ANNOTATION_KEY, PEVCAP_MAX_ANNOTATIONS,
};

/// An annotation batch would leave insufficient room to finish the capture.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
#[error("capture annotation capacity reached")]
pub struct CaptureAnnotationCapacityReached;

/// Admitted header evidence. Updates are atomic and always leave room to close
/// active labels using the current PEVCAP header format.
#[derive(Clone, Debug, Default)]
pub struct CaptureAnnotations {
    entries: ArrayVec<String, PEVCAP_MAX_ANNOTATIONS>,
    labels: CaptureLabelState,
}

impl CaptureAnnotations {
    /// Admitted annotations in capture order.
    #[must_use]
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    /// Intervals represented by the admitted annotations, in start order.
    #[must_use]
    pub fn active_labels(&self) -> &[CaptureSessionLabel] {
        self.labels.active()
    }

    /// Appends a complete batch or leaves all prior evidence unchanged.
    ///
    /// # Errors
    /// Returns a capacity error if the batch would consume reserved closure space.
    pub fn try_append(
        &mut self,
        annotations: impl IntoIterator<Item = String>,
    ) -> Result<(), CaptureAnnotationCapacityReached> {
        let mut entries = self.entries.clone();
        for annotation in annotations {
            entries
                .try_push(annotation)
                .map_err(|_| CaptureAnnotationCapacityReached)?;
        }
        let labels = CaptureLabelState::from_annotations(entries.iter().map(String::as_str));
        if entries.len() + labels.active().len() > PEVCAP_MAX_ANNOTATIONS {
            return Err(CaptureAnnotationCapacityReached);
        }
        self.entries = entries;
        self.labels = labels;
        Ok(())
    }

    /// Admits both boundaries of an exclusive replacement together.
    ///
    /// # Errors
    /// Returns a capacity error if the transition and its eventual stop cannot fit.
    pub fn start(
        &mut self,
        label: CaptureSessionLabel,
    ) -> Result<(), CaptureAnnotationCapacityReached> {
        let boundaries = self.labels.clone().start(label);
        self.append_boundaries(boundaries)
    }

    /// Stops an interval using its reserved slot; repeated stops are no-ops.
    ///
    /// # Errors
    /// Uses bounded batch admission. Every admitted open interval has a reserved
    /// stop slot, so stopping it cannot exhaust capacity.
    pub fn stop(
        &mut self,
        label: CaptureSessionLabel,
    ) -> Result<(), CaptureAnnotationCapacityReached> {
        let boundary = self.labels.clone().stop(label);
        self.append_boundaries(boundary)
    }

    fn append_boundaries(
        &mut self,
        boundaries: impl IntoIterator<Item = CaptureLabelTransition>,
    ) -> Result<(), CaptureAnnotationCapacityReached> {
        self.try_append(boundaries.into_iter().map(|boundary| {
            format!(
                "{PEVCAP_CAPTURE_LABEL_ANNOTATION_KEY}={}",
                boundary.annotation_value()
            )
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_exclusive_replacement_preserves_the_original_interval() {
        let mut annotations = CaptureAnnotations::default();
        annotations
            .try_append((0..PEVCAP_MAX_ANNOTATIONS - 2).map(|i| format!("note={i}")))
            .unwrap();
        annotations.start(CaptureSessionLabel::LowBeamOn).unwrap();
        let original = annotations.entries().to_vec();
        assert_eq!(
            annotations.start(CaptureSessionLabel::LowBeamOff),
            Err(CaptureAnnotationCapacityReached)
        );
        assert_eq!(annotations.entries(), original);
        assert_eq!(
            annotations.active_labels(),
            [CaptureSessionLabel::LowBeamOn]
        );
        annotations.stop(CaptureSessionLabel::LowBeamOn).unwrap();
        assert!(annotations.active_labels().is_empty());
        assert_eq!(
            annotations.entries().last().unwrap(),
            "capture_label=low_beam_on_stop"
        );
        annotations.stop(CaptureSessionLabel::LowBeamOn).unwrap();
        assert_eq!(annotations.entries().len(), PEVCAP_MAX_ANNOTATIONS);
    }
}
