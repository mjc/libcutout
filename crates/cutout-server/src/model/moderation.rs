use super::{Error, OperationId, SpotInput, SpotState, ValidatedSpotInput, id, optional_text};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReportReason {
    WrongLocation,
    Unavailable,
    WrongAccess,
    WrongPrice,
    WrongVoltage,
    WrongConnector,
    Duplicate,
    Abusive,
    Other,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReportRequest {
    pub operation_id: OperationId,
    pub reason: ReportReason,
    pub message: Option<String>,
}

/// Report text is normalized and an `other` reason always has an explanation.
#[derive(Clone, Debug, Serialize)]
#[serde(transparent)]
pub(crate) struct ValidatedReportRequest(ReportRequest);

impl ValidatedReportRequest {
    pub(crate) fn as_request(&self) -> &ReportRequest {
        &self.0
    }
}

impl ReportRequest {
    pub(crate) fn into_validated(mut self) -> Result<ValidatedReportRequest, Error> {
        self.message = optional_text(self.message, 1_000)?;
        if let ReportReason::Other = self.reason
            && self.message.is_none()
        {
            return Err(Error::Invalid);
        }
        Ok(ValidatedReportRequest(self))
    }
}

/// A trimmed, nonempty operator explanation within the audit text limit.
#[derive(Clone, Debug, Serialize)]
#[serde(transparent)]
pub(crate) struct ModerationNote(String);

impl ModerationNote {
    fn parse(value: String) -> Result<Self, Error> {
        optional_text(Some(value), 500)?
            .map(Self)
            .ok_or(Error::Invalid)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ModerationAction<Input = SpotInput> {
    Hide,
    Restore,
    Remove,
    Correct { spot: Input },
    Duplicate { canonical_id: String },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModerationRequest<Input = SpotInput, Note = String> {
    pub operation_id: OperationId,
    pub expected_revision: u64,
    pub note: Note,
    pub action: ModerationAction<Input>,
}

impl ModerationRequest {
    pub(crate) fn into_validated(
        self,
    ) -> Result<ModerationRequest<ValidatedSpotInput, ModerationNote>, Error> {
        let action = match self.action {
            ModerationAction::Correct { spot } => ModerationAction::Correct {
                spot: spot.validate()?,
            },
            ModerationAction::Duplicate { canonical_id } => ModerationAction::Duplicate {
                canonical_id: id(&canonical_id)?,
            },
            ModerationAction::Hide => ModerationAction::Hide,
            ModerationAction::Restore => ModerationAction::Restore,
            ModerationAction::Remove => ModerationAction::Remove,
        };
        Ok(ModerationRequest {
            operation_id: self.operation_id,
            expected_revision: self.expected_revision,
            note: ModerationNote::parse(self.note)?,
            action,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResolveRequest<Note = String> {
    pub operation_id: OperationId,
    pub expected_revision: u64,
    pub note: Note,
}

impl ResolveRequest {
    pub(crate) fn into_validated(self) -> Result<ResolveRequest<ModerationNote>, Error> {
        Ok(ResolveRequest {
            operation_id: self.operation_id,
            expected_revision: self.expected_revision,
            note: ModerationNote::parse(self.note)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReportStatus {
    Open,
    Resolved,
}

impl ReportStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Resolved => "resolved",
        }
    }
}

impl rusqlite::types::FromSql for ReportStatus {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match value.as_str()? {
            "open" => Ok(Self::Open),
            "resolved" => Ok(Self::Resolved),
            _ => Err(rusqlite::types::FromSqlError::Other(Box::new(
                Error::Unavailable,
            ))),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ReportRecord {
    pub id: String,
    pub spot_id: String,
    pub reporter_id: String,
    pub revision: u64,
    pub reason: ReportReason,
    pub message: Option<String>,
    pub status: ReportStatus,
    pub created_at_ms: u64,
    pub resolved_at_ms: Option<u64>,
    pub resolution_note: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ReportReceipt {
    pub operation_id: OperationId,
    pub id: String,
    pub applied_revision: u64,
    pub revision: u64,
    pub status: ReportStatus,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ReportsPage {
    pub items: Vec<ReportRecord>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ModeratorSpot {
    pub state: SpotState,
    pub owner_id: String,
    pub input: Option<SpotInput>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AuditRecord {
    pub sequence: u64,
    pub actor_id: String,
    pub target_id: String,
    pub action: String,
    pub note: String,
    pub canonical_id: Option<String>,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AuditPage {
    pub items: Vec<AuditRecord>,
    pub next_cursor: u64,
    pub has_more: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_require_an_explanation_for_other_and_preserve_normalized_request_bytes() {
        let operation = "00000000-0000-4000-8000-0000000000ab";
        for message in [
            None,
            Some(" \t\n".to_owned()),
            Some("a".repeat(1_001)),
            Some("é".repeat(501)),
            Some("bad\0text".to_owned()),
        ] {
            let request = ReportRequest {
                operation_id: OperationId::parse(operation).unwrap(),
                reason: ReportReason::Other,
                message,
            };
            assert_eq!(request.into_validated().unwrap_err(), Error::Invalid);
        }
        let request = ReportRequest {
            operation_id: OperationId::parse(operation).unwrap(),
            reason: ReportReason::Other,
            message: Some("  Outlet is broken. \n".into()),
        }
        .into_validated()
        .unwrap();
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"operation_id":"00000000-0000-4000-8000-0000000000ab","reason":"other","message":"Outlet is broken."}"#
        );
        let request = ReportRequest {
            operation_id: OperationId::parse(operation).unwrap(),
            reason: ReportReason::Unavailable,
            message: Some(" \t\n".into()),
        }
        .into_validated()
        .unwrap();
        assert!(request.as_request().message.is_none());
        assert!(
            ReportRequest {
                operation_id: OperationId::parse(operation).unwrap(),
                reason: ReportReason::Other,
                message: Some("é".repeat(500)),
            }
            .into_validated()
            .is_ok()
        );
    }

    #[test]
    fn operator_notes_are_required_bounded_and_preserve_normalized_request_bytes() {
        let operation = "00000000-0000-4000-8000-0000000000ab";
        for note in [
            " \t\n".to_owned(),
            "a".repeat(501),
            "é".repeat(251),
            "bad\0text".to_owned(),
        ] {
            let moderation = ModerationRequest {
                operation_id: OperationId::parse(operation).unwrap(),
                expected_revision: 1,
                note: note.clone(),
                action: ModerationAction::Hide,
            };
            assert_eq!(moderation.into_validated().unwrap_err(), Error::Invalid);
            let resolution = ResolveRequest {
                operation_id: OperationId::parse(operation).unwrap(),
                expected_revision: 1,
                note,
            };
            assert_eq!(resolution.into_validated().unwrap_err(), Error::Invalid);
        }
        assert!(ModerationNote::parse("é".repeat(250)).is_ok());
        let moderation = ModerationRequest {
            operation_id: OperationId::parse(operation).unwrap(),
            expected_revision: 1,
            note: "  Same outlet. \n".into(),
            action: ModerationAction::Duplicate {
                canonical_id: "00000000-0000-4000-8000-0000000000CD".into(),
            },
        }
        .into_validated()
        .unwrap();
        assert_eq!(
            serde_json::to_string(&moderation).unwrap(),
            r#"{"operation_id":"00000000-0000-4000-8000-0000000000ab","expected_revision":1,"note":"Same outlet.","action":{"type":"duplicate","canonical_id":"00000000-0000-4000-8000-0000000000cd"}}"#
        );
        let resolution = ResolveRequest {
            operation_id: OperationId::parse(operation).unwrap(),
            expected_revision: 1,
            note: "  Fixed. \n".into(),
        }
        .into_validated()
        .unwrap();
        assert_eq!(
            serde_json::to_string(&resolution).unwrap(),
            r#"{"operation_id":"00000000-0000-4000-8000-0000000000ab","expected_revision":1,"note":"Fixed."}"#
        );
    }
}
