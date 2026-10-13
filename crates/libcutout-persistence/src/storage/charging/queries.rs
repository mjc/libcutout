use super::super::{
    GeoBounds, MapPointCursor, QueryLimit, SpatialSchemaState, StorageError, ensure_spatial_schema,
};
use super::codec::encode;
use super::records::{detail, point_id_from_bytes};
use super::{ChargingFilter, ChargingPage, StoredChargingSpot};
use cutout_ride_maps::{
    ChargingCost, Coordinate, LocationSample, LocationSource, distance_between_millimetres,
};
use rusqlite::{Connection, params};
use std::collections::BinaryHeap;

const CANDIDATES: &str = "FROM map_points_rtree r
JOIN map_point_spatial_keys k ON k.rtree_id=r.id
JOIN charging_spots c ON c.point_id=k.point_id";
const FILTERS: &str = "WHERE c.status IN ('queued','published') AND c.input IS NOT NULL
AND r.max_lat_e7 >= ?1 AND r.min_lat_e7 <= ?2
AND ((?5=0 AND r.max_lon_e7 >= ?3 AND r.min_lon_e7 <= ?4) OR (?5=1 AND (r.max_lon_e7 >= ?3 OR r.min_lon_e7 <= ?4)))
AND (?6 IS NULL OR json_extract(c.input,'$.cost')=?6)
AND ((?7 IS NULL AND ?8 IS NULL) OR EXISTS (SELECT 1 FROM json_each(c.input,'$.supplies') s
 WHERE (?8 IS NULL OR json_extract(s.value,'$.voltage_volts')=?8)
 AND (?7 IS NULL OR EXISTS (SELECT 1 FROM json_each(s.value,'$.connectors') p WHERE json_quote(p.value)=?7 OR p.value=?7))))";
pub(super) fn page(
    connection: &Connection,
    spatial: &mut SpatialSchemaState,
    bounds: GeoBounds,
    filter: &ChargingFilter,
    cursor: Option<MapPointCursor>,
    limit: QueryLimit,
) -> Result<ChargingPage, StorageError> {
    ensure_spatial_schema(connection, spatial)?;
    let after = cursor.map(|value| value.id().uuid().as_bytes().to_vec());
    let connector = filter.connector.as_ref().map(encode).transpose()?;
    let query = format!(
        "SELECT c.point_id {CANDIDATES} {FILTERS}
        AND (?9 IS NULL OR c.point_id > ?9) ORDER BY c.point_id LIMIT ?10"
    );
    let mut statement = connection.prepare(&query)?;
    let rows = statement.query_map(
        params![
            bounds.minimum_latitude,
            bounds.maximum_latitude,
            bounds.minimum_longitude,
            bounds.maximum_longitude,
            bounds.crosses_antimeridian(),
            filter.cost.map(|value| match value {
                ChargingCost::Free => "free",
                ChargingCost::Paid => "paid",
            }),
            connector,
            filter.voltage.map(cutout_ride_maps::SupplyVoltage::as_u16),
            after,
            limit.get() + 1
        ],
        |row| row.get::<_, Vec<u8>>(0),
    )?;
    let mut spots = rows
        .map(|row| detail(connection, point_id_from_bytes(&row?)?)?.ok_or(StorageError::NotFound))
        .collect::<Result<Vec<_>, _>>()?;
    let more = spots.len() > limit.get() as usize;
    if more {
        spots.pop();
    }
    let next_cursor = more
        .then(|| {
            spots
                .last()
                .map(|value| MapPointCursor::new(value.point_id))
        })
        .flatten();
    Ok(ChargingPage { spots, next_cursor })
}
pub(super) fn nearby(
    connection: &Connection,
    spatial: &mut SpatialSchemaState,
    bounds: GeoBounds,
    origin: Coordinate,
    filter: &ChargingFilter,
    limit: QueryLimit,
) -> Result<Vec<StoredChargingSpot>, StorageError> {
    ensure_spatial_schema(connection, spatial)?;
    let connector = filter.connector.as_ref().map(encode).transpose()?;
    let query = format!(
        "SELECT c.point_id, p.latitude_e7, p.longitude_e7 {CANDIDATES}
        JOIN map_points p ON p.id=c.point_id {FILTERS}"
    );
    let mut statement = connection.prepare(&query)?;
    let mut rows = statement.query(params![
        bounds.minimum_latitude,
        bounds.maximum_latitude,
        bounds.minimum_longitude,
        bounds.maximum_longitude,
        bounds.crosses_antimeridian(),
        filter.cost.map(|value| match value {
            ChargingCost::Free => "free",
            ChargingCost::Paid => "paid",
        }),
        connector,
        filter.voltage.map(cutout_ride_maps::SupplyVoltage::as_u16),
    ])?;
    nearest_candidates(&mut rows, origin, limit)?
        .into_iter()
        .map(|id| detail(connection, id)?.ok_or(StorageError::NotFound))
        .collect()
}

fn nearest_candidates(
    rows: &mut rusqlite::Rows<'_>,
    origin: Coordinate,
    limit: QueryLimit,
) -> Result<Vec<super::super::MapPointId>, StorageError> {
    let sample = |coordinate| LocationSample::new(coordinate, 0, 0, None, LocationSource::Live);
    let origin = sample(origin);
    let mut nearest = BinaryHeap::new();
    while let Some(row) = rows.next()? {
        let id = point_id_from_bytes(&row.get::<_, Vec<u8>>(0)?)?;
        let coordinate = Coordinate::from_fixed_parts(row.get(1)?, row.get(2)?)
            .map_err(|_| StorageError::ChargingInvalidInput)?;
        nearest.push((distance_between_millimetres(origin, sample(coordinate)), id));
        if nearest.len() > limit.get() as usize {
            nearest.pop();
        }
    }
    Ok(nearest
        .into_sorted_vec()
        .into_iter()
        .map(|(_, id)| id)
        .collect())
}
