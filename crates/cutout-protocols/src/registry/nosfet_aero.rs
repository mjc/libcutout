use cutout_core::ModelRegistryEntry;

use super::RegisteredModelDefinition;
use crate::{NOSFET_AERO_SESSION_KEY, NosfetAeroModel, RegisteredModelSpec, VETERAN_PARSER_KEY};

/// Structured source data for the NOSFET Aero registry entry.
pub const NOSFET_AERO_MODEL_DEFINITION: RegisteredModelDefinition = RegisteredModelDefinition::new(
    &<NosfetAeroModel as RegisteredModelSpec>::REGISTRY_ENTRY,
    VETERAN_PARSER_KEY,
    NOSFET_AERO_SESSION_KEY,
    crate::VETERAN_DATA_CHANNEL,
    super::nosfet_aero_session,
);

/// Hardware-backed registry entry for the NOSFET Aero.
pub const NOSFET_AERO_REGISTRY_ENTRY: ModelRegistryEntry =
    <NosfetAeroModel as RegisteredModelSpec>::REGISTRY_ENTRY;
