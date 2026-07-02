//! Model registry, catalog, and authoring types.

use crate::{
    BatteryPageKind, BmsCellValuesPerPage, BmsTemperatureValuesPerPage, Capacity, CommandKind,
    CommandMetadata, DeviceCommand, GattChannel, ParallelCount, ProtocolSelector, SeriesCount,
    UnsupportedReason, Voltage,
};
use core::marker::PhantomData;
use std::ops::RangeInclusive;
use thiserror::Error;

/// Protocol family identifier used by registry data.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProtocolFamily {
    /// Veteran/LeaperKim/NOSFET `dc5a5c` frame family.
    VeteranLeaperkimNosfet,

    /// Begode/Gotway `55aa` frame family.
    BegodeGotway,

    /// VESC UART/CAN-derived family used by Refloat-style controllers.
    Vesc,
}

/// Verification state for registry fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerificationStatus {
    /// Not yet verified.
    Unverified,

    /// Inferred from partial evidence.
    Inferred,

    /// Verified against source-attributed protocol documentation.
    SourceVerified,

    /// Verified against actual Bluetooth hardware.
    HardwareVerified,

    /// Verified against both source-attributed documentation and hardware.
    SourceAndHardwareVerified,
}

/// A registry value plus its verification state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedValue<T> {
    /// Data value.
    pub value: T,

    /// Verification status for this value.
    pub verification: VerificationStatus,
}

/// Battery metadata for a registry entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatterySpec {
    /// Series cell count.
    pub series_cells: SeriesCount,

    /// Nominal pack capacity, when known.
    pub nominal_capacity: Option<Capacity>,

    /// Expected pack voltage range.
    pub voltage_range: RangeInclusive<Voltage>,

    /// Verification status for the battery metadata.
    pub verification: VerificationStatus,
}

/// Static BMS selector interpretation for a registry entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BmsPageSelectorSpec {
    /// BMS page selector value.
    pub selector: ProtocolSelector,

    /// Current interpretation of the selector.
    pub kind: BatteryPageKind,

    /// Verification status for this selector interpretation.
    pub verification: VerificationStatus,
}

/// Static BMS layout metadata for a registry entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BmsLayoutSpec {
    /// Series-connected cell count covered by this BMS layout.
    pub series_cells: SeriesCount,

    /// Parallel pack count for this model.
    pub parallel_packs: ParallelCount,

    /// Cell-voltage values decoded from a full cell-voltage page.
    pub cell_values_per_page: BmsCellValuesPerPage,

    /// Temperature values decoded from a full temperature page.
    pub temperature_values_per_page: BmsTemperatureValuesPerPage,

    /// Static selector interpretation table.
    pub selectors: &'static [BmsPageSelectorSpec],

    /// Verification status for the layout geometry.
    pub verification: VerificationStatus,
}

/// Observed roles for a GATT characteristic.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GattRoles(u8);

impl GattRoles {
    const READ: u8 = 1 << 0;
    const WRITE: u8 = 1 << 1;
    const WRITE_WITHOUT_RESPONSE: u8 = 1 << 2;
    const NOTIFY: u8 = 1 << 3;
    const INDICATE: u8 = 1 << 4;

    /// Empty role set.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Returns whether no roles are set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Adds read support.
    #[must_use]
    pub const fn with_read(self) -> Self {
        Self(self.0 | Self::READ)
    }

    /// Adds write-with-response support.
    #[must_use]
    pub const fn with_write(self) -> Self {
        Self(self.0 | Self::WRITE)
    }

    /// Adds write-without-response support.
    #[must_use]
    pub const fn with_write_without_response(self) -> Self {
        Self(self.0 | Self::WRITE_WITHOUT_RESPONSE)
    }

    /// Adds notification support.
    #[must_use]
    pub const fn with_notify(self) -> Self {
        Self(self.0 | Self::NOTIFY)
    }

    /// Adds indication support.
    #[must_use]
    pub const fn with_indicate(self) -> Self {
        Self(self.0 | Self::INDICATE)
    }

    /// Returns whether read is supported.
    #[must_use]
    pub const fn supports_read(self) -> bool {
        self.0 & Self::READ != 0
    }

    /// Returns whether write with response is supported.
    #[must_use]
    pub const fn supports_write(self) -> bool {
        self.0 & Self::WRITE != 0
    }

    /// Returns whether write without response is supported.
    #[must_use]
    pub const fn supports_write_without_response(self) -> bool {
        self.0 & Self::WRITE_WITHOUT_RESPONSE != 0
    }

    /// Returns whether notify is supported.
    #[must_use]
    pub const fn supports_notify(self) -> bool {
        self.0 & Self::NOTIFY != 0
    }

    /// Returns whether indicate is supported.
    #[must_use]
    pub const fn supports_indicate(self) -> bool {
        self.0 & Self::INDICATE != 0
    }
}

/// GATT service/characteristic fingerprint for a registry entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GattFingerprint {
    /// Observed service UUID.
    pub service: GattChannel,

    /// Observed characteristic UUID.
    pub characteristic: GattChannel,

    /// Observed characteristic roles.
    pub roles: GattRoles,

    /// Verification status for this fingerprint.
    pub verification: VerificationStatus,
}

/// Platform namespace for an installed-device identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstalledDevicePlatform {
    /// Apple `CoreBluetooth` peripheral identifier.
    CoreBluetooth,

    /// Android Bluetooth stack identifier.
    Android,

    /// Other host platform namespace.
    Other,
}

