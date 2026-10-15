//! Revision-aware projection of charging pages and local reads.

use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, PoisonError},
};

use crate::persistence::{ChargingRevision, ChargingStatus};

use super::dto::{MobileChargingSpotDto, MobileChargingStatusDto};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ObservationTicket(u64);

#[derive(Clone, Copy, Debug)]
struct Withdrawal {
    ticket: ObservationTicket,
    revision: ChargingRevision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ObservationSource {
    Page,
    Readback,
}

#[derive(Debug)]
struct LatestObservation {
    ticket: ObservationTicket,
    source: ObservationSource,
    spot: MobileChargingSpotDto,
}

#[derive(Debug, Default)]
struct ProjectionState {
    next_ticket: u64,
    latest: HashMap<String, LatestObservation>,
    withdrawals: HashMap<String, Withdrawal>,
}

impl ProjectionState {
    fn next_ticket(&mut self) -> ObservationTicket {
        self.next_ticket = self.next_ticket.saturating_add(1);
        ObservationTicket(self.next_ticket)
    }

    fn accept_observation(
        &mut self,
        ticket: ObservationTicket,
        source: ObservationSource,
        spot: MobileChargingSpotDto,
    ) -> Option<MobileChargingSpotDto> {
        let revision = revision(&spot)?;
        if let Some(withdrawal) = self.withdrawals.get(&spot.id).copied() {
            if ticket < withdrawal.ticket
                || revision <= withdrawal.revision
                || !is_published(spot.status)
            {
                return None;
            }
            self.withdrawals.remove(&spot.id);
        }
        Some(self.record_latest(ticket, source, spot))
    }

    fn accept_resolution(
        &mut self,
        ticket: ObservationTicket,
        spot: MobileChargingSpotDto,
    ) -> Option<MobileChargingSpotDto> {
        let accepted_revision = revision(&spot)?;
        if let Some(withdrawal) = self.withdrawals.get(&spot.id).copied() {
            if ticket < withdrawal.ticket {
                return None;
            }
            if is_published(spot.status) && accepted_revision > withdrawal.revision {
                self.withdrawals.remove(&spot.id);
            }
        }
        Some(self.record_latest(ticket, ObservationSource::Readback, spot))
    }

