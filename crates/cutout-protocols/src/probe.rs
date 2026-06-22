use cutout_core::CommandKind;

use crate::{DeviceFamily, RequestFixtureError};

/// NOSFET/Veteran-family read-only probe identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AeroProbe {
    /// Request identity/model information.
    Identity,

    /// Request firmware or protocol version information.
    FirmwareInfo,

    /// Request live telemetry.
    Telemetry,

    /// Request battery or BMS information.
    BatteryInfo,

    /// Request diagnostic information.
    Diagnostics,
}

impl AeroProbe {
    /// Maps a generic command kind to an Aero/Veteran probe.
    #[must_use]
    pub const fn from_command_kind(kind: CommandKind) -> Option<Self> {
        match kind {
            CommandKind::RequestIdentity => Some(Self::Identity),
            CommandKind::RequestFirmwareInfo => Some(Self::FirmwareInfo),
            CommandKind::RequestTelemetry => Some(Self::Telemetry),
            CommandKind::RequestBatteryInfo => Some(Self::BatteryInfo),
            CommandKind::RequestDiagnostics => Some(Self::Diagnostics),
            CommandKind::RequestSettings
            | CommandKind::SetLights
            | CommandKind::SoundHorn
            | CommandKind::SetRawMotorCurrent => None,
        }
    }

    /// Returns the generic command kind correlated with this probe.
    #[must_use]
    pub const fn command_kind(self) -> CommandKind {
        match self {
            Self::Identity => CommandKind::RequestIdentity,
            Self::FirmwareInfo => CommandKind::RequestFirmwareInfo,
            Self::Telemetry => CommandKind::RequestTelemetry,
            Self::BatteryInfo => CommandKind::RequestBatteryInfo,
            Self::Diagnostics => CommandKind::RequestDiagnostics,
        }
    }
}

/// Begode-family read-only probe identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FalconProbe {
    /// Request identity/model information.
    Identity,

    /// Request firmware or protocol version information.
    FirmwareInfo,

    /// Request live telemetry.
    Telemetry,

    /// Request battery or BMS information.
    BatteryInfo,
}

impl FalconProbe {
    /// Maps a generic command kind to a Begode/Falcon probe.
    #[must_use]
    pub const fn from_command_kind(kind: CommandKind) -> Option<Self> {
        match kind {
            CommandKind::RequestIdentity => Some(Self::Identity),
            CommandKind::RequestFirmwareInfo => Some(Self::FirmwareInfo),
            CommandKind::RequestTelemetry => Some(Self::Telemetry),
            CommandKind::RequestBatteryInfo => Some(Self::BatteryInfo),
            CommandKind::RequestDiagnostics
            | CommandKind::RequestSettings
            | CommandKind::SetLights
            | CommandKind::SoundHorn
            | CommandKind::SetRawMotorCurrent => None,
        }
    }

    /// Returns the temporary placeholder label for this probe.
    #[must_use]
    pub const fn placeholder_label(self) -> &'static [u8] {
        match self {
            Self::Identity => b"falcon:identity",
            Self::FirmwareInfo => b"falcon:firmware",
            Self::Telemetry => b"falcon:telemetry",
            Self::BatteryInfo => b"falcon:battery",
        }
    }

    /// Returns the generic command kind correlated with this probe.
    #[must_use]
    pub const fn command_kind(self) -> CommandKind {
        match self {
            Self::Identity => CommandKind::RequestIdentity,
            Self::FirmwareInfo => CommandKind::RequestFirmwareInfo,
            Self::Telemetry => CommandKind::RequestTelemetry,
            Self::BatteryInfo => CommandKind::RequestBatteryInfo,
        }
    }
}

/// Family-specific request probe used by fixture records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolProbe {
    /// NOSFET/Veteran-family probe.
    Aero(AeroProbe),

    /// Begode/Falcon-family probe.
    Falcon(FalconProbe),
}

impl ProtocolProbe {
    /// Maps a device family and generic command kind into a protocol probe.
    ///
    /// # Errors
    ///
    /// Returns [`RequestFixtureError::UnsupportedCommand`] when the command is
    /// unsupported by the selected family.
    pub fn from_family_command(
        family: DeviceFamily,
        command: CommandKind,
    ) -> Result<Self, RequestFixtureError> {
        match family {
            DeviceFamily::NosfetAero => AeroProbe::from_command_kind(command)
                .map(Self::Aero)
                .ok_or(RequestFixtureError::UnsupportedCommand { family, command }),
            DeviceFamily::BegodeFalcon => FalconProbe::from_command_kind(command)
                .map(Self::Falcon)
                .ok_or(RequestFixtureError::UnsupportedCommand { family, command }),
        }
    }

    /// Returns the family that owns this probe.
    #[must_use]
    pub const fn family(self) -> DeviceFamily {
        match self {
            Self::Aero(_) => DeviceFamily::NosfetAero,
            Self::Falcon(_) => DeviceFamily::BegodeFalcon,
        }
    }

    /// Returns the generic command kind correlated with this probe.
    #[must_use]
    pub const fn command_kind(self) -> CommandKind {
        match self {
            Self::Aero(probe) => probe.command_kind(),
            Self::Falcon(probe) => probe.command_kind(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DeviceFamily;

    #[test]
    fn probe_maps_supported_commands_and_rejects_unsupported_ones() {
        assert_eq!(
            ProtocolProbe::from_family_command(
                DeviceFamily::NosfetAero,
                CommandKind::RequestTelemetry,
            ),
            Ok(ProtocolProbe::Aero(AeroProbe::Telemetry))
        );
        assert_eq!(
            ProtocolProbe::from_family_command(
                DeviceFamily::BegodeFalcon,
                CommandKind::RequestIdentity
            ),
            Ok(ProtocolProbe::Falcon(FalconProbe::Identity))
        );
        assert!(matches!(
            ProtocolProbe::from_family_command(DeviceFamily::NosfetAero, CommandKind::SetLights),
            Err(RequestFixtureError::UnsupportedCommand {
                family: DeviceFamily::NosfetAero,
                command: CommandKind::SetLights,
            })
        ));
        assert!(matches!(
            ProtocolProbe::from_family_command(
                DeviceFamily::BegodeFalcon,
                CommandKind::RequestDiagnostics
            ),
            Err(RequestFixtureError::UnsupportedCommand {
                family: DeviceFamily::BegodeFalcon,
                command: CommandKind::RequestDiagnostics,
            })
        ));
    }
}
