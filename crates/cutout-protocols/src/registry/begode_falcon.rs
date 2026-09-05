use cutout_core::ModelRegistryEntry;

use super::RegisteredModelDefinition;
use crate::{BEGODE_FALCON_SESSION_KEY, BEGODE_PARSER_KEY, BegodeFalconModel, RegisteredModelSpec};

/// Structured source data for the Begode Falcon registry entry.
pub const BEGODE_FALCON_MODEL_DEFINITION: RegisteredModelDefinition =
    RegisteredModelDefinition::new(
        &<BegodeFalconModel as RegisteredModelSpec>::REGISTRY_ENTRY,
        BEGODE_PARSER_KEY,
        BEGODE_FALCON_SESSION_KEY,
        crate::BEGODE_DATA_CHANNEL,
        super::begode_falcon_session,
    );

/// Source-backed initial registry entry for the Begode Falcon.
pub const BEGODE_FALCON_REGISTRY_ENTRY: ModelRegistryEntry =
    <BegodeFalconModel as RegisteredModelSpec>::REGISTRY_ENTRY;
