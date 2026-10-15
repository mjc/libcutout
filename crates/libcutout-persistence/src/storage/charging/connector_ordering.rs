use super::super::{SpatialSchemaState, StorageError, ensure_spatial_schema};
use super::codec::{Input, decode};
use cutout_ride_maps::{
    ChargingConnector, ChargingConnectorPrevalence, ChargingCountry, Coordinate, LocationSample,
    LocationSource, distance_between_millimetres,
};
use rusqlite::{Connection, params};

const LOCAL_RADIUS_MILLIMETRES: u64 = 25_000_000;
const EARTH_RADIUS_METRES: f64 = 6_371_000.0;

pub(super) fn choices(
    connection: &Connection,
    spatial: &mut SpatialSchemaState,
    origin: Coordinate,
    country: Option<&ChargingCountry>,
) -> Result<Vec<ChargingConnector>, StorageError> {
    ensure_spatial_schema(connection, spatial)?;
    let mut prevalence = ChargingConnectorPrevalence::new(country);
    let origin_sample = sample(origin);
    // A latitude belt is conservative across the antimeridian and at the poles.
    // The exact circle test below excludes all other longitudes without a row cap.
    let latitude_span = (25_000.0 / EARTH_RADIUS_METRES).to_degrees();
    let mut statement = connection.prepare(
        "SELECT c.input FROM map_points_rtree r
         JOIN map_point_spatial_keys k ON k.rtree_id=r.id
         JOIN charging_spots c ON c.point_id=k.point_id
         WHERE c.status IN ('queued','published') AND c.input IS NOT NULL
         AND r.max_lat_e7 >= ?1 AND r.min_lat_e7 <= ?2",
    )?;
    let mut rows = statement.query(params![
        (origin.latitude_degrees() - latitude_span) * 10_000_000.0,
        (origin.latitude_degrees() + latitude_span) * 10_000_000.0,
    ])?;
    while let Some(row) = rows.next()? {
        let encoded: String = row.get(0)?;
        let spot = decode::<Input>(&encoded)?.validated()?;
        if distance_between_millimetres(origin_sample, sample(spot.coordinate()))
            <= LOCAL_RADIUS_MILLIMETRES
        {
            prevalence.record_spot(spot.details().supplies());
        }
    }
    Ok(prevalence.into_choices())
}

fn sample(coordinate: Coordinate) -> LocationSample {
    LocationSample::new(coordinate, 0, 0, None, LocationSource::Live)
}

#[cfg(test)]
mod tests {
    use super::super::{mutations, records};
    use super::*;
    use crate::storage::charging::{ChargingAction, ChargingRevision};
    use cutout_ride_maps::{ChargingCost, ChargingDetails, ChargingSupply, NewChargingSpot};

    fn database() -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        super::super::super::migrations::migrate(&mut connection).unwrap();
        connection
    }

    fn save_spot(
        connection: &mut Connection,
        spatial: &mut SpatialSchemaState,
        longitude: f64,
        connector: ChargingConnector,
    ) -> super::super::super::MapPointId {
        let details = ChargingDetails::new(
            None,
            None,
            None,
            None,
            None,
            vec![ChargingSupply::single(None, connector)],
        )
        .unwrap();
        let input = NewChargingSpot::new(
            Coordinate::from_degrees(0.0, longitude).unwrap(),
            "Outlet",
            ChargingCost::Free,
        )
        .unwrap()
        .with_details(details)
        .unwrap();
        mutations::save(connection, spatial, &input)
            .unwrap()
            .point_id
    }

    #[test]
    fn local_order_excludes_distant_and_withdrawn_spots_and_falls_back_when_empty() {
        let mut connection = database();
        let mut spatial = SpatialSchemaState::Uninitialized;
        let country = ChargingCountry::new("US").unwrap();
        let origin = Coordinate::from_degrees(0.0, 0.0).unwrap();
        let fallback = cutout_ride_maps::charging_connector_choices(Some(&country));
        assert_eq!(
            choices(&connection, &mut spatial, origin, Some(&country)).unwrap(),
            fallback
        );
        save_spot(
            &mut connection,
            &mut spatial,
            0.01,
            ChargingConnector::J1772,
        );
        for _ in 0..3 {
            save_spot(&mut connection, &mut spatial, 1.0, ChargingConnector::Nacs);
        }
        let withdrawn = save_spot(
            &mut connection,
            &mut spatial,
            0.02,
            ChargingConnector::TypeA,
        );
        mutations::change(
            &mut connection,
            &mut spatial,
            withdrawn,
            ChargingRevision::INITIAL,
            &ChargingAction::Withdraw,
        )
        .unwrap();
        let ranked = choices(&connection, &mut spatial, origin, Some(&country)).unwrap();
        assert_eq!(ranked[0], ChargingConnector::J1772);
        assert_eq!(ranked[1], fallback[0]);
        assert_eq!(
            records::detail(&connection, withdrawn)
                .unwrap()
                .unwrap()
                .status,
            super::super::ChargingStatus::Withdrawn
        );
    }

    #[test]
    fn local_circle_crosses_antimeridian_and_handles_polar_locations() {
        let mut connection = database();
        let mut spatial = SpatialSchemaState::Uninitialized;
        save_spot(
            &mut connection,
            &mut spatial,
            -179.99,
            ChargingConnector::Nacs,
        );
        let origin = Coordinate::from_degrees(0.0, 179.99).unwrap();
        assert_eq!(
            choices(&connection, &mut spatial, origin, None).unwrap()[0],
            ChargingConnector::Nacs
        );
        let pole = Coordinate::from_degrees(90.0, 0.0).unwrap();
        assert_eq!(
            choices(&connection, &mut spatial, pole, None).unwrap(),
            cutout_ride_maps::charging_connector_choices(None)
        );
    }
}
