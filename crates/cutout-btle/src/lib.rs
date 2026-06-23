#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]

//! Bluetooth transport adapter scaffolding for Cutout.

mod battery;
mod bridge;
mod capture;
mod error;
mod gatt;
mod identity;
mod observation;
mod peripheral;
mod reconnect;
mod report;
mod scan;
mod target;
mod types;
mod units;

pub use battery::read_battery_level;
/// Single-link protocol session drivers.
pub use bridge::{
    capture_session, capture_session_with_commands, drive_session, drive_session_with_commands,
};
/// Live capture records and raw notification capture utilities.
pub use capture::{
    BtleAttributeValue, CapturedBtlePacket, MalformedBtlePacket, MalformedBtlePacketReason,
    PevcapSessionMetadata, RawNotificationRecord, SessionCapture, SessionCaptureRecord,
    capture_raw_notifications,
};
/// Error types returned by the BTLE adapter.
pub use error::{BtleError, SessionBridgeError};
/// Typed GATT UUID markers for known Cutout-relevant services and characteristics.
pub use gatt::{
    GattUuid, KnownGattUuid, SharedFfe0Service, StandardBatteryLevelCharacteristic,
    StandardBatteryService,
};
/// Identity evidence resolved from parsed model/session outputs.
pub use identity::BridgeIdentityResolution;
/// Passive scan observations and redacted advertisement summaries.
pub use observation::{
    AdvertisedServices, ManufacturerDataSummaries, ManufacturerDataSummary, PeripheralObservation,
};
/// Transport boundary implemented by BTLE peripherals and tests.
pub use peripheral::SessionPeripheral;
/// Multi-link reconnect session orchestration.
pub use reconnect::{
    BtleplugReconnectHost, ReconnectAttemptReport, ReconnectingSessionCapture,
    ReconnectingSessionHost, capture_reconnecting_session,
    capture_reconnecting_session_with_commands, capture_reconnecting_session_with_summaries,
};
/// Protocol bridge reports and semantic bridge events.
pub use report::{SessionBridgeEvent, SessionBridgeReport};
/// Adapter scan and connection entry points.
pub use scan::{connect_and_discover, scan_peripherals};
/// User-supplied connection targets and normalized Bluetooth addresses.
pub use target::{BluetoothAddress, ConnectionTarget, NullBluetoothAddress};
/// Connected peripheral summaries, endpoints, and compact GATT inventory types.
pub use types::{
    CharacteristicSummaries, CharacteristicSummary, ConnectedPeripheral, ConnectionSummary,
    PeripheralIdentifier, ServiceSummaries, ServiceSummary, SessionEndpoints,
};
/// Parsed BTLE adapter units used instead of naked primitives.
pub use units::{
    BatteryLevelPercent, DiagnosticEventCount, DisconnectCount, ManufacturerDataLen,
    MaxReconnectLinks, MonotonicMs, NegotiatedWriteLen, NotificationByteTotal, NotificationCount,
    NotificationWindow, ProtocolWriteCount, ReadOnlyResponseCount, ReconnectAttempt, ScanWindow,
    SubscribeCount, TelemetryEventCount, TransportWriteCount, WriteProvenance,
};

/// Returns the crate name used by setup smoke tests.
#[must_use]
pub const fn crate_name() -> &'static str {
    "cutout-btle"
}

#[cfg(test)]
mod tests;
