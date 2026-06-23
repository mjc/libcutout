use std::fmt;

use crate::PeripheralObservation;

/// Bluetooth address text after platform placeholder normalization.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct BluetoothAddress<'a>(&'a str);

/// Error returned when the platform address is only the null placeholder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NullBluetoothAddress;

impl<'a> BluetoothAddress<'a> {
    /// Creates a typed address unless the platform reported the null placeholder.
    #[must_use]
    pub fn new(value: &'a str) -> Option<Self> {
        (value != "00:00:00:00:00:00").then_some(Self(value))
    }

    /// Returns the normalized address text.
    #[must_use]
    pub const fn as_str(&self) -> &'a str {
        self.0
    }

    /// Returns the borrowed normalized address text.
    #[must_use]
    pub const fn into_inner(self) -> &'a str {
        self.0
    }
}

impl fmt::Display for BluetoothAddress<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl<'a> TryFrom<&'a str> for BluetoothAddress<'a> {
    type Error = NullBluetoothAddress;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(NullBluetoothAddress)
    }
}

/// Target used to select a peripheral from scan results.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConnectionTarget {
    /// Match against the peripheral address, when provided.
    pub address: Option<String>,

    /// Match against the platform-specific peripheral identifier.
    pub identifier: Option<String>,

    /// Match against the peripheral local name, when provided.
    pub name_contains: Option<String>,
}

impl ConnectionTarget {
    /// Returns whether an observation matches this target.
    #[must_use]
    pub fn matches(&self, observation: &PeripheralObservation) -> bool {
        [
            self.address
                .as_ref()
                .is_none_or(|address| observation.address.as_deref() == Some(address.as_str())),
            self.identifier
                .as_ref()
                .is_none_or(|identifier| observation.identifier == *identifier),
            self.name_contains.as_ref().is_none_or(|needle| {
                observation
                    .name
                    .as_deref()
                    .is_some_and(|name| name.contains(needle))
            }),
        ]
        .into_iter()
        .all(core::convert::identity)
    }
}

pub(crate) fn normalize_address(address: String) -> Option<String> {
    BluetoothAddress::new(&address).is_some().then_some(address)
}