    fn record_latest(
        &mut self,
        ticket: ObservationTicket,
        source: ObservationSource,
        spot: MobileChargingSpotDto,
    ) -> MobileChargingSpotDto {
        let Some(incoming_revision) = revision(&spot) else {
            return spot;
        };
        if let Some(known) = self.latest.get(&spot.id)
            && let Some(known_revision) = revision(&known.spot)
            && (known_revision > incoming_revision
                || (known_revision == incoming_revision && known.ticket > ticket))
        {
            return known.spot.clone();
        }
        self.latest.insert(
            spot.id.clone(),
            LatestObservation {
                ticket,
                source,
                spot: spot.clone(),
            },
        );
        spot
    }
}

fn revision(spot: &MobileChargingSpotDto) -> Option<ChargingRevision> {
    ChargingRevision::new(spot.local_revision).ok()
}

fn is_published(status: MobileChargingStatusDto) -> bool {
    let status: ChargingStatus = status.into();
    match status {
        ChargingStatus::Published => true,
        ChargingStatus::Queued
        | ChargingStatus::Hidden
        | ChargingStatus::Withdrawn
        | ChargingStatus::Removed => false,
    }
}

fn is_visible(status: MobileChargingStatusDto) -> bool {
    let status: ChargingStatus = status.into();
    status.visible()
}

/// Owns the revision and withdrawal rules for the charging map projection.
#[derive(Debug, Default, uniffi::Object)]
pub struct MobileChargingReconciliation {
    state: Mutex<ProjectionState>,
}

#[uniffi::export]
impl MobileChargingReconciliation {
    /// Creates an empty charging projection.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Mutex::default(),
        }
    }

    /// Starts an observation and returns its ordering ticket.
    pub fn begin_observation(&self) -> u64 {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .next_ticket()
            .0
    }

    /// Records a successful withdrawal and prevents older observations restoring it.
    pub fn record_withdrawal(&self, id: String, revision: u64) {
        let Some(revision) = ChargingRevision::new(revision).ok() else {
            return;
        };
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let ticket = state.next_ticket();
        state
            .withdrawals
            .insert(id, Withdrawal { ticket, revision });
    }

    /// Reconciles a page against newer local observations and returns page membership.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns optional strings at this boundary"
    )]
    pub fn reconcile_page(
        &self,
        request_ticket: u64,
        append: bool,
        current_page: Vec<MobileChargingSpotDto>,
        selected_spot_id: Option<String>,
        incoming: Vec<MobileChargingSpotDto>,
    ) -> Vec<MobileChargingSpotDto> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let ticket = ObservationTicket(request_ticket);
        let page_ids: HashSet<String> = if append {
            current_page.iter().map(|spot| spot.id.clone()).collect()
        } else {
            HashSet::new()
        };
        let withdrawal_ids: HashSet<String> = state.withdrawals.keys().cloned().collect();
        state.latest.retain(|id, observation| {
            observation.source == ObservationSource::Readback
                || Some(id) == selected_spot_id.as_ref()
                || page_ids.contains(id)
                || withdrawal_ids.contains(id)
                || observation.ticket > ticket
        });
        let mut membership = if append {
            current_page
                .into_iter()
                .filter(|spot| is_visible(spot.status))
                .collect()
        } else {
            Vec::new()
        };
        for spot in incoming {
            let Some(spot) = state.accept_observation(ticket, ObservationSource::Page, spot) else {
                continue;
            };
            if !is_visible(spot.status) {
                membership.retain(|existing| existing.id != spot.id);
                continue;
            }
            if let Some(existing) = membership
                .iter_mut()
                .find(|existing| existing.id == spot.id)
                && let (Some(existing_revision), Some(spot_revision)) =
                    (revision(existing), revision(&spot))
                && spot_revision >= existing_revision
            {
                *existing = spot;
            } else if !membership.iter().any(|existing| existing.id == spot.id) {
                membership.push(spot);
            }
        }
        membership
    }

    /// Accepts a readback only when it is current for this spot.
    pub fn reconcile_readback(
        &self,
        request_ticket: u64,
        spot: MobileChargingSpotDto,
    ) -> Option<MobileChargingSpotDto> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .accept_observation(
                ObservationTicket(request_ticket),
                ObservationSource::Readback,
                spot,
            )
    }

    /// Reconciles the authoritative local row after a conflict resolution.
    pub fn reconcile_resolution(
        &self,
        request_ticket: u64,
        spot: MobileChargingSpotDto,
    ) -> Option<MobileChargingSpotDto> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .accept_resolution(ObservationTicket(request_ticket), spot)
    }

    /// Reports whether a successful withdrawal still fences actions on this spot.
    // UniFFI exports owned strings, so this FFI argument cannot borrow `str`.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI requires an owned String argument"
    )]
    pub fn is_withdrawn(&self, id: String) -> bool {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .withdrawals
            .contains_key(&id)
    }
}

#[cfg(test)]
mod tests {
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
    fn delayed_page_preserves_newer_vote_readback() {
        let state = MobileChargingReconciliation::new();
        let page = state.begin_observation();
        let read = state.begin_observation();
        state.reconcile_readback(read, spot("a", 4, MobileChargingStatusDto::Published));
        let projected = state.reconcile_page(
            page,
            false,
            vec![],
            None,
            vec![spot("a", 3, MobileChargingStatusDto::Published)],
        );
        assert_eq!(projected[0].local_revision, 4);
    }

    #[test]
    fn older_readback_does_not_downgrade_projection() {
        let state = MobileChargingReconciliation::new();
        let old = state.begin_observation();
        let new = state.begin_observation();
        state.reconcile_readback(new, spot("a", 8, MobileChargingStatusDto::Published));
        let result =
            state.reconcile_readback(old, spot("a", 7, MobileChargingStatusDto::Published));
        assert_eq!(result.map(|spot| spot.local_revision), Some(8));
    }

