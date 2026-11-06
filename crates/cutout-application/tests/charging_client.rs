//! Exercises the public application surface as a binding-free Rust client.

use cutout_application::charging::{
    discovery::{ChargingDiscovery, ChargingDiscoverySearchResult},
    reconciliation::{ChargingObservation, ChargingReconciliation},
};
use cutout_ride_maps::Coordinate;
use libcutout_persistence::ChargingStatus;

#[derive(Clone, PartialEq)]
struct ViewportFilter {
    connector: String,
}

#[derive(Clone, Eq, PartialEq)]
struct DirectoryRow {
    id: String,
    revision: u64,
    status: ChargingStatus,
    label: String,
}

impl ChargingObservation for DirectoryRow {
    fn id(&self) -> &str {
        &self.id
    }

    fn revision(&self) -> u64 {
        self.revision
    }

    fn status(&self) -> ChargingStatus {
        self.status
    }
}

#[test]
fn rust_client_executes_a_discovery_effect_and_consumes_its_camera_target_once() {
    let filter = ViewportFilter {
        connector: "type-2".into(),
    };
    let discovery = ChargingDiscovery::new(filter.clone());
    let position = Coordinate::from_degrees(39.74, -104.99).unwrap();

    assert!(discovery.viewport_loaded(false).is_none());
    let request = discovery.location_available(position).unwrap();
    assert!(request.filter == filter);
    assert_eq!(request.origin, position);
    assert_eq!(request.limit, 1);

    let result = ChargingDiscoverySearchResult::Nearest(position);
    assert_eq!(discovery.search_completed(result), Some(position));
    assert_eq!(discovery.search_completed(result), None);
}

#[test]
fn rust_client_preserves_its_row_payload_through_delayed_page_reconciliation() {
    let projection = ChargingReconciliation::<DirectoryRow>::new();
    let page_request = projection.begin_observation();
    let readback_request = projection.begin_observation();
    let current = DirectoryRow {
        id: "spot-a".into(),
        revision: 3,
        status: ChargingStatus::Published,
        label: "Current local edit".into(),
    };
    assert!(
        projection.reconcile_readback(readback_request, current.clone()) == Some(current.clone())
    );
    let stale = DirectoryRow {
        revision: 2,
        label: "Delayed page".into(),
        ..current.clone()
    };
    assert!(
        projection.reconcile_page(page_request, false, Vec::new(), None, vec![stale])
            == vec![current]
    );
}