/// Opaque platform-scoped identifier for a remembered device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstalledDevicePlatformId<'a> {
    /// Platform namespace for this identifier.
    pub platform: InstalledDevicePlatform,

    /// Opaque identifier value as reported by the platform.
    pub value: &'a str,
}

/// Protocol-reported device serial number.
pub type ProtocolSerial<'a> = VerifiedValue<&'a str>;

/// Resolved installed-device model identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstalledDeviceModel<'a> {
    /// Resolved manufacturer or brand.
    pub manufacturer: &'a str,

    /// Resolved model name.
    pub model: &'a str,

    /// Resolved protocol family.
    pub protocol_family: ProtocolFamily,

    /// Verification status for this model resolution.
    pub verification: VerificationStatus,
}

/// Persistable installed-device identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstalledDeviceIdentity<'a> {
    /// Platform-scoped primary identifier. This is opaque and must not be
    /// assumed to be a stable public Bluetooth MAC address.
    pub platform_id: InstalledDevicePlatformId<'a>,

    /// Optional protocol-reported serial number.
    pub protocol_serial: Option<ProtocolSerial<'a>>,

    /// Optional user-facing alias.
    pub user_alias: Option<&'a str>,

    /// Optional resolved model identity.
    pub resolved_model: Option<InstalledDeviceModel<'a>>,

    /// Observed model/GATT fingerprints.
    pub gatt_fingerprints: &'a [GattFingerprint],
}

/// Data-only model registry entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelRegistryEntry {
    /// Manufacturer or brand.
    pub manufacturer: ManufacturerKey,

    /// Model name.
    pub model: ModelKey,

    /// Protocol family.
    pub protocol_family: ProtocolFamily,

    /// Advertised-name hints. These are hints only, not identity truth.
    pub advertised_name_hints: &'static [&'static str],

    /// Passive wire model id when known.
    pub wire_model_id: Option<VerifiedValue<u16>>,

    /// Battery metadata when known.
    pub battery: Option<BatterySpec>,

    /// BMS layout metadata when known.
    pub bms: Option<BmsLayoutSpec>,

    /// Observed GATT fingerprints.
    pub gatt: &'static [GattFingerprint],

    /// Supported command capabilities.
    pub capabilities: Capabilities,

    /// Overall entry verification status.
    pub verification: VerificationStatus,
}

/// Stable manufacturer key used by catalog lookup and validation.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ManufacturerKey(&'static str);

impl ManufacturerKey {
    /// Builds a manufacturer key from static registry data.
    #[must_use]
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }

    /// Returns the key text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl core::ops::Deref for ManufacturerKey {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl PartialEq<&str> for ManufacturerKey {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<ManufacturerKey> for &str {
    fn eq(&self, other: &ManufacturerKey) -> bool {
        *self == other.as_str()
    }
}

impl core::fmt::Display for ManufacturerKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Stable model key used by catalog lookup and validation.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ModelKey(&'static str);

impl ModelKey {
    /// Builds a model key from static registry data.
    #[must_use]
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }

    /// Returns the key text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl core::ops::Deref for ModelKey {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl PartialEq<&str> for ModelKey {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<ModelKey> for &str {
    fn eq(&self, other: &ModelKey) -> bool {
        *self == other.as_str()
    }
}

impl core::fmt::Display for ModelKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Stable protocol-family key used by catalog lookup and validation.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FamilyKey(ProtocolFamily);

impl FamilyKey {
    /// Builds a family key.
    #[must_use]
    pub const fn new(value: ProtocolFamily) -> Self {
        Self(value)
    }

    /// Returns the protocol family.
    #[must_use]
    pub const fn protocol_family(self) -> ProtocolFamily {
        self.0
    }
}

/// Opaque parser registration key for a registered model.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ParserKey(&'static str);

impl ParserKey {
    /// Builds a parser registration key.
    #[must_use]
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }

    /// Returns the key text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// Opaque session registration key for a registered model.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SessionKey(&'static str);

impl SessionKey {
    /// Builds a session registration key.
    #[must_use]
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }

    /// Returns the key text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// Runtime registrations attached to an active catalog model.
#[derive(Clone, Copy, Debug, Default)]
pub struct ModelRuntimeRegistration {
    /// Parser registration for model notifications/responses.
    pub parser: Option<ParserKey>,

    /// Session registration for model command/session handling.
    pub session: Option<SessionKey>,
}

impl ModelRuntimeRegistration {
    /// Builds an active parser/session registration pair.
    #[must_use]
    pub const fn active(parser: ParserKey, session: SessionKey) -> Self {
        Self {
            parser: Some(parser),
            session: Some(session),
        }
    }
}

/// Type-state marker for a missing required model-authoring field.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MissingAuthoringField;

/// Type-state marker for a present required model-authoring field.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PresentAuthoringField;

/// Type-state model authoring helper for static registry/catalog data.
///
/// This keeps the scalable model path as compile-time Rust data while making
/// the required fields explicit in the type signature. Optional metadata can be
/// layered in functionally, and only a fully-authored model can produce registry
/// and catalog entries.
#[derive(Clone, Debug)]
pub struct ModelAuthoring<
    Manufacturer = MissingAuthoringField,
    Model = MissingAuthoringField,
    Family = MissingAuthoringField,
    Gatt = MissingAuthoringField,
    CapabilitiesState = MissingAuthoringField,
    Runtime = MissingAuthoringField,
