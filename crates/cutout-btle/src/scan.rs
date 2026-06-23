use std::future::Future;

use btleplug::{
    api::{Central, Manager as _, Peripheral as _, ScanFilter},
    platform::{Adapter, Manager},
};

use crate::{
    BtleError, ConnectedPeripheral, ConnectionSummary, ConnectionTarget, PeripheralObservation,
    ServiceSummary,
    error::backend_call,
    units::{ScanPollInterval, ScanWindow},
};

const TARGETED_SCAN_POLL_INTERVAL: ScanPollInterval = ScanPollInterval::from_millis(100);

/// Scans for peripherals and returns what was observed.
///
/// # Errors
///
/// Returns [`BtleError::NoAdapterAvailable`] when the platform exposes no
/// adapters, or [`BtleError::Backend`] when the BTLE backend reports a failure.
pub async fn scan_peripherals(
    scan_for: ScanWindow,
) -> Result<Vec<PeripheralObservation>, BtleError> {
    let adapter = first_adapter().await?;
    backend_call("start scan", adapter.start_scan(ScanFilter::default())).await?;
    tokio::time::sleep(scan_for.as_duration()).await;
    let observations = collect_observations(&adapter).await?;
    let _ = backend_call("stop scan", adapter.stop_scan()).await;
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
    scan_for: ScanWindow,
) -> Result<ConnectedPeripheral, BtleError> {
    let adapter = first_adapter().await?;
    backend_call("start scan", adapter.start_scan(ScanFilter::default())).await?;

    let peripheral = wait_for_scan_match(scan_for, TARGETED_SCAN_POLL_INTERVAL, || {
        find_peripheral(&adapter, target)
    })
    .await;
    let _ = backend_call("stop scan", adapter.stop_scan()).await;
    let peripheral = peripheral?;

    backend_call("connect peripheral", peripheral.connect()).await?;
    backend_call("discover services", peripheral.discover_services()).await?;

    let observation = observation_from_peripheral(&peripheral).await?;
    let services = peripheral
        .services()
        .into_iter()
        .map(|service| ServiceSummary::from_service(&service))
        .collect();

    Ok(ConnectedPeripheral {
        peripheral,
        summary: ConnectionSummary {
            observation,
            services,
        },
    })
}

async fn first_adapter() -> Result<Adapter, BtleError> {
    let manager = Manager::new().await?;
    let mut adapters = backend_call("list adapters", manager.adapters()).await?;
    adapters.pop().ok_or(BtleError::NoAdapterAvailable)
}

async fn collect_observations(adapter: &Adapter) -> Result<Vec<PeripheralObservation>, BtleError> {
    let mut observations = Vec::new();
    for peripheral in backend_call("list peripherals", adapter.peripherals()).await? {
        if let Some(properties) =
            backend_call("read peripheral properties", peripheral.properties()).await?
        {
            observations.push(PeripheralObservation::from_peripheral(
                &peripheral,
                properties,
            ));
        }
    }
    Ok(observations)
}

async fn observation_from_peripheral(
    peripheral: &btleplug::platform::Peripheral,
) -> Result<PeripheralObservation, BtleError> {
    let Some(properties) =
        backend_call("read peripheral properties", peripheral.properties()).await?
    else {
        return Ok(PeripheralObservation::without_properties(peripheral));
    };
    Ok(PeripheralObservation::from_peripheral(
        peripheral, properties,
    ))
}

async fn find_peripheral(
    adapter: &Adapter,
    target: &ConnectionTarget,
) -> Result<btleplug::platform::Peripheral, BtleError> {
    for peripheral in backend_call("list peripherals", adapter.peripherals()).await? {
        let Some(properties) =
            backend_call("read peripheral properties", peripheral.properties()).await?
        else {
            continue;
        };
        let observation = PeripheralObservation::from_peripheral(&peripheral, properties);
        if target.matches(&observation) {
            return Ok(peripheral);
        }
    }
    Err(BtleError::NoPeripheralMatched)
}

pub(crate) async fn wait_for_scan_match<T, F, Fut>(
    scan_for: ScanWindow,
    poll_interval: ScanPollInterval,
    mut find: F,
) -> Result<T, BtleError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, BtleError>>,
{
    let started = tokio::time::Instant::now();
    let deadline = started + scan_for.as_duration();

    loop {
        match find().await {
            Ok(value) => return Ok(value),
            Err(BtleError::NoPeripheralMatched) if tokio::time::Instant::now() < deadline => {
                let next_poll =
                    (tokio::time::Instant::now() + poll_interval.as_duration()).min(deadline);
                tokio::time::sleep_until(next_poll).await;
            }
            Err(error) => return Err(error),
        }
    }
}
