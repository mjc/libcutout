//! Typed facts and operations exchanged with the CoreBluetooth adapter.

use crate::{MobileBluetoothUuid, MobileMelkLightingWriteDto};

/// Rust-owned lifecycle state for the standalone MELK transport.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMelkLightingSessionStateDto {
    /// No transport has been started.
    Idle,
    /// The adapter is discovering candidates.
    Scanning,
    /// A candidate connection is in flight.
    Connecting,
    /// A retry is waiting for its timer.
    Retrying {
        attempt: u8,
        delay_milliseconds: u64,
    },
    /// GATT roles and notifications are being prepared.
    Discovering,
    /// The verified profile is ready for commands.
    Ready,
    /// The adapter was explicitly stopped or cannot reconnect.
    Disconnected,
    /// A terminal error with a user-visible explanation.
    Failed { reason: String },
}

/// CoreBluetooth facts submitted to the Rust MELK reducer.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMelkLightingSessionEventDto {
    /// CoreBluetooth changed power state.
    BluetoothState { powered_on: bool, state_code: i32 },
    /// A peripheral was observed while scanning.
    Discovered {
        name: Option<String>,
        platform_identifier: String,
        rssi: i32,
    },
    /// The platform selected or restored a peripheral.
    Restored {
        name: Option<String>,
        platform_identifier: String,
        connected: bool,
    },
    /// No remembered peripheral was available for the selected identity.
    RestoreUnavailable,
    /// A connection completed.
    Connected {
        name: Option<String>,
        platform_identifier: String,
    },
    /// A connection failed.
    ConnectFailed { reason: String },
    /// The connection attempt timer fired.
    ConnectTimeout,
    /// Services were discovered and the required service was present.
    ServicesDiscovered {
        service_uuids: Vec<MobileBluetoothUuid>,
        error: Option<String>,
    },
    /// Characteristics were discovered with their typed GATT roles.
    CharacteristicsDiscovered {
        name: Option<String>,
        service_uuid: MobileBluetoothUuid,
        characteristics: Vec<MobileMelkLightingCharacteristicEvidenceDto>,
        error: Option<String>,
    },
    /// Notification subscription state changed.
    NotificationState {
        characteristic: MobileBluetoothUuid,
        ready: bool,
        can_send: bool,
        error: Option<String>,
    },
    /// A notification arrived on the verified FFF4 channel.
    Notification {
        characteristic: MobileBluetoothUuid,
        bytes: Vec<u8>,
    },
    /// CoreBluetooth can accept another no-response write.
    WriteReady { can_send: bool },
    /// A reducer timer fired.
    TimerFired {
        timer: MobileMelkLightingTimerDto,
        can_send: bool,
    },
    /// The selected peripheral disconnected.
    Disconnected { reason: String, powered_on: bool },
}

/// Timers requested by the Rust reducer and scheduled by the platform adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMelkLightingTimerDto {
    /// Bounds a pending connection attempt.
    ConnectionAttempt,
    /// Starts the next reconnect attempt.
    Reconnect,
    /// Advances the initialization sequence.
    Initialization,
    /// Advances the bounded user-write queue.
    WriteDrain,
}

/// CoreBluetooth operation requested by the Rust reducer.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMelkLightingSessionActionDto {
    /// Scan without a service filter; MELK does not advertise FFF0.
    Scan,
    /// Stop scanning before connecting to a selected candidate.
    StopScan,
    /// Connect to a platform-local peripheral identifier.
    Connect { platform_identifier: String },
    /// Cancel a failed first-pairing candidate.
    CancelConnect { platform_identifier: String },
    /// Discover the verified FFF0 service.
    DiscoverServices {
        platform_identifier: String,
        service: MobileBluetoothUuid,
    },
    /// Discover all characteristics for FFF0.
    DiscoverCharacteristics {
        platform_identifier: String,
        service: MobileBluetoothUuid,
    },
    /// Subscribe to FFF4 notifications.
    Subscribe {
        platform_identifier: String,
        characteristic: MobileBluetoothUuid,
    },
    /// Execute one typed FFF3 write.
    Write {
        platform_identifier: String,
        write: MobileMelkLightingWriteDto,
    },
    /// Arm a reducer timer on the platform queue.
    ArmTimer {
        timer: MobileMelkLightingTimerDto,
        delay_milliseconds: u64,
    },
}

/// One characteristic's UUID and native CoreBluetooth properties.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMelkLightingCharacteristicEvidenceDto {
    /// GATT UUID.
    pub uuid: MobileBluetoothUuid,
    /// Whether the characteristic supports write-without-response.
    pub write_without_response: bool,
    /// Whether the characteristic supports notifications or indications.
    pub notify_or_indicate: bool,
}

/// A bounded candidate surfaced during first pairing.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMelkLightingSessionCandidateDto {
    /// Platform-local CoreBluetooth identifier.
    pub platform_identifier: String,
    /// Advertised local name, when present.
    pub name: Option<String>,
    /// Last observed RSSI.
    pub rssi: i32,
}

/// Snapshot of Rust-owned session state for the thin platform wrapper.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMelkLightingSessionSnapshotDto {
    /// Current lifecycle state.
    pub state: MobileMelkLightingSessionStateDto,
    /// Selected platform identifier, when one is active.
    pub platform_identifier: Option<String>,
    /// Selected advertised name, when one is known.
    pub name: Option<String>,
    /// Last command evidence state: 0 idle, 1 requested, 2 confirmed, 3 unconfirmed.
    pub command_status: u8,
    /// Whether FFF4 notifications are ready.
    pub notification_ready: bool,
}