> {
    manufacturer: ManufacturerKey,
    model: ModelKey,
    family: FamilyKey,
    advertised_name_hints: &'static [&'static str],
    wire_model_id: Option<VerifiedValue<u16>>,
    battery: Option<BatterySpec>,
    bms: Option<BmsLayoutSpec>,
    gatt: &'static [GattFingerprint],
    capabilities: Capabilities,
    verification: VerificationStatus,
    runtime: ModelRuntimeRegistration,
    _state: PhantomData<(
        Manufacturer,
        Model,
        Family,
        Gatt,
        CapabilitiesState,
        Runtime,
    )>,
}

/// Fully-authored model state that can emit registry and catalog entries.
pub type CompleteModelAuthoring = ModelAuthoring<
    PresentAuthoringField,
    PresentAuthoringField,
    PresentAuthoringField,
    PresentAuthoringField,
    PresentAuthoringField,
    PresentAuthoringField,
>;

impl ModelAuthoring {
    /// Starts authoring a static model definition.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            manufacturer: ManufacturerKey::new(""),
            model: ModelKey::new(""),
            family: FamilyKey::new(ProtocolFamily::VeteranLeaperkimNosfet),
            advertised_name_hints: &[],
            wire_model_id: None,
            battery: None,
            bms: None,
            gatt: &[],
            capabilities: Capabilities::from_supported_commands([]),
            verification: VerificationStatus::Unverified,
            runtime: ModelRuntimeRegistration {
                parser: None,
                session: None,
            },
            _state: PhantomData,
        }
    }
}

impl Default for ModelAuthoring {
    fn default() -> Self {
        Self::new()
    }
}

impl<M, N, F, G, C, R> ModelAuthoring<M, N, F, G, C, R> {
    /// Sets the manufacturer key.
    #[must_use]
    pub const fn manufacturer(
        self,
        manufacturer: ManufacturerKey,
    ) -> ModelAuthoring<PresentAuthoringField, N, F, G, C, R> {
        let Self {
            model,
            family,
            advertised_name_hints,
            wire_model_id,
            battery,
            bms,
            gatt,
            capabilities,
            verification,
            runtime,
            _state: _,
            ..
        } = self;
        ModelAuthoring {
            manufacturer,
            model,
            family,
            advertised_name_hints,
            wire_model_id,
            battery,
            bms,
            gatt,
            capabilities,
            verification,
            runtime,
            _state: PhantomData,
        }
    }

    /// Sets the model key.
    #[must_use]
    pub const fn model(
        self,
        model: ModelKey,
    ) -> ModelAuthoring<M, PresentAuthoringField, F, G, C, R> {
        let Self {
            manufacturer,
            family,
            advertised_name_hints,
            wire_model_id,
            battery,
            bms,
            gatt,
            capabilities,
            verification,
            runtime,
            _state: _,
            ..
        } = self;
        ModelAuthoring {
            manufacturer,
            model,
            family,
            advertised_name_hints,
            wire_model_id,
            battery,
            bms,
            gatt,
            capabilities,
            verification,
            runtime,
            _state: PhantomData,
        }
    }

    /// Sets the protocol family key.
    #[must_use]
    pub const fn family(
        self,
        family: FamilyKey,
    ) -> ModelAuthoring<M, N, PresentAuthoringField, G, C, R> {
        let Self {
            manufacturer,
            model,
            advertised_name_hints,
            wire_model_id,
            battery,
            bms,
            gatt,
            capabilities,
            verification,
            runtime,
            _state: _,
            ..
        } = self;
        ModelAuthoring {
            manufacturer,
            model,
            family,
            advertised_name_hints,
            wire_model_id,
            battery,
            bms,
            gatt,
            capabilities,
            verification,
            runtime,
            _state: PhantomData,
        }
    }