    #[test]
    fn withdrawal_blocks_old_pages_and_newer_publication_restores() {
        let state = MobileChargingReconciliation::new();
        let old = state.begin_observation();
        state.record_withdrawal("a".into(), 3);
        assert_eq!(
            state.reconcile_page(
                old,
                false,
                vec![],
                None,
                vec![spot("a", 3, MobileChargingStatusDto::Published),]
            ),
            vec![]
        );
        let restoration = state.begin_observation();
        let restored = state.reconcile_resolution(
            restoration,
            spot("a", 4, MobileChargingStatusDto::Published),
        );
        assert_eq!(restored.map(|spot| spot.local_revision), Some(4));
        assert!(
            !state
                .reconcile_page(
                    old,
                    false,
                    vec![],
                    None,
                    vec![spot("a", 3, MobileChargingStatusDto::Published),]
                )
                .iter()
                .any(|spot| spot.local_revision < 4)
        );
    }

    #[test]
    fn resolution_returns_queued_and_hidden_rows_without_clearing_withdrawal_fence() {
        let state = MobileChargingReconciliation::new();
        state.record_withdrawal("a".into(), 1);
        let queued_ticket = state.begin_observation();
        let queued = state
            .reconcile_resolution(queued_ticket, spot("a", 2, MobileChargingStatusDto::Queued));
        assert_eq!(
            queued.map(|spot| spot.status),
            Some(MobileChargingStatusDto::Queued)
        );
        assert!(state.is_withdrawn("a".into()));

        let hidden_ticket = state.begin_observation();
        let hidden = state
            .reconcile_resolution(hidden_ticket, spot("a", 3, MobileChargingStatusDto::Hidden));
        assert_eq!(
            hidden.map(|spot| spot.status),
            Some(MobileChargingStatusDto::Hidden)
        );
        assert!(state.is_withdrawn("a".into()));
    }

    #[test]
    fn filtered_membership_does_not_add_retained_non_page_rows() {
        let state = MobileChargingReconciliation::new();
        let ticket = state.begin_observation();
        state.reconcile_readback(
            ticket,
            spot("detail", 1, MobileChargingStatusDto::Published),
        );
        assert_eq!(
            state.reconcile_page(ticket, false, vec![], None, vec![]),
            vec![]
        );
    }

    #[test]
    fn page_replacement_keeps_readback_for_delayed_page_after_selection_changes() {
        let state = MobileChargingReconciliation::new();
        let delayed_page = state.begin_observation();
        let vote_readback = state.begin_observation();
        state.reconcile_readback(
            vote_readback,
            spot("voted", 4, MobileChargingStatusDto::Published),
        );

        let replacement_page = state.begin_observation();
        state.reconcile_page(
            replacement_page,
            false,
            vec![],
            Some("selected".into()),
            vec![spot("selected", 1, MobileChargingStatusDto::Published)],
        );
        let delayed = state.reconcile_page(
            delayed_page,
            false,
            vec![],
            None,
            vec![spot("voted", 3, MobileChargingStatusDto::Published)],
        );

        assert_eq!(delayed[0].local_revision, 4);
    }

    #[test]
    fn delayed_page_does_not_restore_nonvisible_readbacks() {
        for status in [
            MobileChargingStatusDto::Hidden,
            MobileChargingStatusDto::Withdrawn,
            MobileChargingStatusDto::Removed,
        ] {
            let state = MobileChargingReconciliation::new();
            let delayed_page = state.begin_observation();
            let readback = state.begin_observation();
            let hidden = spot("a", 4, status);
            state.reconcile_readback(readback, hidden.clone());

            let page = state.reconcile_page(
                delayed_page,
                false,
                vec![],
                None,
                vec![spot("a", 3, MobileChargingStatusDto::Published)],
            );

            assert_eq!(
                page,
                vec![],
                "cached status {status:?} must stay out of page"
            );
            assert_eq!(
                state.reconcile_readback(state.begin_observation(), hidden.clone()),
                Some(hidden),
                "the nonvisible row remains available as selected detail"
            );
        }
    }

    #[test]
    fn newer_page_acknowledges_readback_for_later_projection_pruning() {
        let state = MobileChargingReconciliation::new();
        let readback_ticket = state.begin_observation();
        state.reconcile_readback(
            readback_ticket,
            spot("seen", 4, MobileChargingStatusDto::Published),
        );
        let page_ticket = state.begin_observation();
        state.reconcile_page(
            page_ticket,
            false,
            vec![],
            None,
            vec![spot("seen", 4, MobileChargingStatusDto::Published)],
        );
        let next_page = state.begin_observation();
        state.reconcile_page(next_page, false, vec![], None, vec![]);

        assert!(
            !state
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .latest
                .contains_key("seen")
        );
    }
}
