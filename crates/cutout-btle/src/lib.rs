#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]

//! Bluetooth transport adapter scaffolding for Cutout.

use std::{fmt, time::Duration};

use btleplug::{
    api::{Central, Manager as _, Peripheral as _, PeripheralProperties, ScanFilter},
    platform::{Adapter, Manager},
};
use thiserror::Error;
use uuid::Uuid;

/// Returns the crate name used by setup smoke tests.
#[must_use]
pub const fn crate_name() -> &'static str {
    "cutout-btle"
}

/// A peripheral observation gathered from a scan or connection pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeripheralObservation {
    /// Bluetooth address rendered in the platform default format.
    pub address: String,

    /// Peripheral local name, if one was advertised.
    pub name: Option<String>,

    /// Received signal strength, if the platform exposed it.
    pub rssi: Option<i16>,

    /// Advertised service UUIDs, if the peripheral exposed them.
    pub advertised_services: Vec<Uuid>,
}

impl PeripheralObservation {
    fn from_properties(properties: &PeripheralProperties) -> Self {
        Self {
            address: properties.address.to_string(),
            name: properties.local_name.clone(),
            rssi: properties.rssi,
            advertised_services: properties.services.clone(),
        }
    }
}

impl fmt::Display for PeripheralObservation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} name={}",
            self.address,
            self.name.as_deref().unwrap_or("<none>")
        )?;
        if let Some(rssi) = self.rssi {
            write!(f, " rssi={rssi}")?;
        }
        write!(f, " services=[{}]", join_uuids(&self.advertised_services))
    }
}

/// Target used to select a peripheral from scan results.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConnectionTarget {
    /// Match against the peripheral address, when provided.
    pub address: Option<String>,

    /// Match against the peripheral local name, when provided.
    pub name_contains: Option<String>,
}

impl ConnectionTarget {
    /// Returns whether an observation matches this target.
    #[must_use]
    pub fn matches(&self, observation: &PeripheralObservation) -> bool {
        let address_matches = self
            .address
            .as_ref()
            .is_none_or(|address| observation.address == *address);
        let name_matches = self.name_contains.as_ref().is_none_or(|needle| {
            observation
                .name
                .as_deref()
                .is_some_and(|name| name.contains(needle))
        });

        address_matches && name_matches
    }
}

/// Summary of a successful connection/discovery pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionSummary {
    /// Selected peripheral observation.
    pub observation: PeripheralObservation,

    /// Discovered characteristic UUIDs.
    pub characteristic_uuids: Vec<Uuid>,
}

impl fmt::Display for ConnectionSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "connected {}", self.observation)?;
        write!(
            f,
            "characteristics=[{}]",
            join_uuids(&self.characteristic_uuids)
        )
    }
}

/// Errors surfaced by the BTLE adapter.
#[derive(Debug, Error)]
pub enum BtleError {
    /// No Bluetooth adapters were reported by the platform.
    #[error("no bluetooth adapters were found")]
    NoAdapterAvailable,

    /// No peripheral matched the requested target.
    #[error("no peripheral matched the requested target")]
    NoPeripheralMatched,

    /// Error reported by the underlying BTLE stack.
    #[error(transparent)]
    Backend(#[from] btleplug::Error),
}

/// Scans for peripherals and returns what was observed.
///
/// # Errors
///
/// Returns [`BtleError::NoAdapterAvailable`] when the platform exposes no
/// adapters, or [`BtleError::Backend`] when the BTLE backend reports a failure.
pub async fn scan_peripherals(scan_for: Duration) -> Result<Vec<PeripheralObservation>, BtleError> {
    let adapter = first_adapter().await?;
    adapter.start_scan(ScanFilter::default()).await?;
    tokio::time::sleep(scan_for).await;
    let observations = collect_observations(&adapter).await?;
    let _ = adapter.stop_scan().await;
    Ok(observations)
}

/// Connects to the first peripheral matching the target and returns a summary.
///
/// # Errors
///
/// Returns [`BtleError::NoAdapterAvailable`] if no adapter is present,
/// [`BtleError::NoPeripheralMatched`] if scan results do not satisfy the
/// target, or [`BtleError::Backend`] if the BTLE backend fails.
pub async fn connect_and_discover(
    target: &ConnectionTarget,
    scan_for: Duration,
) -> Result<ConnectionSummary, BtleError> {
    let adapter = first_adapter().await?;
    adapter.start_scan(ScanFilter::default()).await?;
    tokio::time::sleep(scan_for).await;

    let peripheral = find_peripheral(&adapter, target).await?;
    let _ = adapter.stop_scan().await;

    peripheral.connect().await?;
    peripheral.discover_services().await?;

    let observation = observation_from_peripheral(&peripheral).await?;
    let characteristic_uuids = peripheral
        .characteristics()
        .into_iter()
        .map(|characteristic| characteristic.uuid)
        .collect();

    Ok(ConnectionSummary {
        observation,
        characteristic_uuids,
    })
}

async fn first_adapter() -> Result<Adapter, BtleError> {
    let manager = Manager::new().await?;
    let mut adapters = manager.adapters().await?;
    adapters.pop().ok_or(BtleError::NoAdapterAvailable)
}

async fn collect_observations(adapter: &Adapter) -> Result<Vec<PeripheralObservation>, BtleError> {
    let mut observations = Vec::new();
    for peripheral in adapter.peripherals().await? {
        if let Some(properties) = peripheral.properties().await? {
            observations.push(PeripheralObservation::from_properties(&properties));
        }
    }
    Ok(observations)
}

async fn observation_from_peripheral(
    peripheral: &btleplug::platform::Peripheral,
) -> Result<PeripheralObservation, BtleError> {
    let Some(properties) = peripheral.properties().await? else {
        return Ok(PeripheralObservation {
            address: "<unknown>".to_owned(),
            name: None,
            rssi: None,
            advertised_services: Vec::new(),
        });
    };
    Ok(PeripheralObservation::from_properties(&properties))
}

async fn find_peripheral(
    adapter: &Adapter,
    target: &ConnectionTarget,
) -> Result<btleplug::platform::Peripheral, BtleError> {
    for peripheral in adapter.peripherals().await? {
        let Some(properties) = peripheral.properties().await? else {
            continue;
        };
        let observation = PeripheralObservation::from_properties(&properties);
        if target.matches(&observation) {
            return Ok(peripheral);
        }
    }
    Err(BtleError::NoPeripheralMatched)
}

fn join_uuids(values: &[Uuid]) -> String {
    values
        .iter()
        .map(Uuid::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::crate_name;

    #[test]
    fn exposes_the_expected_name() {
        assert_eq!(crate_name(), "cutout-btle");
    }

    #[test]
    fn connection_target_matches_on_address_and_name() {
        let target = crate::ConnectionTarget {
            address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
            name_contains: Some("Aero".to_owned()),
        };
        let observation = crate::PeripheralObservation {
            address: "AA:BB:CC:DD:EE:FF".to_owned(),
            name: Some("NOSFET Aero".to_owned()),
            rssi: Some(-42),
            advertised_services: vec![],
        };

        assert!(target.matches(&observation));
    }

    #[test]
    fn connection_summary_renders_services_and_characteristics() {
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                address: "AA:BB:CC:DD:EE:FF".to_owned(),
                name: Some("NOSFET Aero".to_owned()),
                rssi: Some(-42),
                advertised_services: vec![],
            },
            characteristic_uuids: vec![],
        };

        assert!(summary.to_string().contains("AA:BB:CC:DD:EE:FF"));
        assert!(summary.to_string().contains("NOSFET Aero"));
    }
}