    /// Sets advertised-name hints. These remain hints, not identity truth.
    #[must_use]
    pub const fn advertised_name_hints(self, hints: &'static [&'static str]) -> Self {
        let Self {
            manufacturer,
            model,
            family,
            wire_model_id,
            battery,
            bms,
            gatt,
            capabilities,
            verification,
            runtime,
            _state: state,
            ..
        } = self;
        Self {
            manufacturer,
            model,
            family,
            advertised_name_hints: hints,
            wire_model_id,
            battery,
            bms,
            gatt,
            capabilities,
            verification,
            runtime,
            _state: state,
        }
    }

    /// Sets the passive wire model id.
    #[must_use]
    pub const fn wire_model_id(self, wire_model_id: VerifiedValue<u16>) -> Self {
        let Self {
            manufacturer,
            model,
            family,
            advertised_name_hints,
            battery,
            bms,
            gatt,
            capabilities,
            verification,
            runtime,
            _state: state,
            ..
        } = self;
        Self {
            manufacturer,
            model,
            family,
            advertised_name_hints,
            wire_model_id: Some(wire_model_id),
            battery,
            bms,
            gatt,
            capabilities,
            verification,
            runtime,
            _state: state,
        }
    }

    /// Sets battery metadata.
    #[must_use]
    pub const fn battery(self, battery: BatterySpec) -> Self {
        let Self {
            manufacturer,
            model,
            family,
            advertised_name_hints,
            wire_model_id,
            bms,
            gatt,
            capabilities,
            verification,
            runtime,
            _state: state,
            ..
        } = self;
        Self {
            manufacturer,
            model,
            family,
            advertised_name_hints,
            wire_model_id,
            battery: Some(battery),
            bms,
            gatt,
            capabilities,
            verification,
            runtime,
            _state: state,
        }
    }

    /// Sets BMS layout metadata.
    #[must_use]
    pub const fn bms(self, bms: BmsLayoutSpec) -> Self {
        let Self {
            manufacturer,
            model,
            family,
            advertised_name_hints,
            wire_model_id,
            battery,
            gatt,
            capabilities,
            verification,
            runtime,
            _state: state,
            ..
        } = self;
        Self {
            manufacturer,
            model,
            family,
            advertised_name_hints,
            wire_model_id,
            battery,
            bms: Some(bms),
            gatt,
            capabilities,
            verification,
            runtime,
            _state: state,
        }
    }

    /// Sets observed GATT fingerprints.
    #[must_use]
    pub const fn gatt(
        self,
        gatt: &'static [GattFingerprint],
    ) -> ModelAuthoring<M, N, F, PresentAuthoringField, C, R> {
        let Self {
            manufacturer,
            model,
            family,
            advertised_name_hints,
            wire_model_id,
            battery,
            bms,
            capabilities,
            verification,
            runtime,
            _state: _,
            ..
        } = self;
        ModelAuthoring {
            manufacturer,
            model,
            family,
            advertised_name_hints,
            wire_model_id,
            battery,
            bms,
            gatt,
            capabilities,
            verification,
            runtime,
            _state: PhantomData,
        }
    }

    /// Sets supported command capabilities.
    #[must_use]
    pub const fn capabilities(
        self,
        capabilities: Capabilities,
    ) -> ModelAuthoring<M, N, F, G, PresentAuthoringField, R> {
        let Self {
            manufacturer,
            model,
            family,
            advertised_name_hints,
            wire_model_id,
            battery,
            bms,
            gatt,
            verification,
            runtime,
            _state: _,
            ..
        } = self;
        ModelAuthoring {
            manufacturer,
            model,
            family,
            advertised_name_hints,
            wire_model_id,
            battery,
            bms,
            gatt,
            capabilities,
            verification,
            runtime,
            _state: PhantomData,
        }
    }

    /// Sets the overall verification status.
    #[must_use]
    pub const fn verification(self, verification: VerificationStatus) -> Self {
        let Self {
            manufacturer,
            model,
            family,
            advertised_name_hints,
            wire_model_id,
            battery,
            bms,
            gatt,
            capabilities,
            runtime,
            _state: state,
            ..
        } = self;
        Self {
            manufacturer,
            model,
            family,
            advertised_name_hints,
            wire_model_id,
            battery,
            bms,
            gatt,
            capabilities,
            verification,
            runtime,
            _state: state,
        }
    }

    /// Sets active parser and session runtime registrations.
    #[must_use]
    pub const fn active_runtime(
        self,
        parser: ParserKey,
        session: SessionKey,
    ) -> ModelAuthoring<M, N, F, G, C, PresentAuthoringField> {
        let Self {
            manufacturer,
            model,
            family,
            advertised_name_hints,
            wire_model_id,
            battery,
            bms,
            gatt,
            capabilities,
            verification,
            _state: _,
            ..
        } = self;
        ModelAuthoring {
            manufacturer,
            model,
            family,
            advertised_name_hints,
            wire_model_id,
            battery,
            bms,
            gatt,
            capabilities,
            verification,
            runtime: ModelRuntimeRegistration::active(parser, session),
            _state: PhantomData,
        }
    }
}

impl CompleteModelAuthoring {
    /// Builds a data-only registry entry from a fully-authored model.
    #[must_use]
    pub const fn registry_entry(self) -> ModelRegistryEntry {
        ModelRegistryEntry {
            manufacturer: self.manufacturer,
            model: self.model,
            protocol_family: self.family.protocol_family(),
            advertised_name_hints: self.advertised_name_hints,
            wire_model_id: self.wire_model_id,
            battery: self.battery,
            bms: self.bms,
            gatt: self.gatt,
            capabilities: self.capabilities,
            verification: self.verification,
        }
    }

    /// Builds a catalog entry from a fully-authored model and static registry entry.
    #[must_use]
    pub const fn catalog_entry(self, registry: &'static ModelRegistryEntry) -> ModelCatalogEntry {
        ModelCatalogEntry::new(registry, self.runtime)
    }
}

/// Static catalog entry combining data-only metadata with runtime registration.
#[derive(Clone, Copy, Debug)]
pub struct ModelCatalogEntry {
    /// Data-only registry entry.
    pub registry: &'static ModelRegistryEntry,

    /// Runtime registrations used by hosts/protocol adapters.
    pub registration: ModelRuntimeRegistration,
}

impl ModelCatalogEntry {
    /// Builds a catalog entry from registry metadata and runtime registrations.
    #[must_use]
    pub const fn new(
        registry: &'static ModelRegistryEntry,
        registration: ModelRuntimeRegistration,
    ) -> Self {
        Self {
            registry,
            registration,
        }
    }

    /// Manufacturer key for this catalog entry.
    #[must_use]
    pub const fn manufacturer_key(self) -> ManufacturerKey {
        self.registry.manufacturer
    }

    /// Model key for this catalog entry.
    #[must_use]
    pub const fn model_key(self) -> ModelKey {
        self.registry.model
    }

    /// Family key for this catalog entry.
    #[must_use]
    pub const fn family_key(self) -> FamilyKey {
        FamilyKey::new(self.registry.protocol_family)
    }
}

