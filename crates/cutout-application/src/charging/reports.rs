use std::{future::Future, pin::Pin};

use uuid::Uuid;

/// Immutable report operation prepared before HTTP serialization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChargingContentReport {
    spot_id: Uuid,
    operation_id: Uuid,
    message: Option<String>,
}

impl ChargingContentReport {
    /// Validates and normalizes report details while assigning an idempotency identity.
    ///
    /// # Errors
    /// Returns `InvalidMetadata` when the message exceeds its bound or contains disallowed control
    /// characters.
    pub fn new(spot_id: Uuid, operation_id: Uuid, message: &str) -> Result<Self, ReportInputError> {
        let message = message.trim();
        if message.len() > 1_000
            || message
                .chars()
                .any(|c| c.is_control() && !['\n', '\r', '\t'].contains(&c))
        {
            return Err(ReportInputError::InvalidMetadata);
        }
        Ok(Self {
            spot_id,
            operation_id,
            message: (!message.is_empty()).then(|| message.to_owned()),
        })
    }

    /// Published spot reported by this operation.
    #[must_use]
    pub const fn spot_id(&self) -> Uuid {
        self.spot_id
    }
    /// Stable identity reused for retries.
    #[must_use]
    pub const fn operation_id(&self) -> Uuid {
        self.operation_id
    }
    /// Normalized optional message.
    #[must_use]
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

/// Report input rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportInputError {
    InvalidMetadata,
}

/// Validated identifiers returned by the moderation service.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChargingReportReceipt {
    /// Report operation identifier echoed by the service.
    pub operation_id: Uuid,
    /// Server-side report identifier.
    pub report_id: Uuid,
    /// Revision at which this report was applied.
    pub applied_revision: u64,
    /// Current server revision.
    pub revision: u64,
}

/// Immutable wire-neutral command submitted by the report workflow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChargingReportCommand {
    pub spot_id: Uuid,
    pub operation_id: Uuid,
    pub reason: &'static str,
    pub message: Option<String>,
}

/// Remote report failure translated from the transport's status codes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportRemoteError {
    Unauthorized,
    Conflict,
    Rejected,
    Throttled,
    Network,
}

/// Effect port used by the portable report submission operation.
pub trait ChargingReportTransport {
    fn submit<'a>(
        &'a self,
        token: &'a str,
        command: ChargingReportCommand,
    ) -> Pin<Box<dyn Future<Output = Result<ChargingReportReceipt, ReportRemoteError>> + Send + 'a>>;
}

/// Report submission failure after applying token and receipt policy.
#[derive(Debug, thiserror::Error)]
pub enum ReportSubmitError {
    #[error("invalid account token")]
    Unauthorized,
    #[error("invalid report receipt")]
    InvalidReceipt,
    #[error("report transport failed: {0:?}")]
    Remote(ReportRemoteError),
}

/// Submits a prepared report with the exact operation identity reused across caller retries.
///
/// # Errors
/// Returns an error for a malformed token, remote failure, or receipt that does not match the
/// prepared operation or represent an applied revision.
pub async fn submit_charging_report<T: ChargingReportTransport>(
    report: &ChargingContentReport,
    token: &str,
    transport: &T,
) -> Result<ChargingReportReceipt, ReportSubmitError> {
    if !charging_account_token_is_well_formed(token) {
        return Err(ReportSubmitError::Unauthorized);
    }
    let command = ChargingReportCommand {
        spot_id: report.spot_id(),
        operation_id: report.operation_id(),
        reason: "abusive",
        message: report.message().map(str::to_owned),
    };
    let receipt = transport
        .submit(token, command)
        .await
        .map_err(ReportSubmitError::Remote)?;
    if !charging_report_receipt_is_valid(report.operation_id(), receipt) {
        return Err(ReportSubmitError::InvalidReceipt);
    }
    Ok(receipt)
}

/// Checks the bearer-token shape accepted by the service boundary.
#[must_use]
pub fn charging_account_token_is_well_formed(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Confirms a response belongs to the operation and represents an applied revision.
#[must_use]
pub fn charging_report_receipt_is_valid(
    operation_id: Uuid,
    receipt: ChargingReportReceipt,
) -> bool {
    receipt.operation_id == operation_id
        && receipt.applied_revision > 0
        && receipt.revision >= receipt.applied_revision
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_text_normalization_and_receipt_validation_are_portable_policy() {
        let operation = Uuid::from_u128(1);
        let report =
            ChargingContentReport::new(Uuid::from_u128(2), operation, "  note\n  ").unwrap();
        assert_eq!(report.message(), Some("note"));
        assert!(ChargingContentReport::new(Uuid::from_u128(2), operation, "x\0y").is_err());
        assert!(charging_report_receipt_is_valid(
            operation,
            ChargingReportReceipt {
                operation_id: operation,
                report_id: Uuid::from_u128(3),
                applied_revision: 2,
                revision: 2,
            }
        ));
        assert!(!charging_report_receipt_is_valid(
            operation,
            ChargingReportReceipt {
                operation_id: Uuid::from_u128(4),
                report_id: Uuid::from_u128(3),
                applied_revision: 2,
                revision: 2,
            }
        ));
    }
}
