//! Terminal capture receipt classification, shared by native adapters.

use crate::{
    MobileCaptureCompletionDto, MobileCaptureFinishOutcomeDto, MobileCaptureWriteOutcomeDto,
    MobileSavedCaptureArtifactDto,
};

/// Publication authorized by an owned terminal writer receipt.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureCompletionDecisionDto {
    /// The receipt is premature, unowned, or already consumed.
    Rejected,
    /// A complete file is available; index publication failure retains that file.
    PublishArtifact {
        artifact: MobileSavedCaptureArtifactDto,
        database_publication_failed: bool,
    },
    /// SQLite established a terminal capture, including explicitly incomplete captures.
    PublishDatabase {
        outcome: MobileCaptureFinishOutcomeDto,
        database_publication_failed: bool,
    },
    /// No usable terminal capture receipt was established.
    PublishFailure,
}

/// Interprets durability independently of optional saved-history publication.
#[must_use]
#[uniffi::export]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI record inputs are owned"
)]
pub fn capture_completion_succeeded(
    completion: MobileCaptureCompletionDto,
    prior_write_outcome: MobileCaptureWriteOutcomeDto,
) -> bool {
    completion_succeeded(&completion, prior_write_outcome)
}

pub(crate) fn completion_succeeded(
    completion: &MobileCaptureCompletionDto,
    prior_write_outcome: MobileCaptureWriteOutcomeDto,
) -> bool {
    match completion.finish {
        MobileCaptureFinishOutcomeDto::ArtifactAvailable { .. } => {
            prior_write_outcome == MobileCaptureWriteOutcomeDto::Accepted
        }
        MobileCaptureFinishOutcomeDto::DatabaseFinished { .. } => true,
        MobileCaptureFinishOutcomeDto::NotStarted
        | MobileCaptureFinishOutcomeDto::Finalizing
        | MobileCaptureFinishOutcomeDto::Failed { .. } => false,
    }
}

