use crate::error::ComposeToSchemaError;
use crate::manifest::DefinitionOverrides;
use pywr_schema::{ModelSchema, NetworkSchema};
use std::path::PathBuf;

/// A composed model that combines a base model with additional networks and metadata overrides.
pub struct ComposedModelSchemas {
    name: String,
    base_model: ModelSchema,
    includes: Vec<NetworkSchema>,
    overrides: Option<DefinitionOverrides>,
}

impl ComposedModelSchemas {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn base_model(&self) -> &ModelSchema {
        &self.base_model
    }

    pub fn includes(&self) -> &[NetworkSchema] {
        &self.includes
    }

    pub fn overrides(&self) -> Option<&DefinitionOverrides> {
        self.overrides.as_ref()
    }

    /// Compose the base model with the included networks and overrides, returning a new [`ModelSchema`].
    pub fn into_model_schema(self) -> Result<ModelSchema, ComposeToSchemaError> {
        let mut model_schema = self.base_model;

        for network in self.includes {
            model_schema.network.merge(network)?;
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

/// A composed model that combines a base model with additional networks and metadata overrides.
pub struct ComposedModel {
    name: String,
    base_model: PathBuf,
    includes: Vec<PathBuf>,
    overrides: Option<DefinitionOverrides>,
}

impl ComposedModel {
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Deserialize the composed model from the specified paths, returning a [`ComposedModelSchemas`] instance.
    pub fn load(&self) -> Result<ComposedModelSchemas, ComposeToSchemaError> {
        let base_schema = ModelSchema::from_path(&self.base_model)?;

        let includes: Vec<NetworkSchema> = self
            .includes
            .iter()
            .map(NetworkSchema::from_path)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ComposedModelSchemas {
            name: self.name.clone(),
            base_model: base_schema,
            includes,
            overrides: self.overrides.clone(),
        })
    }

    pub fn all_paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![self.base_model.clone()];
        paths.extend(self.includes.clone());
        paths
    }
}

pub struct ComposedModelBuilder {
    name: String,
    base_model: PathBuf,
    includes: Vec<PathBuf>,
    overrides: Option<DefinitionOverrides>,
}

impl ComposedModelBuilder {
    pub fn new(name: String, base_model: PathBuf) -> Self {
        Self {
            name,
            base_model,
            includes: Vec::new(),
            overrides: None,
        }
    }

    pub fn add_include(&mut self, include: PathBuf) -> &mut Self {
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
            includes: self.includes,
            overrides: self.overrides,
        }
    }
}
