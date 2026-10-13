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

const QUERY: &str = "SELECT c.point_id FROM map_points_rtree r JOIN map_point_spatial_keys k ON k.rtree_id=r.id JOIN charging_spots c ON c.point_id=k.point_id
WHERE c.status IN ('queued','published') AND c.input IS NOT NULL
AND r.max_lat_e7 >= ?1 AND r.min_lat_e7 <= ?2
AND ((?5=0 AND r.max_lon_e7 >= ?3 AND r.min_lon_e7 <= ?4) OR (?5=1 AND (r.max_lon_e7 >= ?3 OR r.min_lon_e7 <= ?4)))
AND (?6 IS NULL OR c.point_id > ?6)
AND (?7 IS NULL OR json_extract(c.input,'$.cost')=?7)
AND ((?8 IS NULL AND ?9 IS NULL) OR EXISTS (SELECT 1 FROM json_each(c.input,'$.supplies') s
 WHERE (?9 IS NULL OR json_extract(s.value,'$.voltage_volts')=?9)
 AND (?8 IS NULL OR EXISTS (SELECT 1 FROM json_each(s.value,'$.connectors') p WHERE json_quote(p.value)=?8 OR p.value=?8))))
ORDER BY c.point_id LIMIT ?10";
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
    let mut statement = connection.prepare(QUERY)?;
    let rows = statement.query_map(
        params![
            bounds.minimum_latitude,
            bounds.maximum_latitude,
            bounds.minimum_longitude,
            bounds.maximum_longitude,
            bounds.crosses_antimeridian(),
            after,
            filter.cost.map(|value| match value {
                ChargingCost::Free => "free",
                ChargingCost::Paid => "paid",
            }),
            connector,
            filter.voltage.map(cutout_ride_maps::SupplyVoltage::as_u16),
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
    let sample = |coordinate| LocationSample::new(coordinate, 0, 0, None, LocationSource::Live);
    let mut best = Vec::new();
    let mut cursor = None;
    loop {
        let page = page(connection, spatial, bounds, filter, cursor, QueryLimit(500))?;
        for spot in page.spots {
            let coordinate = spot
                .input
                .as_ref()
                .ok_or(StorageError::ChargingInvalidInput)?
                .coordinate();
            best.push((
                distance_between_millimetres(sample(origin), sample(coordinate)),
                spot.point_id,
                spot,
            ));
            best.sort_unstable_by_key(|value| (value.0, value.1));
            best.truncate(limit.get() as usize);
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    Ok(best.into_iter().map(|(_, _, spot)| spot).collect())
}
