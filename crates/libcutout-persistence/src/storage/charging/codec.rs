use super::super::StorageError;
use super::ChargingAction;
use cutout_ride_maps::{
    ChargingCost, ChargingDetails, ChargingPaidTerms, ChargingPrice, ChargingSupply, ChargingVenue,
    ChargingVote, Coordinate, NewChargingSpot,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    coordinate: Pin,
    access_note: String,
    cost: ChargingCost,
    name: Option<String>,
    venue: Option<ChargingVenue>,
    hours: Option<String>,
    paid_terms: Option<ChargingPaidTerms>,
    price: Option<ChargingPrice>,
    supplies: Vec<ChargingSupply>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Pin {
    latitude_e7: i32,
    longitude_e7: i32,
}
impl Input {
    pub(super) fn validated(self) -> Result<NewChargingSpot, StorageError> {
        let invalid = || StorageError::InvalidStoredValue {
            field: "charging input",
            value: "invalid domain value".to_owned(),
        };
        let coordinate =
            Coordinate::from_fixed_parts(self.coordinate.latitude_e7, self.coordinate.longitude_e7)
                .map_err(|_| invalid())?;
        let details = ChargingDetails::new(
            self.name.as_deref(),
            self.venue,
            self.hours.as_deref(),
            self.paid_terms,
            self.price,
            self.supplies,
        )
        .map_err(|_| invalid())?;
        NewChargingSpot::new(coordinate, &self.access_note, self.cost)
            .map_err(|_| invalid())?
            .with_details(details)
            .map_err(|_| invalid())
    }
}
#[derive(Deserialize, Serialize)]
enum ActionWire {
    Create(Input),
    Edit(Input),
    Withdraw,
    Verify,
    Vote(Option<ChargingVote>),
}
pub(super) fn encode<T: Serialize>(value: &T) -> Result<String, StorageError> {
    serde_json::to_string(value).map_err(|_| StorageError::ChargingInvalidInput)
}
pub(super) fn decode<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, StorageError> {
    serde_json::from_str(value).map_err(|_| StorageError::InvalidStoredValue {
        field: "charging data",
        value: "invalid encoding".to_owned(),
    })
}
pub(super) fn input_json(input: &NewChargingSpot) -> Result<String, StorageError> {
    encode(&input.community_submission())
}
pub(super) fn action_json(action: &ChargingAction) -> Result<String, StorageError> {
    let input = |value: &NewChargingSpot| decode::<Input>(&input_json(value)?);
    encode(&match action {
        ChargingAction::Create(value) => ActionWire::Create(input(value)?),
        ChargingAction::Edit(value) => ActionWire::Edit(input(value)?),
        ChargingAction::Withdraw => ActionWire::Withdraw,
        ChargingAction::Verify => ActionWire::Verify,
        ChargingAction::Vote(value) => ActionWire::Vote(*value),
    })
}
pub(super) fn action_from_json(value: &str) -> Result<ChargingAction, StorageError> {
    Ok(match decode(value)? {
        ActionWire::Create(value) => ChargingAction::Create(value.validated()?),
        ActionWire::Edit(value) => ChargingAction::Edit(value.validated()?),
        ActionWire::Withdraw => ChargingAction::Withdraw,
        ActionWire::Verify => ChargingAction::Verify,
        ActionWire::Vote(value) => ChargingAction::Vote(value),
    })
}