/// Borrowed model catalog for allocation-free lookup over static entries.
#[derive(Clone, Copy, Debug)]
pub struct ModelCatalog<'a> {
    entries: &'a [ModelCatalogEntry],
}

impl<'a> ModelCatalog<'a> {
    /// Builds a borrowed model catalog.
    #[must_use]
    pub const fn new(entries: &'a [ModelCatalogEntry]) -> Self {
        Self { entries }
    }

    /// Returns the underlying catalog entries.
    #[must_use]
    pub const fn entries(self) -> &'a [ModelCatalogEntry] {
        self.entries
    }

    /// Finds an entry by typed manufacturer/model keys.
    #[must_use]
    pub fn find_model(
        self,
        manufacturer: ManufacturerKey,
        model: ModelKey,
    ) -> Option<&'a ModelCatalogEntry> {
        self.find_model_names(manufacturer.as_str(), model.as_str())
    }

    /// Finds an entry by borrowed manufacturer/model names.
    #[must_use]
    pub fn find_model_names(
        self,
        manufacturer: &str,
        model: &str,
    ) -> Option<&'a ModelCatalogEntry> {
        self.entries.iter().find(|entry| {
            entry.registry.manufacturer == manufacturer && entry.registry.model == model
        })
    }

    /// Resolves a display model name to a catalog entry within a protocol family.
    #[must_use]
    pub fn resolve_display_model(
        self,
        family: ProtocolFamily,
        display_model: &str,
    ) -> CatalogModelResolution<'a> {
        let mut matches = self
            .entries
            .iter()
            .filter(|entry| entry.registry.protocol_family == family)
            .filter(|entry| registry_entry_matches_display_model(entry.registry, display_model));
        let Some(first) = matches.next() else {
            return CatalogModelResolution::NoMatch;
        };
        if matches.next().is_some() {
            CatalogModelResolution::Ambiguous
        } else {
            CatalogModelResolution::Matched(first)
        }
    }

    /// Resolves a BLE advertised name against catalog model hints.
    #[must_use]
    pub fn resolve_advertised_name(self, name: &str) -> CatalogModelResolution<'a> {
        let mut matches = self
            .entries
            .iter()
            .filter(|entry| registry_entry_matches_advertised_name(entry.registry, name));
        let Some(first) = matches.next() else {
            return CatalogModelResolution::NoMatch;
        };
        if matches.next().is_some() {
            CatalogModelResolution::Ambiguous
        } else {
            CatalogModelResolution::Matched(first)
        }
    }

    /// Finds the first catalog entry registered for a parser key.
    #[must_use]
    pub fn find_parser(self, parser: ParserKey) -> Option<&'a ModelCatalogEntry> {
        self.entries
            .iter()
            .find(|entry| entry.registration.parser == Some(parser))
    }

    /// Finds the first catalog entry registered for a session key.
    #[must_use]
    pub fn find_session(self, session: SessionKey) -> Option<&'a ModelCatalogEntry> {
        self.entries
            .iter()
            .find(|entry| entry.registration.session == Some(session))
    }

    /// Iterates entries for a protocol family without allocating.
    pub fn family_entries(
        self,
        family: FamilyKey,
    ) -> impl Clone + Iterator<Item = &'a ModelCatalogEntry> {
        self.entries
            .iter()
            .filter(move |entry| entry.family_key() == family)
    }
}

/// Result of resolving identity metadata against a model catalog.
#[derive(Clone, Copy, Debug)]
pub enum CatalogModelResolution<'a> {
    /// Exactly one catalog entry matched.
    Matched(&'a ModelCatalogEntry),

    /// No catalog entry matched.
    NoMatch,

    /// More than one catalog entry matched.
    Ambiguous,
}

fn registry_entry_matches_display_model(entry: &ModelRegistryEntry, display_model: &str) -> bool {
    entry.model == display_model
        || display_model
            .strip_prefix(entry.manufacturer.as_str())
            .and_then(|suffix| suffix.strip_prefix(' '))
            == Some(entry.model.as_str())
}

fn registry_entry_matches_advertised_name(entry: &ModelRegistryEntry, name: &str) -> bool {
    contains_ascii_ignore_case(name, entry.manufacturer.as_str())
        || contains_ascii_ignore_case(name, entry.model.as_str())
        || entry
            .advertised_name_hints
            .iter()
            .copied()
            .any(|hint| contains_ascii_ignore_case(name, hint))
}

fn contains_ascii_ignore_case(haystack: &str, needle: &str) -> bool {
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();

    !needle.is_empty()
        && haystack.len() >= needle.len()
        && haystack
            .windows(needle.len())
            .any(|window| ascii_eq_ignore_case(window, needle))
}

fn ascii_eq_ignore_case(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
}

