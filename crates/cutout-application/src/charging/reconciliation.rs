//! Revision-aware projection of charging pages and local reads.

use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, PoisonError},
};

use libcutout_persistence::{ChargingRevision, ChargingStatus};

/// A caller-owned charging row projected without depending on a UI or binding record.
///
/// The service validates revision observations before retaining them. All other
/// row data is preserved intact when a newer observation replaces an older one.
pub trait ChargingObservation: Clone {
    /// Stable identity of the charging spot.
    fn id(&self) -> &str;
    /// Revision received from storage or a remote observation.
    fn revision(&self) -> u64;
    /// Lifecycle visibility used by map membership and restoration.
    fn status(&self) -> ChargingStatus;
}

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
struct LatestObservation<T> {
    ticket: ObservationTicket,
    source: ObservationSource,
    revision: ChargingRevision,
    spot: T,
}

#[derive(Debug)]
struct ProjectionState<T> {
    next_ticket: u64,
    latest: HashMap<String, LatestObservation<T>>,
    withdrawals: HashMap<String, Withdrawal>,
}

impl<T> Default for ProjectionState<T> {
    fn default() -> Self {
        Self {
            next_ticket: 0,
            latest: HashMap::new(),
            withdrawals: HashMap::new(),
        }
    }
}

impl<T: ChargingObservation> ProjectionState<T> {
    fn next_ticket(&mut self) -> ObservationTicket {
        self.next_ticket = self.next_ticket.saturating_add(1);
        ObservationTicket(self.next_ticket)
    }

    fn accept_observation(
        &mut self,
        ticket: ObservationTicket,
        source: ObservationSource,
        spot: T,
    ) -> Option<T> {
        let revision = revision(&spot)?;
        if let Some(withdrawal) = self.withdrawals.get(spot.id()).copied() {
            if ticket < withdrawal.ticket
                || revision <= withdrawal.revision
                || !is_published(spot.status())
            {
                return None;
            }
            self.withdrawals.remove(spot.id());
        }
        Some(self.record_latest(ticket, source, revision, spot))
    }

    fn accept_resolution(&mut self, ticket: ObservationTicket, spot: T) -> Option<T> {
        let accepted_revision = revision(&spot)?;
        if let Some(withdrawal) = self.withdrawals.get(spot.id()).copied() {
            if ticket < withdrawal.ticket {
                return None;
            }
            if is_published(spot.status()) && accepted_revision > withdrawal.revision {
                self.withdrawals.remove(spot.id());
            }
        }
        Some(self.record_latest(ticket, ObservationSource::Readback, accepted_revision, spot))
    }

    fn record_latest(
        &mut self,
        ticket: ObservationTicket,
        source: ObservationSource,
        incoming_revision: ChargingRevision,
        spot: T,
    ) -> T {
        if let Some(known) = self.latest.get(spot.id())
            && (known.revision > incoming_revision
                || (known.revision == incoming_revision && known.ticket > ticket))
        {
            return known.spot.clone();
        }
        self.latest.insert(
            spot.id().to_owned(),
            LatestObservation {
                ticket,
                source,
                revision: incoming_revision,
                spot: spot.clone(),
            },
        );
        spot
    }
}

fn revision<T: ChargingObservation>(spot: &T) -> Option<ChargingRevision> {
    ChargingRevision::new(spot.revision()).ok()
}

fn is_published(status: ChargingStatus) -> bool {
    match status {
        ChargingStatus::Published => true,
        ChargingStatus::Queued
        | ChargingStatus::Hidden
        | ChargingStatus::Withdrawn
        | ChargingStatus::Removed => false,
    }
}

fn is_visible(status: ChargingStatus) -> bool {
    status.visible()
}

/// Owns the revision and withdrawal rules for the charging map projection.
#[derive(Debug)]
pub struct ChargingReconciliation<T> {
    state: Mutex<ProjectionState<T>>,
}

impl<T> Default for ChargingReconciliation<T> {
    fn default() -> Self {
        Self {
            state: Mutex::default(),
        }
    }
}

