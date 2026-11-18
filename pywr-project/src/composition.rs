use crate::error::ComposeToSchemaError;
use crate::manifest::DefinitionOverrides;
use pywr_schema::meta::ProvenanceSource;
use pywr_schema::{ModelSchema, NetworkMergeOptions, NetworkSchema, NetworkSchemaReadError};
use relative_path::RelativePathBuf;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug)]
pub struct PositionOffset {
    pub schematic: Option<(f32, f32)>,
    pub geographic: Option<(f32, f32)>,
}

pub struct ComposedModelNetworkSchema {
    network_schema: NetworkSchema,
    position_offset: Option<PositionOffset>,
    source: ProvenanceSource,
}

/// A composed model that combines a base model with additional networks and metadata overrides.
pub struct ComposedModelSchemas {
    name: String,
    base_model: ModelSchema,
    base_source: ProvenanceSource,
    includes: Vec<ComposedModelNetworkSchema>,
    overrides: Option<DefinitionOverrides>,
}

impl ComposedModelSchemas {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn base_model(&self) -> &ModelSchema {
        &self.base_model
    }

    pub fn includes(&self) -> &[ComposedModelNetworkSchema] {
        &self.includes
    }

    pub fn overrides(&self) -> Option<&DefinitionOverrides> {
        self.overrides.as_ref()
    }

    /// Compose the base model with the included networks and overrides, returning a new [`ModelSchema`].
    pub fn into_model_schema(self, options: &NetworkMergeOptions) -> Result<ModelSchema, ComposeToSchemaError> {
        let mut model_schema = self.base_model;
        model_schema.network.set_provenance(self.base_source);
        merge_includes(&mut model_schema.network, self.includes, options)?;

        if let Some(overrides) = self.overrides {
            if let Some(time) = overrides.time {
                model_schema.time = time;
            }
            if let Some(scenarios) = overrides.scenarios {
                model_schema.scenarios = Some(scenarios);
            }
        }

        model_schema.metadata.title = self.name;

        Ok(model_schema)
    }
}

/// Merge each included network into `network` in order, applying its position offset.
fn merge_includes(
    network: &mut NetworkSchema,
    includes: Vec<ComposedModelNetworkSchema>,
    options: &NetworkMergeOptions,
) -> Result<(), ComposeToSchemaError> {
    for include in includes {
        let mut included_schema = include.network_schema;
        included_schema.set_provenance(include.source);
        // Clone the options and apply any position offsets from the network
        let mut network_options = options.clone();
        if let Some(offset) = include.position_offset {
            if let Some(offset) = offset.schematic {
                if let Some(existing_offset) = &mut network_options.schematic_position_offset {
                    existing_offset.0 += offset.0;
                    existing_offset.1 += offset.1;
                } else {
                    network_options.schematic_position_offset = Some(offset);
                }
            }
            if let Some(offset) = offset.geographic {
                if let Some(existing_offset) = &mut network_options.geographic_position_offset {
                    existing_offset.0 += offset.0;
                    existing_offset.1 += offset.1;
                } else {
                    network_options.geographic_position_offset = Some(offset);
                }
            }
        }

        network.merge(included_schema, &network_options)?;
    }
    Ok(())
}

/// Read each included network from its path.
fn load_includes(includes: &[ComposedNetworkPath]) -> Result<Vec<ComposedModelNetworkSchema>, NetworkSchemaReadError> {
    includes
        .iter()
        .map(|network| {
            let network_schema = NetworkSchema::from_path(&network.path)?;
            Ok(ComposedModelNetworkSchema {
                network_schema,
                position_offset: network.position_offset,
                source: network.source.clone(),
            })
        })
        .collect()
}

/// A composed network that combines the networks selected by a network definition.
pub struct ComposedNetworkSchemas {
    name: String,
    includes: Vec<ComposedModelNetworkSchema>,
}

impl ComposedNetworkSchemas {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn includes(&self) -> &[ComposedModelNetworkSchema] {
        &self.includes
    }

    /// Merge the included networks in order, starting from an empty network.
    pub fn into_network_schema(self, options: &NetworkMergeOptions) -> Result<NetworkSchema, ComposeToSchemaError> {
        let mut network = NetworkSchema::default();
        merge_includes(&mut network, self.includes, options)?;
        Ok(network)
    }
}

/// A composed network that combines the networks selected by a network definition.
pub struct ComposedNetwork {
    name: String,
    includes: Vec<ComposedNetworkPath>,
}

impl ComposedNetwork {
    pub(crate) fn new(name: String, includes: Vec<ComposedNetworkPath>) -> Self {
        Self { name, includes }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Deserialize the included networks from their paths, returning a [`ComposedNetworkSchemas`] instance.
    pub fn load(&self) -> Result<ComposedNetworkSchemas, ComposeToSchemaError> {
        Ok(ComposedNetworkSchemas {
            name: self.name.clone(),
            includes: load_includes(&self.includes)?,
        })
    }

    pub fn all_paths(&self) -> Vec<PathBuf> {
        self.includes.iter().map(|n| n.path.clone()).collect()
    }
}

pub struct ComposedNetworkPath {
    pub path: PathBuf,
    pub position_offset: Option<PositionOffset>,
    pub source: ProvenanceSource,
}

/// A composed model that combines a base model with additional networks and metadata overrides.
pub struct ComposedModel {
    name: String,
    base_model: PathBuf,
    base_source: ProvenanceSource,
    includes: Vec<ComposedNetworkPath>,
    overrides: Option<DefinitionOverrides>,
}

impl ComposedModel {
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Deserialize the composed model from the specified paths, returning a [`ComposedModelSchemas`] instance.
    pub fn load(&self) -> Result<ComposedModelSchemas, ComposeToSchemaError> {
        let base_schema = ModelSchema::from_path(&self.base_model)?;

        let includes = load_includes(&self.includes)?;

        Ok(ComposedModelSchemas {
            name: self.name.clone(),
            base_model: base_schema,
            base_source: self.base_source.clone(),
            includes,
            overrides: self.overrides.clone(),
        })
    }

    pub fn all_paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![self.base_model.clone()];
        paths.extend(self.includes.iter().map(|n| n.path.clone()));
        paths
    }
}

pub struct ComposedModelBuilder {
    name: String,
    base_model: PathBuf,
    base_source: ProvenanceSource,
    includes: Vec<ComposedNetworkPath>,
    overrides: Option<DefinitionOverrides>,
}

impl ComposedModelBuilder {
    pub fn new(name: String, base_model: PathBuf, base_file: RelativePathBuf) -> Self {
        let base_source = ProvenanceSource {
            file: base_file,
            network_set: None,
            network: None,
        };
        Self {
            name,
            base_model,
            base_source,
            includes: Vec::new(),
            overrides: None,
        }
    }

    pub fn add_include(&mut self, include: ComposedNetworkPath) -> &mut Self {
        self.includes.push(include);
        self
    }

    pub fn overrides(&mut self, overrides: DefinitionOverrides) -> &mut Self {
        self.overrides = Some(overrides);
        self
    }

    pub fn build(self) -> ComposedModel {
        ComposedModel {
            name: self.name,
            base_model: self.base_model,
            base_source: self.base_source,
            includes: self.includes,
            overrides: self.overrides,
        }
    }
}
