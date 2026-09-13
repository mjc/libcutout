//! Thin platform location observation and acquisition projection bindings.

use crate::{MobileRideMapCore, MobileRideMapCoreDecisionDto, MobileRideMapCoreErrorDto};
use libcutout_persistence::{
    LocationAcquisition, LocationAuthorization, LocationAvailability, LocationDemand,
    LocationEnvironment,
};
use std::sync::PoisonError;

/// Mobile representation of the recording owner's LocationAuthorization values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileLocationAuthorizationDto {
    /// Corresponding LocationAuthorization observation or intent.
    NotDetermined,
    /// Corresponding LocationAuthorization observation or intent.
    Denied,
    /// Corresponding LocationAuthorization observation or intent.
    Restricted,
    /// Corresponding LocationAuthorization observation or intent.
    WhenInUse,
    /// Corresponding LocationAuthorization observation or intent.
    Always,
}

impl From<MobileLocationAuthorizationDto> for LocationAuthorization {
    fn from(value: MobileLocationAuthorizationDto) -> Self {
        match value {
            MobileLocationAuthorizationDto::NotDetermined => Self::NotDetermined,
            MobileLocationAuthorizationDto::Denied => Self::Denied,
            MobileLocationAuthorizationDto::Restricted => Self::Restricted,
            MobileLocationAuthorizationDto::WhenInUse => Self::WhenInUse,
            MobileLocationAuthorizationDto::Always => Self::Always,
        }
    }
}

/// Mobile representation of the recording owner's LocationAvailability values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileRideMapAvailabilityDto {
    /// Corresponding LocationAvailability observation or intent.
    Checking,
    /// Corresponding LocationAvailability observation or intent.
    Ready,
    /// Corresponding LocationAvailability observation or intent.
    PermissionRequired,
    /// Corresponding LocationAvailability observation or intent.
    Denied,
    /// Corresponding LocationAvailability observation or intent.
    Restricted,
    /// Corresponding LocationAvailability observation or intent.
    ServicesDisabled,
    /// Corresponding LocationAvailability observation or intent.
    LocationUnavailable,
    /// Corresponding LocationAvailability observation or intent.
    StorageUnavailable,
}

impl From<LocationAvailability> for MobileRideMapAvailabilityDto {
    fn from(value: LocationAvailability) -> Self {
        match value {
            LocationAvailability::Checking => Self::Checking,
            LocationAvailability::Ready => Self::Ready,
            LocationAvailability::PermissionRequired => Self::PermissionRequired,
            LocationAvailability::Denied => Self::Denied,
            LocationAvailability::Restricted => Self::Restricted,
            LocationAvailability::ServicesDisabled => Self::ServicesDisabled,
            LocationAvailability::TemporarilyUnavailable => Self::LocationUnavailable,
            LocationAvailability::StorageUnavailable => Self::StorageUnavailable,
        }
    }
}

/// Mobile representation of the recording owner's LocationDemand values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileLocationDemandDto {
    /// Corresponding LocationDemand observation or intent.
    Idle,
    /// Corresponding LocationDemand observation or intent.
    RequestPermission,
    /// Corresponding LocationDemand observation or intent.
    Record,
}

impl From<LocationDemand> for MobileLocationDemandDto {
    fn from(value: LocationDemand) -> Self {
        match value {
            LocationDemand::Idle => Self::Idle,
            LocationDemand::RequestPermission => Self::RequestPermission,
            LocationDemand::Record => Self::Record,
        }
    }
}

/// Native observations supplied without lifecycle policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileLocationEnvironmentDto {
    /// Current location authorization.
    pub authorization: MobileLocationAuthorizationDto,
    /// System location-service availability.
    pub services_enabled: bool,
    /// Recoverable provider failure is currently reported.
    pub temporarily_unavailable: bool,
}
impl From<MobileLocationEnvironmentDto> for LocationEnvironment {
    fn from(value: MobileLocationEnvironmentDto) -> Self {
        Self {
            authorization: value.authorization.into(),
            services_enabled: value.services_enabled,
            temporarily_unavailable: value.temporarily_unavailable,
        }
    }
}

/// Immutable native acquisition work from the existing recording owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileLocationAcquisitionDto {
    /// Revision shared with the recording snapshot.
    pub revision: u64,
    /// Current location readiness or failure reason.
    pub availability: MobileRideMapAvailabilityDto,
    /// Native update/prompt work requested by Rust.
    pub demand: MobileLocationDemandDto,
}
impl From<LocationAcquisition> for MobileLocationAcquisitionDto {
    fn from(value: LocationAcquisition) -> Self {
        Self {
            revision: value.revision,
            availability: value.availability.into(),
            demand: value.demand.into(),
        }
    }
}

#[uniffi::export]
impl MobileRideMapCore {
    /// Supplies platform observations and returns the resulting owner projection.
    pub fn observe_location_environment(
        &self,
        environment: MobileLocationEnvironmentDto,
    ) -> MobileLocationAcquisitionDto {
        let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        state.observe_location_environment(environment.into());
        state.location_acquisition().into()
    }

    /// Forwards successful explicit diagnostic writer lifetime observations.
    pub fn observe_diagnostic_capture_location(
        &self,
        generation: u64,
        active: bool,
    ) -> MobileLocationAcquisitionDto {
        let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        state.observe_diagnostic_capture_location(generation, active);
        state.location_acquisition().into()
    }

    /// Settles preceding recording writes through the existing persistence owner.
    /// # Errors
    /// Returns the owner's storage failure if the checkpoint cannot complete.
    pub fn checkpoint(
        &self,
    ) -> Result<Vec<MobileRideMapCoreDecisionDto>, MobileRideMapCoreErrorDto> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .checkpoint()
            .map(|decisions| decisions.into_iter().map(Into::into).collect())
            .map_err(Into::into)
    }

    /// Returns acquisition intent even when no ride exists yet.
    pub fn location_acquisition(&self) -> MobileLocationAcquisitionDto {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .location_acquisition()
            .into()
    }
}
