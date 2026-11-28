use crate::error::{ComposeToSchemaError, PlaceholderKind, UnresolvedPlaceholder};
use crate::manifest::DefinitionOverrides;
use pywr_schema::meta::{ComponentMeta, ProvenanceSource};
use pywr_schema::metric::MetricValueType;
use pywr_schema::model::{Metadata, MultiNetworkEntry, MultiNetworkTransfer, ScenarioDomain, TimeDomain};
use pywr_schema::{
    ModelSchema, MultiNetworkModelSchema, NetworkMergeOptions, NetworkSchema, NetworkSchemaReadError, NetworkSchemaRef,
    Reference, VisitPaths, VisitReferences,
};
use relative_path::{RelativePath, RelativePathBuf};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq)]
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

        let placeholders = unresolved_placeholders(&model_schema.network);
        if !placeholders.is_empty() {
            return Err(ComposeToSchemaError::UnresolvedPlaceholders { placeholders });
        }
        // A plain model has no other networks to receive transfers from.
        if let Some(transfer) = transfers_read(&model_schema.network).into_iter().next() {
            return Err(ComposeToSchemaError::TransferInModel { transfer });
        }

        Ok(model_schema)
    }
}

/// List each placeholder left in `network`, with where it came from if known.
fn unresolved_placeholders(network: &NetworkSchema) -> Vec<UnresolvedPlaceholder> {
    fn describe(kind: PlaceholderKind, name: &str, meta: &dyn ComponentMeta) -> UnresolvedPlaceholder {
        UnresolvedPlaceholder {
            kind,
            name: name.to_string(),
            origin: meta.provenance().map(|provenance| provenance.origin.clone()),
            network: None,
        }
    }

    let mut placeholders = Vec::new();
    for node in network.nodes.iter().filter(|node| node.is_placeholder()) {
        placeholders.push(describe(PlaceholderKind::Node, node.name(), node.meta()));
    }
    for node in network
        .virtual_nodes
        .iter()
        .flatten()
        .filter(|node| node.is_placeholder())
    {
        placeholders.push(describe(PlaceholderKind::VirtualNode, node.name(), node.meta()));
    }
    for parameter in network.parameters.iter().flatten().filter(|p| p.is_placeholder()) {
        placeholders.push(describe(PlaceholderKind::Parameter, parameter.name(), parameter.meta()));
    }
    for table in network.tables.iter().flatten().filter(|table| table.is_placeholder()) {
        placeholders.push(describe(PlaceholderKind::Table, table.name(), table.meta()));
    }
    for time_series in network.time_series.iter().flatten().filter(|ts| ts.is_placeholder()) {
        placeholders.push(describe(
            PlaceholderKind::TimeSeries,
            time_series.name(),
            time_series.meta(),
        ));
    }
    for output in network
        .outputs
        .iter()
        .flatten()
        .filter(|output| output.is_placeholder())
    {
        placeholders.push(describe(PlaceholderKind::Output, output.name(), output.meta()));
    }
    placeholders
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

/// Read each included network from its path, applying its load context.
fn load_includes(includes: &[ComposedNetworkPath]) -> Result<Vec<ComposedModelNetworkSchema>, NetworkSchemaReadError> {
    includes
        .iter()
        .map(|network| {
            let mut network_schema = NetworkSchema::from_path(&network.path)?;
            if let Some(data_dir) = &network.context.data_dir {
                rebase_data_paths(&mut network_schema, data_dir);
            }
            Ok(ComposedModelNetworkSchema {
                network_schema,
                position_offset: network.position_offset,
                source: network.source.clone(),
            })
        })
        .collect()
}

/// Prefix the relative data paths in `network` with `data_dir`.
///
/// Output paths are left unchanged, because they are relative to the output directory rather
/// than the data directory.
fn rebase_data_paths(network: &mut NetworkSchema, data_dir: &Path) {
    let mut rebase = |path: &mut PathBuf| {
        if path.is_relative() {
            *path = join_portable(data_dir, path);
        }
    };
    network.nodes.visit_paths_mut(&mut rebase);
    network.virtual_nodes.visit_paths_mut(&mut rebase);
    network.parameters.visit_paths_mut(&mut rebase);
    network.tables.visit_paths_mut(&mut rebase);
    network.time_series.visit_paths_mut(&mut rebase);
    network.metric_sets.visit_paths_mut(&mut rebase);
}

/// Join `path` to `dir` with `/` separators when both are relative, so a composed model saved on
/// Windows loads elsewhere. An absolute `dir` is joined with the platform's separator.
fn join_portable(dir: &Path, path: &Path) -> PathBuf {
    match (RelativePath::from_path(dir), RelativePath::from_path(path)) {
        (Ok(dir), Ok(path)) => PathBuf::from(dir.join(path).as_str()),
        _ => dir.join(path),
    }
}

/// A composed network that combines the networks selected by a network definition.
pub struct ComposedNetworkSchemas {
    name: String,
    /// The network of a model definition's base model, which the includes are merged into.
    base_network: Option<NetworkSchema>,
    includes: Vec<ComposedModelNetworkSchema>,
}

impl ComposedNetworkSchemas {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn includes(&self) -> &[ComposedModelNetworkSchema] {
        &self.includes
    }

    /// Merge the included networks in order, starting from the base network if there is one and
    /// an empty network otherwise.
    pub fn into_network_schema(self, options: &NetworkMergeOptions) -> Result<NetworkSchema, ComposeToSchemaError> {
        let mut network = self.base_network.unwrap_or_default();
        merge_includes(&mut network, self.includes, options)?;
        Ok(network)
    }
}

/// A composed network that combines the networks selected by a network definition, or the
/// network part of a model definition.
pub struct ComposedNetwork {
    name: String,
    /// A model definition's base model, whose network is the starting point.
    base_model: Option<ComposedNetworkPath>,
    includes: Vec<ComposedNetworkPath>,
}

impl ComposedNetwork {
    pub(crate) fn new(
        name: String,
        base_model: Option<ComposedNetworkPath>,
        includes: Vec<ComposedNetworkPath>,
    ) -> Self {
        Self {
            name,
            base_model,
            includes,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The model definition's base model, whose network is the starting point, if any.
    pub fn base_model(&self) -> Option<&ComposedNetworkPath> {
        self.base_model.as_ref()
    }

    /// The network files to merge, in merge order, each with its provenance.
    pub fn includes(&self) -> &[ComposedNetworkPath] {
        &self.includes
    }

    /// Deserialize the included networks from their paths, returning a [`ComposedNetworkSchemas`] instance.
    pub fn load(&self) -> Result<ComposedNetworkSchemas, ComposeToSchemaError> {
        let base_network = match &self.base_model {
            Some(base_model) => {
                let mut network = ModelSchema::from_path(&base_model.path)?.network;
                if let Some(data_dir) = &base_model.context.data_dir {
                    rebase_data_paths(&mut network, data_dir);
                }
                network.set_provenance(base_model.source.clone());
                Some(network)
            }
            None => None,
        };
        Ok(ComposedNetworkSchemas {
            name: self.name.clone(),
            base_network,
            includes: load_includes(&self.includes)?,
        })
    }

    pub fn all_paths(&self) -> Vec<PathBuf> {
        self.base_model
            .iter()
            .map(|base_model| base_model.path.clone())
            .chain(self.includes.iter().map(|n| n.path.clone()))
            .collect()
    }
}

/// Where a composed multi-model takes its time domain, scenarios and metadata from.
pub enum ComposedDomain {
    /// A base model file, whose network is ignored.
    BaseModel(PathBuf),
    Inline {
        time: TimeDomain,
        scenarios: Option<ScenarioDomain>,
    },
}

/// One network of a composed multi-model, with the transfers into it.
pub struct ComposedMultiModelNetwork {
    pub network: ComposedNetwork,
    pub transfers: Vec<MultiNetworkTransfer>,
    /// The canonical path of the file some of `transfers` were read from, if any.
    pub transfers_file: Option<PathBuf>,
}

/// A composed multi-network model: several composed networks, linked by transfers, that share a
/// time domain and scenarios.
pub struct ComposedMultiModel {
    name: String,
    domain: ComposedDomain,
    /// Overrides applied to the domain, in order.
    overrides: Vec<DefinitionOverrides>,
    networks: Vec<ComposedMultiModelNetwork>,
}

impl ComposedMultiModel {
    pub(crate) fn new(
        name: String,
        domain: ComposedDomain,
        overrides: Vec<DefinitionOverrides>,
        networks: Vec<ComposedMultiModelNetwork>,
    ) -> Self {
        Self {
            name,
            domain,
            overrides,
            networks,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn networks(&self) -> &[ComposedMultiModelNetwork] {
        &self.networks
    }

    /// Deserialize the domain and networks from their paths, returning a [`ComposedMultiModelSchemas`] instance.
    pub fn load(&self) -> Result<ComposedMultiModelSchemas, ComposeToSchemaError> {
        let (metadata, mut time, mut scenarios) = match &self.domain {
            ComposedDomain::BaseModel(path) => {
                let model = ModelSchema::from_path(path)?;
                (model.metadata, model.time, model.scenarios)
            }
            ComposedDomain::Inline { time, scenarios } => (Metadata::default(), time.clone(), scenarios.clone()),
        };
        for overrides in &self.overrides {
            if let Some(overridden) = &overrides.time {
                time = overridden.clone();
            }
            if let Some(overridden) = &overrides.scenarios {
                scenarios = Some(overridden.clone());
            }
        }
        let networks = self
            .networks
            .iter()
            .map(|entry| Ok((entry.network.load()?, entry.transfers.clone())))
            .collect::<Result<_, ComposeToSchemaError>>()?;
        Ok(ComposedMultiModelSchemas {
            metadata: Metadata {
                title: self.name.clone(),
                ..metadata
            },
            time,
            scenarios,
            networks,
        })
    }

    pub fn all_paths(&self) -> Vec<PathBuf> {
        let domain = match &self.domain {
            ComposedDomain::BaseModel(path) => Some(path.clone()),
            ComposedDomain::Inline { .. } => None,
        };
        domain
            .into_iter()
            .chain(self.networks.iter().flat_map(|entry| {
                entry
                    .network
                    .all_paths()
                    .into_iter()
                    .chain(entry.transfers_file.clone())
            }))
            .collect()
    }
}

/// A composed multi-network model with its networks loaded but not yet merged.
pub struct ComposedMultiModelSchemas {
    metadata: Metadata,
    time: TimeDomain,
    scenarios: Option<ScenarioDomain>,
    networks: Vec<(ComposedNetworkSchemas, Vec<MultiNetworkTransfer>)>,
}

impl ComposedMultiModelSchemas {
    /// Merge each network and return a [`MultiNetworkModelSchema`] with every network inline.
    ///
    /// Fails if a network has unresolved placeholders, reads a transfer that is not declared for
    /// it, or writes an output file that another network also writes, or if a transfer reads a
    /// node, virtual node or parameter that its source network does not have.
    pub fn into_multi_model_schema(
        self,
        options: &NetworkMergeOptions,
    ) -> Result<MultiNetworkModelSchema, ComposeToSchemaError> {
        let mut networks = Vec::with_capacity(self.networks.len());
        let mut placeholders = Vec::new();
        for (schemas, transfers) in self.networks {
            let name = schemas.name().to_string();
            let network = schemas.into_network_schema(options)?;
            placeholders.extend(unresolved_placeholders(&network).into_iter().map(|placeholder| {
                UnresolvedPlaceholder {
                    network: Some(name.clone()),
                    ..placeholder
                }
            }));
            networks.push(MultiNetworkEntry {
                name,
                network: NetworkSchemaRef::Inline(network),
                transfers,
            });
        }
        if !placeholders.is_empty() {
            return Err(ComposeToSchemaError::UnresolvedPlaceholders { placeholders });
        }
        check_transfers_declared(&networks)?;
        check_transfer_components(&networks)?;
        check_output_files(&networks)?;

        Ok(MultiNetworkModelSchema {
            metadata: self.metadata,
            time: self.time,
            scenarios: self.scenarios,
            networks,
        })
    }
}

/// The inter-network transfers that `network` reads, each once, in the order first read.
fn transfers_read(network: &NetworkSchema) -> Vec<String> {
    transfer_reads(network).into_iter().map(|(name, _)| name).collect()
}

/// The inter-network transfers that `network` reads, each once per kind of value read, in the
/// order first read.
fn transfer_reads(network: &NetworkSchema) -> Vec<(String, MetricValueType)> {
    let mut reads: Vec<(String, MetricValueType)> = Vec::new();
    network.visit_references(&mut |reference| {
        if let Reference::InterNetworkTransfer { name, metric } = reference
            && !reads.iter().any(|(other, m)| other == name && *m == metric)
        {
            reads.push((name.to_string(), metric));
        }
    });
    reads
}

/// Check that each transfer a network reads is declared as a transfer into it, and that a
/// transfer read as an index is an index transfer.
fn check_transfers_declared(networks: &[MultiNetworkEntry]) -> Result<(), ComposeToSchemaError> {
    for entry in networks {
        let NetworkSchemaRef::Inline(network) = &entry.network else {
            continue;
        };
        for (transfer, metric) in transfer_reads(network) {
            let Some(declared) = entry.transfers.iter().find(|declared| declared.name() == transfer) else {
                return Err(ComposeToSchemaError::TransferNotDeclared {
                    network: entry.name.clone(),
                    transfer,
                });
            };
            // An index transfer can be read as a float, but a float transfer not as an index.
            if metric == MetricValueType::Index && declared.value_type() == MetricValueType::Float {
                return Err(ComposeToSchemaError::TransferNotAnIndex {
                    network: entry.name.clone(),
                    transfer,
                });
            }
        }
    }
    Ok(())
}

/// Check that no two networks write the same output file, which would fail or overwrite each
/// other's output when the model runs.
fn check_output_files(networks: &[MultiNetworkEntry]) -> Result<(), ComposeToSchemaError> {
    let mut written: Vec<(&Path, &str)> = Vec::new();
    for entry in networks {
        let NetworkSchemaRef::Inline(network) = &entry.network else {
            continue;
        };
        for filename in network.outputs.iter().flatten().filter_map(|output| output.filename()) {
            if let Some((_, first)) = written
                .iter()
                .find(|(other, name)| *other == filename && *name != entry.name)
            {
                return Err(ComposeToSchemaError::SharedOutputFile {
                    file: filename.to_path_buf(),
                    first: first.to_string(),
                    second: entry.name.clone(),
                });
            }
            written.push((filename, &entry.name));
        }
    }
    Ok(())
}

/// Check that each transfer reads a node, virtual node or parameter that its source network has.
fn check_transfer_components(networks: &[MultiNetworkEntry]) -> Result<(), ComposeToSchemaError> {
    let inline = |name: &str| {
        networks
            .iter()
            .find(|entry| entry.name == name)
            .and_then(|entry| match &entry.network {
                NetworkSchemaRef::Inline(network) => Some(network),
                NetworkSchemaRef::Path(_) => None,
            })
    };
    for entry in networks {
        for transfer in &entry.transfers {
            let Some(source) = inline(transfer.from_network()) else {
                continue;
            };
            let mut missing = None;
            transfer.visit_references(&mut |reference| {
                if missing.is_some() {
                    return;
                }
                missing = match reference {
                    Reference::Node { name, .. } if source.get_node_by_name(name).is_none() => {
                        Some(format!("the node `{name}`"))
                    }
                    Reference::VirtualNode { name, .. } if source.get_virtual_node_by_name(name).is_none() => {
                        Some(format!("the virtual node `{name}`"))
                    }
                    Reference::Parameter { name, .. } if source.get_parameter_by_name(name).is_none() => {
                        Some(format!("the parameter `{name}`"))
                    }
                    _ => None,
                };
            });
            if let Some(component) = missing {
                return Err(ComposeToSchemaError::TransferComponentNotFound {
                    network: entry.name.clone(),
                    transfer: transfer.name().to_string(),
                    from_network: transfer.from_network().to_string(),
                    component,
                });
            }
        }
    }
    Ok(())
}

/// A composed model or multi-model, as named by a definition.
pub enum Composed {
    Model(ComposedModel),
    MultiModel(ComposedMultiModel),
}

/// How an included network is adjusted as it is loaded.
#[derive(Clone, Debug, Default)]
pub struct LoadContext {
    /// Directory that the network's relative data paths are relative to. They are rewritten to
    /// be relative to the root project's directory instead. The directory is itself relative to
    /// the root project's directory, possibly through `..`, or absolute when no relative path
    /// exists. `None` leaves the paths as they are.
    pub data_dir: Option<PathBuf>,
}

/// A network file to merge into a composed model or network, with where it came from.
#[derive(Clone, Debug)]
pub struct ComposedNetworkPath {
    pub path: PathBuf,
    pub position_offset: Option<PositionOffset>,
    pub source: ProvenanceSource,
    pub context: LoadContext,
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

    /// The path of the base model file.
    pub fn base_model_path(&self) -> &Path {
        &self.base_model
    }

    /// The provenance recorded for components of the base model.
    pub fn base_source(&self) -> &ProvenanceSource {
        &self.base_source
    }

    /// The network files to merge into the base model, in merge order, each with its provenance.
    pub fn includes(&self) -> &[ComposedNetworkPath] {
        &self.includes
    }

    /// The overrides applied to the base model's time and scenario domains, if any.
    pub fn overrides(&self) -> Option<&DefinitionOverrides> {
        self.overrides.as_ref()
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
            sub_network: None,
            project: None,
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

#[cfg(test)]
mod tests {
    use super::join_portable;
    use std::path::{Path, PathBuf};

    #[test]
    fn relative_data_paths_are_joined_with_forward_slashes() {
        let joined = join_portable(Path::new("../org/data"), Path::new("flows/inflow.csv"));
        assert_eq!(joined.to_str(), Some("../org/data/flows/inflow.csv"));
        assert_eq!(
            join_portable(Path::new("/abs/data"), Path::new("flows.csv")),
            PathBuf::from("/abs/data").join("flows.csv")
        );
    }
}