pub(crate) fn publication_for_completion(
    completion: MobileCaptureCompletionDto,
    prior_write_outcome: MobileCaptureWriteOutcomeDto,
) -> MobileCaptureCompletionDecisionDto {
    if !completion_succeeded(&completion, prior_write_outcome) {
        return MobileCaptureCompletionDecisionDto::PublishFailure;
    }
    let database_publication_failed = completion.database_publication_succeeded == Some(false);
    match completion.finish {
        MobileCaptureFinishOutcomeDto::ArtifactAvailable { artifact } => {
            MobileCaptureCompletionDecisionDto::PublishArtifact {
                artifact,
                database_publication_failed,
            }
        }
        outcome @ MobileCaptureFinishOutcomeDto::DatabaseFinished { .. } => {
            MobileCaptureCompletionDecisionDto::PublishDatabase {
                outcome,
                database_publication_failed,
            }
        }
        MobileCaptureFinishOutcomeDto::NotStarted
        | MobileCaptureFinishOutcomeDto::Finalizing
        | MobileCaptureFinishOutcomeDto::Failed { .. } => {
            MobileCaptureCompletionDecisionDto::PublishFailure
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CutoutSessionStateHandle, MobileCaptureArtifactIdDto, MobileCaptureIntegrityDto,
        MobileCaptureJsonlExportDto, MobileCaptureOriginDto, MobileCaptureStageDto,
        MobileCaptureWriterStatusDto,
    };

    fn file_completion(published: Option<bool>) -> MobileCaptureCompletionDto {
        MobileCaptureCompletionDto {
            finish: MobileCaptureFinishOutcomeDto::ArtifactAvailable {
                artifact: MobileSavedCaptureArtifactDto {
                    id: MobileCaptureArtifactIdDto {
                        value: "capture-a".into(),
                    },
                    path: "/captures/capture-a.jsonl".into(),
                    status: MobileCaptureWriterStatusDto::default(),
                },
            },
            database_publication_succeeded: published,
        }
    }

    #[test]
    fn optional_publication_failure_preserves_the_admitted_file() {
        let completion = file_completion(Some(false));
        assert!(capture_completion_succeeded(
            completion.clone(),
            MobileCaptureWriteOutcomeDto::Accepted
        ));
        let MobileCaptureFinishOutcomeDto::ArtifactAvailable { artifact } =
            completion.finish.clone()
        else {
            panic!("fixture must contain an artifact")
        };
        assert_eq!(
            publication_for_completion(completion, MobileCaptureWriteOutcomeDto::Accepted),
            MobileCaptureCompletionDecisionDto::PublishArtifact {
                artifact,
                database_publication_failed: true,
            }
        );
    }

    #[test]
    fn file_completion_cannot_hide_prior_evidence_failure() {
        let completion = file_completion(None);
        assert!(!capture_completion_succeeded(
            completion.clone(),
            MobileCaptureWriteOutcomeDto::Failed
        ));
        assert_eq!(
            publication_for_completion(completion, MobileCaptureWriteOutcomeDto::Failed),
            MobileCaptureCompletionDecisionDto::PublishFailure
        );
    }

    #[test]
    fn durable_database_receipt_preserves_incomplete_evidence_and_export_failure() {
        let outcome = MobileCaptureFinishOutcomeDto::DatabaseFinished {
            live_capture_id: "capture-a".into(),
            integrity: MobileCaptureIntegrityDto::Incomplete {
                dropped_messages: 65,
            },
            jsonl_export: MobileCaptureJsonlExportDto::Failed {
                message: "export failed".into(),
            },
            status: MobileCaptureWriterStatusDto {
                dropped_messages: 65,
                ..Default::default()
            },
        };
        let completion = MobileCaptureCompletionDto {
            finish: outcome.clone(),
            database_publication_succeeded: None,
        };
        assert!(capture_completion_succeeded(
            completion.clone(),
            MobileCaptureWriteOutcomeDto::Failed
        ));
        assert_eq!(
            publication_for_completion(completion, MobileCaptureWriteOutcomeDto::Failed),
            MobileCaptureCompletionDecisionDto::PublishDatabase {
                outcome,
                database_publication_failed: false,
            }
        );
    }

    #[test]
    fn failed_and_nonterminal_receipts_never_report_success() {
        for finish in [
            MobileCaptureFinishOutcomeDto::NotStarted,
            MobileCaptureFinishOutcomeDto::Finalizing,
            MobileCaptureFinishOutcomeDto::Failed {
                message: "writer failed".into(),
            },
        ] {
            let completion = MobileCaptureCompletionDto {
                finish,
                database_publication_succeeded: Some(true),
            };
            assert!(!capture_completion_succeeded(
                completion.clone(),
                MobileCaptureWriteOutcomeDto::Accepted
            ));
            assert_eq!(
                publication_for_completion(completion, MobileCaptureWriteOutcomeDto::Accepted),
                MobileCaptureCompletionDecisionDto::PublishFailure
            );
        }
    }

    #[test]
    fn historical_completion_publishes_once_without_finishing_the_new_writer() {
        let session = CutoutSessionStateHandle::new();
        let first = session
            .begin_capture(MobileCaptureOriginDto::Automatic)
            .unwrap();
        assert!(session.capture_writer_started(first));
        assert!(session.retire_capture_writer(first));
        let second = session
            .begin_capture(MobileCaptureOriginDto::Manual)
            .unwrap();
        assert!(session.capture_writer_started(second));
        let completion = file_completion(None);
        assert_eq!(
            session.complete_capture_writer(
                first,
                completion.clone(),
                MobileCaptureWriteOutcomeDto::Accepted
            ),
            publication_for_completion(completion.clone(), MobileCaptureWriteOutcomeDto::Accepted)
        );
        assert_eq!(
            session.complete_capture_writer(
                first,
                completion,
                MobileCaptureWriteOutcomeDto::Accepted
            ),
            MobileCaptureCompletionDecisionDto::Rejected
        );
        let current = session.capture_lifecycle_snapshot().attempt.unwrap();
        assert_eq!(current.generation, second);
        assert_eq!(current.stage, MobileCaptureStageDto::Recording);
    }

    #[test]
    fn premature_receipt_does_not_consume_a_future_terminal_receipt() {
        let session = CutoutSessionStateHandle::new();
        let generation = session
            .begin_capture(MobileCaptureOriginDto::Manual)
            .unwrap();
        assert!(session.capture_writer_started(generation));
        let completion = file_completion(None);
        assert_eq!(
            session.complete_capture_writer(
                generation,
                completion.clone(),
                MobileCaptureWriteOutcomeDto::Accepted
            ),
            MobileCaptureCompletionDecisionDto::Rejected
        );
        assert!(session.retire_capture_writer(generation));
        for finish in [
            MobileCaptureFinishOutcomeDto::NotStarted,
            MobileCaptureFinishOutcomeDto::Finalizing,
        ] {
            assert_eq!(
                session.complete_capture_writer(
                    generation,
                    MobileCaptureCompletionDto {
                        finish,
                        database_publication_succeeded: None,
                    },
                    MobileCaptureWriteOutcomeDto::Accepted,
                ),
                MobileCaptureCompletionDecisionDto::Rejected
            );
        }
        assert_eq!(
            session.complete_capture_writer(
                generation,
                completion.clone(),
                MobileCaptureWriteOutcomeDto::Accepted
            ),
            publication_for_completion(completion, MobileCaptureWriteOutcomeDto::Accepted)
        );
        assert_eq!(
            session.capture_lifecycle_snapshot().attempt.unwrap().stage,
            MobileCaptureStageDto::Saved
        );
    }
}
