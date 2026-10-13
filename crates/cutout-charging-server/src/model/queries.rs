use super::{Error, PublicSpot, ReportStatus, SpotState, id};
use cutout_ride_maps::{ChargingCost, Coordinate};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListQuery {
    pub south_e7: Option<i32>,
    pub north_e7: Option<i32>,
    pub west_e7: Option<i32>,
    pub east_e7: Option<i32>,
    pub cost: Option<ChargingCost>,
    pub after: Option<String>,
    pub limit: Option<u32>,
}

impl ListQuery {
    pub(crate) fn into_validated(self) -> Result<ValidatedListQuery, Error> {
        let limit = PageSize::new(self.limit)?;
        let bounds =
            DiscoveryBounds::new(self.south_e7, self.north_e7, self.west_e7, self.east_e7)?;
        Ok(ValidatedListQuery {
            after: self
                .after
                .map(|value| id(&value))
                .transpose()?
                .unwrap_or_default(),
            cost: self.cost,
            bounds,
            limit,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PageSize(u32);

impl PageSize {
    const DEFAULT: u32 = 50;

    fn new(value: Option<u32>) -> Result<Self, Error> {
        let value = value.unwrap_or(Self::DEFAULT);
        (1..=100)
            .contains(&value)
            .then_some(Self(value))
            .ok_or(Error::Invalid)
    }

    pub(crate) fn count(self) -> usize {
        self.0 as usize
    }
    pub(crate) fn lookahead(self) -> u32 {
        self.0 + 1
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DiscoveryBounds {
    south: i32,
    north: i32,
    west: i32,
    east: i32,
}

impl DiscoveryBounds {
    fn new(
        south: Option<i32>,
        north: Option<i32>,
        west: Option<i32>,
        east: Option<i32>,
    ) -> Result<Self, Error> {
        let (south, north, west, east) = match (south, north, west, east) {
            (None, None, None, None) => (-900_000_000, 900_000_000, -1_800_000_000, 1_800_000_000),
            (Some(south), Some(north), Some(west), Some(east)) => (south, north, west, east),
            _ => return Err(Error::Invalid),
        };
        Coordinate::from_fixed_parts(south, west).map_err(|_| Error::Invalid)?;
        Coordinate::from_fixed_parts(north, east).map_err(|_| Error::Invalid)?;
        if south > north {
            return Err(Error::Invalid);
        }
        Ok(Self {
            south,
            north,
            west,
            east,
        })
    }

    pub(crate) fn south_e7(self) -> i32 {
        self.south
    }
    pub(crate) fn north_e7(self) -> i32 {
        self.north
    }
    pub(crate) fn west_e7(self) -> i32 {
        self.west
    }
    pub(crate) fn east_e7(self) -> i32 {
        self.east
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ValidatedListQuery {
    after: String,
    cost: Option<ChargingCost>,
    bounds: DiscoveryBounds,
    limit: PageSize,
}

impl ValidatedListQuery {
    pub(crate) fn after(&self) -> &str {
        &self.after
    }
    pub(crate) fn cost(&self) -> Option<ChargingCost> {
        self.cost
    }
    pub(crate) fn bounds(&self) -> DiscoveryBounds {
        self.bounds
    }
    pub(crate) fn limit(&self) -> PageSize {
        self.limit
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ValidatedChangesQuery {
    after: u64,
    limit: PageSize,
}

impl ValidatedChangesQuery {
    pub(crate) fn after(&self) -> u64 {
        self.after
    }
    pub(crate) fn limit(&self) -> PageSize {
        self.limit
    }
}

impl ChangesQuery {
    pub(crate) fn into_validated(self) -> Result<ValidatedChangesQuery, Error> {
        let limit = PageSize::new(self.limit)?;
        let after = self.after.unwrap_or_default();
        if after > i64::MAX as u64 {
            return Err(Error::Invalid);
        }
        Ok(ValidatedChangesQuery { after, limit })
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SpotPage {
    pub items: Vec<PublicSpot>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChangesQuery {
    pub after: Option<u64>,
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ChangesPage {
    pub items: Vec<SpotState>,
    pub next_cursor: u64,
    pub has_more: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReportsQuery {
    pub after: Option<String>,
    pub limit: Option<u32>,
    pub status: Option<ReportStatus>,
}

#[derive(Clone, Debug)]
pub(crate) struct ValidatedReportsQuery {
    after: String,
    status: Option<ReportStatus>,
    limit: PageSize,
}

impl ValidatedReportsQuery {
    pub(crate) fn after(&self) -> &str {
        &self.after
    }
    pub(crate) fn status(&self) -> Option<ReportStatus> {
        self.status
    }
    pub(crate) fn limit(&self) -> PageSize {
        self.limit
    }
}

impl ReportsQuery {
    pub(crate) fn into_validated(self) -> Result<ValidatedReportsQuery, Error> {
        Ok(ValidatedReportsQuery {
            after: self
                .after
                .map(|value| id(&value))
                .transpose()?
                .unwrap_or_default(),
            status: self.status,
            limit: PageSize::new(self.limit)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ChangesQuery, ListQuery, ReportStatus, ReportsQuery};

    #[test]
    fn list_page_size_and_bounds_are_validated_as_a_unit() {
        let default = list(None, None, None, None, None, None)
            .into_validated()
            .unwrap();
        assert_eq!(default.limit().count(), 50);
        assert_eq!(default.limit().lookahead(), 51);
        for size in [1, 100] {
            let query = list(None, None, None, None, None, Some(size))
                .into_validated()
                .unwrap();
            assert_eq!(query.limit().count(), size as usize);
            assert_eq!(query.limit().lookahead(), size + 1);
        }
        for size in [0, 101] {
            assert!(
                list(None, None, None, None, None, Some(size))
                    .into_validated()
                    .is_err()
            );
        }

        assert!(
            list(Some(0), None, Some(0), Some(1), None, None)
                .into_validated()
                .is_err()
        );
        assert!(
            list(Some(2), Some(1), Some(0), Some(1), None, None)
                .into_validated()
                .is_err()
        );
        assert!(
            list(Some(0), Some(900_000_001), Some(0), Some(1), None, None)
                .into_validated()
                .is_err()
        );
        let antimeridian = list(
            Some(-10),
            Some(10),
            Some(1_700_000_000),
            Some(-1_700_000_000),
            None,
            None,
        )
        .into_validated()
        .unwrap();
        assert_eq!(antimeridian.bounds().west_e7(), 1_700_000_000);
        assert_eq!(antimeridian.bounds().east_e7(), -1_700_000_000);
        assert_eq!(antimeridian.bounds().south_e7(), -10);
        assert_eq!(antimeridian.bounds().north_e7(), 10);
    }

    #[test]
    fn cursors_are_normalized_and_bounded() {
        let uppercase = "00000000-0000-4000-8000-0000000000AB";
        let validated = list(None, None, None, None, Some(uppercase.into()), None)
            .into_validated()
            .unwrap();
        assert_eq!(validated.after(), "00000000-0000-4000-8000-0000000000ab");
        assert!(
            list(None, None, None, None, Some("bad".into()), None)
                .into_validated()
                .is_err()
        );

        let reports = ReportsQuery {
            after: Some(uppercase.into()),
            limit: None,
            status: Some(ReportStatus::Open),
        }
        .into_validated()
        .unwrap();
        assert_eq!(reports.after(), "00000000-0000-4000-8000-0000000000ab");
        assert_eq!(reports.status(), Some(ReportStatus::Open));
        assert_eq!(reports.limit().count(), 50);
        assert!(
            ReportsQuery {
                after: Some("bad".into()),
                limit: None,
                status: None
            }
            .into_validated()
            .is_err()
        );

        let changes = ChangesQuery {
            after: Some(i64::MAX as u64),
            limit: Some(100),
        }
        .into_validated()
        .unwrap();
        assert_eq!(changes.after(), i64::MAX as u64);
        assert_eq!(changes.limit().lookahead(), 101);
        assert!(
            ChangesQuery {
                after: Some(i64::MAX as u64 + 1),
                limit: None
            }
            .into_validated()
            .is_err()
        );
        assert!(
            ChangesQuery {
                after: None,
                limit: Some(0)
            }
            .into_validated()
            .is_err()
        );
    }

    fn list(
        south_e7: Option<i32>,
        north_e7: Option<i32>,
        west_e7: Option<i32>,
        east_e7: Option<i32>,
        after: Option<String>,
        limit: Option<u32>,
    ) -> ListQuery {
        ListQuery {
            south_e7,
            north_e7,
            west_e7,
            east_e7,
            cost: None,
            after,
            limit,
        }
    }
}