/// Registry data validation error.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum RegistryValidationError {
    /// Registry entry has an empty manufacturer.
    #[error("registry entry at index {index} has an empty manufacturer")]
    EmptyManufacturer {
        /// Entry index in the validated slice.
        index: usize,
    },

    /// Registry entry has an empty model.
    #[error("registry entry at index {index} has an empty model")]
    EmptyModel {
        /// Entry index in the validated slice.
        index: usize,
    },

    /// Registry entry duplicates an earlier manufacturer/model key.
    #[error("registry entry at index {index} duplicates entry at index {first_index}")]
    DuplicateModel {
        /// Duplicate entry index.
        index: usize,

        /// First entry index with the same manufacturer/model key.
        first_index: usize,
    },

    /// Registry entry duplicates a wire model id in the same protocol family.
    #[error(
        "registry entry at index {index} conflicts with entry at index {first_index} for a protocol wire model id"
    )]
    ConflictingWireModelId {
        /// Conflicting entry index.
        index: usize,

        /// First entry index with the same family and wire model id.
        first_index: usize,
    },

    /// Registry entry has no observed GATT fingerprints.
    #[error("registry entry at index {index} has no GATT fingerprints")]
    MissingGattFingerprint {
        /// Entry index in the validated slice.
        index: usize,
    },

    /// Registry entry has a GATT fingerprint with no characteristic roles.
    #[error(
        "registry entry at index {index} has invalid GATT fingerprint at index {fingerprint_index}"
    )]
    InvalidGattFingerprint {
        /// Entry index in the validated slice.
        index: usize,

        /// GATT fingerprint index in the entry.
        fingerprint_index: usize,
    },

    /// Registry entry has a GATT fingerprint whose service and characteristic are identical.
    #[error(
        "registry entry at index {index} has identical service and characteristic UUIDs in GATT fingerprint at index {fingerprint_index}"
    )]
    EqualGattServiceAndCharacteristic {
        /// Entry index in the validated slice.
        index: usize,

        /// GATT fingerprint index in the entry.
        fingerprint_index: usize,
    },

    /// Registry entry exposes no supported commands.
    #[error("registry entry at index {index} exposes no command capabilities")]
    EmptyCapabilities {
        /// Entry index in the validated slice.
        index: usize,
    },

    /// Active catalog entry has no parser registration.
    #[error("catalog entry at index {index} has active capabilities but no parser registration")]
    MissingParserRegistration {
        /// Entry index in the validated slice.
        index: usize,
    },

    /// Active catalog entry has no session registration.
    #[error("catalog entry at index {index} has active capabilities but no session registration")]
    MissingSessionRegistration {
        /// Entry index in the validated slice.
        index: usize,
    },
}

/// Validates registry entries as data before they are bundled, hashed, or used
/// for model identification.
///
/// # Errors
///
/// Returns [`RegistryValidationError`] for the first structural inconsistency
/// found in the supplied entries.
pub fn validate_registry_entries(
    entries: &[&ModelRegistryEntry],
) -> Result<(), RegistryValidationError> {
    for (index, entry) in entries.iter().enumerate() {
        validate_registry_entry(index, entry)?;
        if let Some(first_index) = first_duplicate_model_index(entries, index, entry) {
            return Err(RegistryValidationError::DuplicateModel { index, first_index });
        }
        if let Some(first_index) = first_conflicting_wire_model_id_index(entries, index, entry) {
            return Err(RegistryValidationError::ConflictingWireModelId { index, first_index });
        }
    }
    Ok(())
}

/// Validates catalog entries before hosts use registry metadata or factories.
///
/// # Errors
///
/// Returns [`RegistryValidationError`] for the first structural inconsistency
/// found in the supplied entries.
pub fn validate_model_catalog(
    entries: &[ModelCatalogEntry],
) -> Result<(), RegistryValidationError> {
    for (index, entry) in entries.iter().enumerate() {
        validate_registry_entry(index, entry.registry)?;
        if entry.registration.parser.is_none() {
            return Err(RegistryValidationError::MissingParserRegistration { index });
        }
        if entry.registration.session.is_none() {
            return Err(RegistryValidationError::MissingSessionRegistration { index });
        }
        if let Some(first_index) = first_duplicate_catalog_model_index(entries, index, entry) {
            return Err(RegistryValidationError::DuplicateModel { index, first_index });
        }
        if let Some(first_index) =
            first_conflicting_catalog_wire_model_id_index(entries, index, entry)
        {
            return Err(RegistryValidationError::ConflictingWireModelId { index, first_index });
        }
    }
    Ok(())
}

/// Deterministic fingerprint for a registry snapshot.
///
/// This is intended for capture provenance and replay compatibility checks. It
/// is not a cryptographic authenticity mechanism.
#[must_use]
pub fn registry_entries_hash(entries: &[&ModelRegistryEntry]) -> [u8; 32] {
    let mut hasher = RegistryHashBuilder::new();
    hasher.write_bytes(b"cutout-registry-v1");
    hasher.write_usize(entries.len());
    for entry in entries {
        hasher.write_registry_entry(entry);
    }
    hasher.finish()
}

fn validate_registry_entry(
    index: usize,
    entry: &ModelRegistryEntry,
) -> Result<(), RegistryValidationError> {
    if entry.manufacturer.as_str().is_empty() {
        return Err(RegistryValidationError::EmptyManufacturer { index });
    }
    if entry.model.as_str().is_empty() {
        return Err(RegistryValidationError::EmptyModel { index });
    }
    if entry.gatt.is_empty() {
        return Err(RegistryValidationError::MissingGattFingerprint { index });
    }
    if let Some(fingerprint_index) = first_invalid_gatt_fingerprint_index(entry.gatt) {
        return Err(RegistryValidationError::InvalidGattFingerprint {
            index,
            fingerprint_index,
        });
    }
    if let Some(fingerprint_index) = first_equal_gatt_service_characteristic_index(entry.gatt) {
        return Err(RegistryValidationError::EqualGattServiceAndCharacteristic {
            index,
            fingerprint_index,
        });
    }
    if capabilities_are_empty(entry.capabilities) {
        return Err(RegistryValidationError::EmptyCapabilities { index });
    }
    Ok(())
}

