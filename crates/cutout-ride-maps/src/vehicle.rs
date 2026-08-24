//! Vehicle identity value objects.

/// A non-empty platform identity for a connected vehicle.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VehicleIdentity(String);

/// Error returned when a vehicle identity is empty after trimming.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VehicleIdentityError;

impl std::fmt::Display for VehicleIdentityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("vehicle identity must not be empty")
    }
}

impl std::error::Error for VehicleIdentityError {}

impl VehicleIdentity {
    /// Creates an identity after trimming surrounding whitespace.
    #[must_use]
    pub fn new(value: impl AsRef<str>) -> Option<Self> {
        let value = value.as_ref().trim();
        (!value.is_empty()).then(|| Self(value.to_owned()))
    }

    /// Returns the platform identity string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for VehicleIdentity {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<VehicleIdentity> for String {
    fn from(identity: VehicleIdentity) -> Self {
        identity.0
    }
}

impl TryFrom<&str> for VehicleIdentity {
    type Error = VehicleIdentityError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(VehicleIdentityError)
    }
}