impl<T: ChargingObservation> ChargingReconciliation<T> {
    /// Creates an empty charging projection.
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
    pub fn reconcile_page(
        &self,
        request_ticket: u64,
        append: bool,
        current_page: Vec<T>,
        selected_spot_id: Option<&str>,
        incoming: Vec<T>,
    ) -> Vec<T> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let ticket = ObservationTicket(request_ticket);
        let page_ids: HashSet<String> = if append {
            current_page
                .iter()
                .map(|spot| spot.id().to_owned())
                .collect()
        } else {
            HashSet::new()
        };
        let withdrawal_ids: HashSet<String> = state.withdrawals.keys().cloned().collect();
        state.latest.retain(|id, observation| {
            observation.source == ObservationSource::Readback
                || Some(id.as_str()) == selected_spot_id
                || page_ids.contains(id)
                || withdrawal_ids.contains(id)
                || observation.ticket > ticket
        });
        let mut membership = if append {
            current_page
                .into_iter()
                .filter(|spot| is_visible(spot.status()))
                .collect()
        } else {
            Vec::new()
        };
        for spot in incoming {
            let Some(spot) = state.accept_observation(ticket, ObservationSource::Page, spot) else {
                continue;
            };
            if !is_visible(spot.status()) {
                membership.retain(|existing| existing.id() != spot.id());
                continue;
            }
            if let Some(existing) = membership
                .iter_mut()
                .find(|existing| existing.id() == spot.id())
                && let (Some(existing_revision), Some(spot_revision)) =
                    (revision(existing), revision(&spot))
                && spot_revision >= existing_revision
            {
                *existing = spot;
            } else if !membership.iter().any(|existing| existing.id() == spot.id()) {
                membership.push(spot);
            }
        }
        membership
    }

    /// Accepts a readback only when it is current for this spot.
    pub fn reconcile_readback(&self, request_ticket: u64, spot: T) -> Option<T> {
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
    pub fn reconcile_resolution(&self, request_ticket: u64, spot: T) -> Option<T> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .accept_resolution(ObservationTicket(request_ticket), spot)
    }

    /// Reports whether a successful withdrawal still fences actions on this spot.
    pub fn is_withdrawn(&self, id: &str) -> bool {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .withdrawals
            .contains_key(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct Spot {
        id: String,
        local_revision: u64,
        status: ChargingStatus,
    }
    impl ChargingObservation for Spot {
        fn id(&self) -> &str {
            &self.id
        }
        fn revision(&self) -> u64 {
            self.local_revision
        }
        fn status(&self) -> ChargingStatus {
            self.status
        }
    }
    fn spot(id: &str, revision: u64, status: ChargingStatus) -> Spot {
        Spot {
            id: id.into(),
            local_revision: revision,
            status,
        }
    }

    #[test]
    fn out_of_range_revisions_are_rejected_before_caching() {
        let state = ChargingReconciliation::<Spot>::new();
        let ticket = state.begin_observation();
        let invalid = spot("a", u64::MAX, ChargingStatus::Published);
        assert!(state.reconcile_readback(ticket, invalid.clone()).is_none());
        assert!(
            state
                .reconcile_resolution(ticket, invalid.clone())
                .is_none()
        );
        assert_eq!(
            state.reconcile_page(ticket, false, vec![], None, vec![invalid]),
            Vec::new()
        );
        state.record_withdrawal("a".into(), u64::MAX);
        assert!(!state.is_withdrawn("a"));
        let accepted = state.reconcile_readback(ticket, spot("a", 0, ChargingStatus::Published));
        assert_eq!(accepted.map(|spot| spot.local_revision), Some(0));
    }

    #[test]
    fn delayed_page_preserves_newer_vote_readback() {
        let state = ChargingReconciliation::<Spot>::new();
        let page = state.begin_observation();
        let read = state.begin_observation();
        state.reconcile_readback(read, spot("a", 4, ChargingStatus::Published));
        let projected = state.reconcile_page(
            page,
            false,
            vec![],
            None,
            vec![spot("a", 3, ChargingStatus::Published)],
        );
        assert_eq!(projected[0].local_revision, 4);
    }

    #[test]
    fn older_readback_does_not_downgrade_projection() {
        let state = ChargingReconciliation::<Spot>::new();
        let old = state.begin_observation();
        let new = state.begin_observation();
        state.reconcile_readback(new, spot("a", 8, ChargingStatus::Published));
        let result = state.reconcile_readback(old, spot("a", 7, ChargingStatus::Published));
        assert_eq!(result.map(|spot| spot.local_revision), Some(8));
    }

    #[test]
    fn withdrawal_blocks_old_pages_and_newer_publication_restores() {
        let state = ChargingReconciliation::<Spot>::new();
        let old = state.begin_observation();
        state.record_withdrawal("a".into(), 3);
        assert_eq!(
            state.reconcile_page(
                old,
                false,
                vec![],
                None,
                vec![spot("a", 3, ChargingStatus::Published),]
            ),
            vec![]
        );
        let restoration = state.begin_observation();
        let restored =
            state.reconcile_resolution(restoration, spot("a", 4, ChargingStatus::Published));
        assert_eq!(restored.map(|spot| spot.local_revision), Some(4));
        assert!(
            !state
                .reconcile_page(
                    old,
                    false,
                    vec![],
                    None,
                    vec![spot("a", 3, ChargingStatus::Published),]
                )
                .iter()
                .any(|spot| spot.local_revision < 4)
        );
    }

    #[test]
    fn resolution_returns_queued_and_hidden_rows_without_clearing_withdrawal_fence() {
        let state = ChargingReconciliation::<Spot>::new();
        state.record_withdrawal("a".into(), 1);
        let queued_ticket = state.begin_observation();
        let queued =
            state.reconcile_resolution(queued_ticket, spot("a", 2, ChargingStatus::Queued));
        assert_eq!(queued.map(|spot| spot.status), Some(ChargingStatus::Queued));
        assert!(state.is_withdrawn("a"));

        let hidden_ticket = state.begin_observation();
        let hidden =
            state.reconcile_resolution(hidden_ticket, spot("a", 3, ChargingStatus::Hidden));
        assert_eq!(hidden.map(|spot| spot.status), Some(ChargingStatus::Hidden));
        assert!(state.is_withdrawn("a"));
    }

    #[test]
    fn filtered_membership_does_not_add_retained_non_page_rows() {
        let state = ChargingReconciliation::<Spot>::new();
        let ticket = state.begin_observation();
        state.reconcile_readback(ticket, spot("detail", 1, ChargingStatus::Published));
        assert_eq!(
            state.reconcile_page(ticket, false, vec![], None, vec![]),
            vec![]
        );
    }

    #[test]
    fn page_replacement_keeps_readback_for_delayed_page_after_selection_changes() {
        let state = ChargingReconciliation::<Spot>::new();
        let delayed_page = state.begin_observation();
        let vote_readback = state.begin_observation();
        state.reconcile_readback(vote_readback, spot("voted", 4, ChargingStatus::Published));

        let replacement_page = state.begin_observation();
        state.reconcile_page(
            replacement_page,
            false,
            vec![],
            Some("selected"),
            vec![spot("selected", 1, ChargingStatus::Published)],
        );
        let delayed = state.reconcile_page(
            delayed_page,
            false,
            vec![],
            None,
            vec![spot("voted", 3, ChargingStatus::Published)],
        );

        assert_eq!(delayed[0].local_revision, 4);
    }

    #[test]
    fn delayed_page_does_not_restore_nonvisible_readbacks() {
        for status in [
            ChargingStatus::Hidden,
            ChargingStatus::Withdrawn,
            ChargingStatus::Removed,
        ] {
            let state = ChargingReconciliation::<Spot>::new();
            let delayed_page = state.begin_observation();
            let readback = state.begin_observation();
            let hidden = spot("a", 4, status);
            state.reconcile_readback(readback, hidden.clone());

            let page = state.reconcile_page(
                delayed_page,
                false,
                vec![],
                None,
                vec![spot("a", 3, ChargingStatus::Published)],
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
        let state = ChargingReconciliation::<Spot>::new();
        let readback_ticket = state.begin_observation();
        state.reconcile_readback(readback_ticket, spot("seen", 4, ChargingStatus::Published));
        let page_ticket = state.begin_observation();
        state.reconcile_page(
            page_ticket,
            false,
            vec![],
            None,
            vec![spot("seen", 4, ChargingStatus::Published)],
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