fn first_duplicate_model_index(
    entries: &[&ModelRegistryEntry],
    index: usize,
    entry: &ModelRegistryEntry,
) -> Option<usize> {
    entries[..index].iter().position(|candidate| {
        candidate.manufacturer == entry.manufacturer && candidate.model == entry.model
    })
}

fn first_conflicting_wire_model_id_index(
    entries: &[&ModelRegistryEntry],
    index: usize,
    entry: &ModelRegistryEntry,
) -> Option<usize> {
    let wire_model_id = entry.wire_model_id?.value;
    entries[..index].iter().position(|candidate| {
        candidate.protocol_family == entry.protocol_family
            && candidate
                .wire_model_id
                .is_some_and(|candidate_id| candidate_id.value == wire_model_id)
    })
}

fn first_duplicate_catalog_model_index(
    entries: &[ModelCatalogEntry],
    index: usize,
    entry: &ModelCatalogEntry,
) -> Option<usize> {
    entries[..index].iter().position(|candidate| {
        candidate.registry.manufacturer == entry.registry.manufacturer
            && candidate.registry.model == entry.registry.model
    })
}

fn first_conflicting_catalog_wire_model_id_index(
    entries: &[ModelCatalogEntry],
    index: usize,
    entry: &ModelCatalogEntry,
) -> Option<usize> {
    let wire_model_id = entry.registry.wire_model_id?.value;
    entries[..index].iter().position(|candidate| {
        candidate.registry.protocol_family == entry.registry.protocol_family
            && candidate
                .registry
                .wire_model_id
                .is_some_and(|candidate_id| candidate_id.value == wire_model_id)
    })
}

fn capabilities_are_empty(capabilities: Capabilities) -> bool {
    ALL_COMMAND_KINDS
        .iter()
        .all(|command| !capabilities.supports_command_kind(*command))
}

fn first_invalid_gatt_fingerprint_index(gatt: &[GattFingerprint]) -> Option<usize> {
    gatt.iter()
        .position(|fingerprint| fingerprint.roles.is_empty())
}

fn first_equal_gatt_service_characteristic_index(gatt: &[GattFingerprint]) -> Option<usize> {
    gatt.iter()
        .position(|fingerprint| fingerprint.service == fingerprint.characteristic)
}

struct RegistryHashBuilder {
    lanes: [u64; 4],
}

impl RegistryHashBuilder {
    const fn new() -> Self {
        Self {
            lanes: [
                0xcbf2_9ce4_8422_2325,
                0x9e37_79b9_7f4a_7c15,
                0x517c_c1b7_2722_0a95,
                0x94d0_49bb_1331_11eb,
            ],
        }
    }

    fn finish(self) -> [u8; 32] {
        let mut output = [0u8; 32];
        for (index, lane) in self.lanes.into_iter().enumerate() {
            let start = index * 8;
            output[start..start + 8].copy_from_slice(&lane.to_le_bytes());
        }
        output
    }

    fn write_registry_entry(&mut self, entry: &ModelRegistryEntry) {
        self.write_str(entry.manufacturer.as_str());
        self.write_str(entry.model.as_str());
        self.write_u8(protocol_family_code(entry.protocol_family));
        self.write_strs(entry.advertised_name_hints);
        self.write_verified_u16(entry.wire_model_id);
        self.write_battery(entry.battery.as_ref());
        self.write_bms(entry.bms.as_ref());
        self.write_gatt(entry.gatt);
        self.write_capabilities(entry.capabilities);
        self.write_u8(verification_code(entry.verification));
    }

    fn write_strs(&mut self, values: &[&str]) {
        self.write_usize(values.len());
        for value in values {
            self.write_str(value);
        }
    }

    fn write_verified_u16(&mut self, value: Option<VerifiedValue<u16>>) {
        match value {
            Some(value) => {
                self.write_u8(1);
                self.write_u16(value.value);
                self.write_u8(verification_code(value.verification));
            }
            None => self.write_u8(0),
        }
    }

    fn write_battery(&mut self, battery: Option<&BatterySpec>) {
        match battery {
            Some(battery) => {
                self.write_u8(1);
                self.write_u8(battery.series_cells.get());
                self.write_optional_u32(battery.nominal_capacity.map(Capacity::as_milliamp_hours));
                self.write_i32(battery.voltage_range.start().as_millivolts());
                self.write_i32(battery.voltage_range.end().as_millivolts());
                self.write_u8(verification_code(battery.verification));
            }
            None => self.write_u8(0),
        }
    }

    fn write_bms(&mut self, bms: Option<&BmsLayoutSpec>) {
        match bms {
            Some(bms) => {
                self.write_u8(1);
                self.write_u8(bms.series_cells.get());
                self.write_u8(bms.parallel_packs.get());
                self.write_u8(bms.cell_values_per_page.get());
                self.write_u8(bms.temperature_values_per_page.get());
                self.write_usize(bms.selectors.len());
                for selector in bms.selectors {
                    self.write_u8(selector.selector.get());
                    self.write_u8(battery_page_kind_code(selector.kind));
                    self.write_u8(verification_code(selector.verification));
                }
                self.write_u8(verification_code(bms.verification));
            }
            None => self.write_u8(0),
        }
    }

    fn write_gatt(&mut self, fingerprints: &[GattFingerprint]) {
        self.write_usize(fingerprints.len());
        for fingerprint in fingerprints {
            self.write_bytes(&fingerprint.service.as_bytes());
            self.write_bytes(&fingerprint.characteristic.as_bytes());
            self.write_u8(gatt_roles_code(fingerprint.roles));
            self.write_u8(verification_code(fingerprint.verification));
        }
    }

