use crate::error::ComposeToSchemaError;
use crate::manifest::DefinitionOverrides;
use pywr_schema::meta::ProvenanceSource;
use pywr_schema::{ModelSchema, NetworkMergeOptions, NetworkSchema, NetworkSchemaReadError};
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

        for network in self.includes {
            let mut included_schema = network.network_schema;
            included_schema.set_provenance(network.source);
            // Clone the options and apply any position offsets from the network
            let mut network_options = options.clone();
            if let Some(offset) = network.position_offset {
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

            model_schema.network.merge(included_schema, &network_options)?;
        }

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

        let includes: Vec<ComposedModelNetworkSchema> = self
            .includes
            .iter()
            .map(|network| {
                let network_schema = NetworkSchema::from_path(&network.path)?;
                Ok(ComposedModelNetworkSchema {
                    network_schema,
                    position_offset: network.position_offset,
                    source: network.source.clone(),
                })
            })
            .collect::<Result<Vec<_>, NetworkSchemaReadError>>()?;

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
    pub fn new(name: String, base_model: PathBuf, base_file: String) -> Self {
        let base_source = ProvenanceSource {
            file: base_file,
            network_set: None,
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
