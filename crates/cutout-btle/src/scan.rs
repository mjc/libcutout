use std::future::Future;

use async_trait::async_trait;
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
    scan_adapter(&adapter, scan_for).await
}

async fn scan_adapter<A>(
    adapter: &A,
    scan_for: ScanWindow,
) -> Result<Vec<PeripheralObservation>, BtleError>
where
    A: ScanAdapter + Sync,
{
    adapter.start_scan().await?;
    tokio::time::sleep(scan_for.as_duration()).await;
    let observations = adapter.collect_observations().await;
    let _ = adapter.stop_scan().await;
    observations
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
    backend_call(
        "start scan",
        Central::start_scan(&adapter, ScanFilter::default()),
    )
    .await?;

    let peripheral = wait_for_scan_match(scan_for, TARGETED_SCAN_POLL_INTERVAL, || {
        find_peripheral(&adapter, target)
    })
    .await;
    let _ = backend_call("stop scan", Central::stop_scan(&adapter)).await;
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
    let adapters = backend_call("list adapters", manager.adapters()).await?;
    select_first_adapter(adapters)
}

fn select_first_adapter<T>(adapters: Vec<T>) -> Result<T, BtleError> {
    adapters
        .into_iter()
        .next()
        .ok_or(BtleError::NoAdapterAvailable)
}

#[async_trait]
trait ScanAdapter {
    async fn start_scan(&self) -> Result<(), BtleError>;

    async fn stop_scan(&self) -> Result<(), BtleError>;

    async fn collect_observations(&self) -> Result<Vec<PeripheralObservation>, BtleError>;
}

#[async_trait]
impl ScanAdapter for Adapter {
    async fn start_scan(&self) -> Result<(), BtleError> {
        backend_call(
            "start scan",
            Central::start_scan(self, ScanFilter::default()),
        )
        .await
    }

    async fn stop_scan(&self) -> Result<(), BtleError> {
        backend_call("stop scan", Central::stop_scan(self)).await
    }

    async fn collect_observations(&self) -> Result<Vec<PeripheralObservation>, BtleError> {
        collect_observations(self).await
    }
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

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    use super::*;

    type FakeScanResult = Result<Vec<PeripheralObservation>, BtleError>;

    #[derive(Clone, Default)]
    struct FakeScanAdapter {
        collect_result: Arc<Mutex<Option<FakeScanResult>>>,
        start_calls: Arc<AtomicUsize>,
        stop_calls: Arc<AtomicUsize>,
        collect_calls: Arc<AtomicUsize>,
        stopped_after_collect: Arc<AtomicBool>,
    }

    impl FakeScanAdapter {
        fn returning(result: FakeScanResult) -> Self {
            Self {
                collect_result: Arc::new(Mutex::new(Some(result))),
                ..Self::default()
            }
        }

        fn start_calls(&self) -> usize {
            self.start_calls.load(Ordering::SeqCst)
        }

        fn stop_calls(&self) -> usize {
            self.stop_calls.load(Ordering::SeqCst)
        }

        fn collect_calls(&self) -> usize {
            self.collect_calls.load(Ordering::SeqCst)
        }

        fn stopped_after_collect(&self) -> bool {
            self.stopped_after_collect.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl ScanAdapter for FakeScanAdapter {
        async fn start_scan(&self) -> Result<(), BtleError> {
            self.start_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn stop_scan(&self) -> Result<(), BtleError> {
            self.stop_calls.fetch_add(1, Ordering::SeqCst);
            if self.collect_calls() > 0 {
                self.stopped_after_collect.store(true, Ordering::SeqCst);
            }
            Ok(())
        }

        async fn collect_observations(&self) -> Result<Vec<PeripheralObservation>, BtleError> {
            self.collect_calls.fetch_add(1, Ordering::SeqCst);
            self.collect_result
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
                .expect("fake result is configured")
        }
    }

    #[tokio::test]
    async fn scan_adapter_stops_after_successful_collection() {
        let adapter = FakeScanAdapter::returning(Ok(Vec::new()));

        let observations = scan_adapter(&adapter, ScanWindow::from_millis(0))
            .await
            .expect("scan succeeds");

        assert!(observations.is_empty());
        assert_eq!(adapter.start_calls(), 1);
        assert_eq!(adapter.collect_calls(), 1);
        assert_eq!(adapter.stop_calls(), 1);
        assert!(adapter.stopped_after_collect());
    }

    #[tokio::test]
    async fn scan_adapter_stops_after_collection_failure() {
        let adapter = FakeScanAdapter::returning(Err(BtleError::NoPeripheralMatched));

        let err = scan_adapter(&adapter, ScanWindow::from_millis(0))
            .await
            .expect_err("collection error is preserved");

        assert!(matches!(err, BtleError::NoPeripheralMatched));
        assert_eq!(adapter.start_calls(), 1);
        assert_eq!(adapter.collect_calls(), 1);
        assert_eq!(adapter.stop_calls(), 1);
        assert!(adapter.stopped_after_collect());
    }

    #[test]
    fn select_first_adapter_uses_backend_order() {
        let adapter = select_first_adapter(vec!["first", "second"]).expect("adapter");

        assert_eq!(adapter, "first");
    }

    #[test]
    fn select_first_adapter_rejects_empty_backend_list() {
        let err = select_first_adapter::<&str>(Vec::new()).expect_err("no adapter");

        assert!(matches!(err, BtleError::NoAdapterAvailable));
    }
}