    fn write_capabilities(&mut self, capabilities: Capabilities) {
        for command in ALL_COMMAND_KINDS {
            self.write_u8(u8::from(capabilities.supports_command_kind(command)));
        }
    }

    fn write_optional_u32(&mut self, value: Option<u32>) {
        match value {
            Some(value) => {
                self.write_u8(1);
                self.write_u32(value);
            }
            None => self.write_u8(0),
        }
    }

    fn write_str(&mut self, value: &str) {
        self.write_usize(value.len());
        self.write_bytes(value.as_bytes());
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(u64::try_from(value).unwrap_or(u64::MAX));
    }

    fn write_u64(&mut self, value: u64) {
        self.write_bytes(&value.to_le_bytes());
    }

    fn write_u32(&mut self, value: u32) {
        self.write_bytes(&value.to_le_bytes());
    }

    fn write_i32(&mut self, value: i32) {
        self.write_bytes(&value.to_le_bytes());
    }

    fn write_u16(&mut self, value: u16) {
        self.write_bytes(&value.to_le_bytes());
    }

    fn write_u8(&mut self, value: u8) {
        self.write_bytes(&[value]);
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            for (lane_index, lane) in self.lanes.iter_mut().enumerate() {
                let lane_index_u64 = u64::try_from(lane_index).unwrap_or_default();
                let lane_index_u32 = u32::try_from(lane_index).unwrap_or_default();
                *lane ^= u64::from(*byte).wrapping_add(lane_index_u64 << 8);
                *lane = lane.wrapping_mul(0x0000_0100_0000_01b3 + lane_index_u64);
                *lane ^= lane.rotate_left(17 + lane_index_u32);
            }
        }
    }
}

const ALL_COMMAND_KINDS: [CommandKind; 9] = [
    CommandKind::RequestIdentity,
    CommandKind::RequestTelemetry,
    CommandKind::RequestFirmwareInfo,
    CommandKind::RequestBatteryInfo,
    CommandKind::RequestDiagnostics,
    CommandKind::RequestSettings,
    CommandKind::SetLights,
    CommandKind::SoundHorn,
    CommandKind::SetRawMotorCurrent,
];

const fn protocol_family_code(family: ProtocolFamily) -> u8 {
    match family {
        ProtocolFamily::VeteranLeaperkimNosfet => 1,
        ProtocolFamily::BegodeGotway => 2,
        ProtocolFamily::Vesc => 3,
    }
}

const fn verification_code(verification: VerificationStatus) -> u8 {
    match verification {
        VerificationStatus::Unverified => 0,
        VerificationStatus::Inferred => 1,
        VerificationStatus::SourceVerified => 2,
        VerificationStatus::HardwareVerified => 3,
        VerificationStatus::SourceAndHardwareVerified => 4,
    }
}

const fn gatt_roles_code(roles: GattRoles) -> u8 {
    let mut bits = 0u8;
    if roles.supports_read() {
        bits |= 1 << 0;
    }
    if roles.supports_write() {
        bits |= 1 << 1;
    }
    if roles.supports_write_without_response() {
        bits |= 1 << 2;
    }
    if roles.supports_notify() {
        bits |= 1 << 3;
    }
    if roles.supports_indicate() {
        bits |= 1 << 4;
    }
    bits
}

const fn battery_page_kind_code(kind: BatteryPageKind) -> u8 {
    match kind {
        BatteryPageKind::Metadata => 1,
        BatteryPageKind::CellVoltage => 2,
        BatteryPageKind::Temperature => 3,
        BatteryPageKind::Raw => 4,
    }
}

/// Current command capabilities for a resolved device/session.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Capabilities {
    supported_commands: CommandSet,
}

impl Capabilities {
    /// Creates capabilities from supported command kinds.
    #[must_use]
    pub const fn from_supported_commands<const N: usize>(commands: [CommandKind; N]) -> Self {
        Self {
            supported_commands: CommandSet::from_commands(commands),
        }
    }

    /// Returns whether the command kind is supported.
    #[must_use]
    pub const fn supports_command_kind(self, kind: CommandKind) -> bool {
        self.supported_commands.contains(kind)
    }

    /// Checks whether a command is supported and returns metadata for it.
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedReason::CommandNotSupported`] when the command kind
    /// is absent from this capability set.
    pub const fn check_command(
        self,
        command: DeviceCommand,
    ) -> Result<CommandMetadata, UnsupportedReason> {
        let kind = command.kind();
        if self.supports_command_kind(kind) {
            Ok(command.metadata())
        } else {
            Err(UnsupportedReason::CommandNotSupported(kind))
        }
    }
}

/// Compact command-kind set.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CommandSet(u16);

impl CommandSet {
    const fn from_commands<const N: usize>(commands: [CommandKind; N]) -> Self {
        let mut set = Self(0);
        let mut index = 0;
        while index < N {
            set = set.insert(commands[index]);
            index += 1;
        }
        set
    }

    const fn insert(self, kind: CommandKind) -> Self {
        Self(self.0 | kind.bit())
    }

    const fn contains(self, kind: CommandKind) -> bool {
        self.0 & kind.bit() != 0
    }
}

impl CommandKind {
    const fn bit(self) -> u16 {
        1 << (self as u16)
    }
}
