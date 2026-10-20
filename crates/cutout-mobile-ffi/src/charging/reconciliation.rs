//! Native binding adapter for the shared charging reconciliation service.

use super::dto::MobileChargingSpotDto;
use crate::persistence::ChargingStatus;
use cutout_application::charging::reconciliation::{ChargingObservation, ChargingReconciliation};

impl ChargingObservation for MobileChargingSpotDto {
    fn id(&self) -> &str {
        &self.id
    }
    fn revision(&self) -> u64 {
        self.local_revision
    }
    fn status(&self) -> ChargingStatus {
        self.status.into()
    }
}

/// Owns the revision and withdrawal rules for the charging map projection.
#[derive(Debug, Default, uniffi::Object)]
pub struct MobileChargingReconciliation {
    inner: ChargingReconciliation<MobileChargingSpotDto>,
}

#[uniffi::export]
impl MobileChargingReconciliation {
    /// Creates an empty charging projection.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts an observation and returns its ordering ticket.
    pub fn begin_observation(&self) -> u64 {
        self.inner.begin_observation()
    }

    /// Records a successful withdrawal and prevents older observations restoring it.
    pub fn record_withdrawal(&self, id: String, revision: u64) {
        self.inner.record_withdrawal(id, revision);
    }

    /// Reconciles a page against newer local observations and returns page membership.
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI exports own strings")]
    pub fn reconcile_page(
        &self,
        request_ticket: u64,
        append: bool,
        current_page: Vec<MobileChargingSpotDto>,
        selected_spot_id: Option<String>,
        incoming: Vec<MobileChargingSpotDto>,
    ) -> Vec<MobileChargingSpotDto> {
        self.inner.reconcile_page(
            request_ticket,
            append,
            current_page,
            selected_spot_id.as_deref(),
            incoming,
        )
    }

    /// Accepts a readback only when it is current for this spot.
    pub fn reconcile_readback(
        &self,
        request_ticket: u64,
        spot: MobileChargingSpotDto,
    ) -> Option<MobileChargingSpotDto> {
        self.inner.reconcile_readback(request_ticket, spot)
    }

    /// Reconciles the authoritative local row after a conflict resolution.
    pub fn reconcile_resolution(
        &self,
        request_ticket: u64,
        spot: MobileChargingSpotDto,
    ) -> Option<MobileChargingSpotDto> {
        self.inner.reconcile_resolution(request_ticket, spot)
    }

    /// Reports whether a successful withdrawal still fences actions on this spot.
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI exports own strings")]
    pub fn is_withdrawn(&self, id: String) -> bool {
        self.inner.is_withdrawn(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::super::dto::MobileChargingStatusDto;
    use super::*;
    fn spot(id: &str, revision: u64, status: MobileChargingStatusDto) -> MobileChargingSpotDto {
        MobileChargingSpotDto {
            id: id.into(),
            input: None,
            owned: true,
            status,
            local_revision: revision,
            remote_id: None,
            remote_revision: 0,
            change_sequence: 0,
            verified_at_ms: None,
            vote: None,
            vote_revision: 0,
            votes_up: 0,
            votes_down: 0,
            pending_operations: 0,
            pending_vote: false,
        }
    }

    #[test]
    fn adapter_preserves_newer_payload_and_withdrawal_fences() {
        let adapter = MobileChargingReconciliation::new();
        let page = adapter.begin_observation();
        let readback = adapter.begin_observation();
        let mut latest = spot("a", 7, MobileChargingStatusDto::Published);
        latest.votes_up = 12;
        latest.pending_operations = 3;
        assert_eq!(
            adapter.reconcile_readback(readback, latest.clone()),
            Some(latest.clone())
        );
        assert_eq!(
            adapter.reconcile_page(
                page,
                false,
                vec![],
                None,
                vec![spot("a", 6, MobileChargingStatusDto::Published)]
            ),
            vec![latest]
        );
        adapter.record_withdrawal("a".into(), 7);
        assert!(adapter.is_withdrawn("a".into()));
        assert!(
            adapter
                .reconcile_readback(page, spot("a", 8, MobileChargingStatusDto::Published))
                .is_none()
        );
        let restored = spot("a", 8, MobileChargingStatusDto::Published);
        assert_eq!(
            adapter.reconcile_resolution(adapter.begin_observation(), restored.clone()),
            Some(restored)
        );
        assert!(!adapter.is_withdrawn("a".into()));
    }
}
