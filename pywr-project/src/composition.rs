use crate::error::{ComposeToSchemaError, PlaceholderKind, UnresolvedPlaceholder};
use crate::manifest::DefinitionOverrides;
use pywr_schema::meta::{ComponentMeta, ProvenanceSource};
use pywr_schema::model::{Metadata, MultiNetworkEntry, MultiNetworkTransfer, ScenarioDomain, TimeDomain};
use pywr_schema::template::{LookupError, TemplateUse};
use pywr_schema::{
    ModelSchema, MultiNetworkModelSchema, NetworkMergeOptions, NetworkSchema, NetworkSchemaRef, Reference, VisitPaths,
    VisitReferences,
};
use relative_path::{RelativePath, RelativePathBuf};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

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
fn load_includes(includes: &[ComposedNetworkPath]) -> Result<Vec<ComposedModelNetworkSchema>, ComposeToSchemaError> {
    includes
        .iter()
        .map(|network| {
            let network_schema = network.context.read_network(&network.path)?;
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
///
/// Relative paths are normalised, so that a path such as `upper/data/../../data/flows.csv`,
/// which a path variable gives in a sub-network's file, becomes `data/flows.csv`.
fn join_portable(dir: &Path, path: &Path) -> PathBuf {
    match (RelativePath::from_path(dir), RelativePath::from_path(path)) {
        (Ok(dir), Ok(path)) => PathBuf::from(dir.join(path).normalize().as_str()),
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
                let mut network = base_model.context.read_model(&base_model.path)?.network;
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
    let mut names: Vec<String> = Vec::new();
    network.visit_references(&mut |reference| {
        if let Reference::InterNetworkTransfer(name) = reference
            && !names.iter().any(|other| other == name)
        {
            names.push(name.to_string());
        }
    });
    names
}

/// Check that each transfer a network reads is declared as a transfer into it.
fn check_transfers_declared(networks: &[MultiNetworkEntry]) -> Result<(), ComposeToSchemaError> {
    for entry in networks {
        let NetworkSchemaRef::Inline(network) = &entry.network else {
            continue;
        };
        for transfer in transfers_read(network) {
            if !entry.transfers.iter().any(|declared| declared.name == transfer) {
                return Err(ComposeToSchemaError::TransferNotDeclared {
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
            let Some(source) = inline(&transfer.from_network) else {
                continue;
            };
            let mut missing = None;
            transfer.metric.visit_references(&mut |reference| {
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
                    transfer: transfer.name.clone(),
                    from_network: transfer.from_network.clone(),
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
    /// The variables of the file's project, which fill in the `${name}` templates in its string
    /// values. A template using a variable not given here is an error.
    pub variables: Arc<ProjectVariables>,
}

impl LoadContext {
    /// Read the network at `path`, filling in its templates and then rebasing its data paths.
    fn read_network(&self, path: &Path) -> Result<NetworkSchema, ComposeToSchemaError> {
        let mut network = NetworkSchema::from_path_with(path, |used| self.lookup(used))?;
        if let Some(data_dir) = &self.data_dir {
            rebase_data_paths(&mut network, data_dir);
        }
        Ok(network)
    }

    /// Read the model at `path`, filling in its templates and then rebasing its network's data
    /// paths.
    fn read_model(&self, path: &Path) -> Result<ModelSchema, ComposeToSchemaError> {
        let mut model = ModelSchema::from_path_with(path, |used| self.lookup(used))?;
        if let Some(data_dir) = &self.data_dir {
            rebase_data_paths(&mut model.network, data_dir);
        }
        Ok(model)
    }

    fn lookup(&self, used: &TemplateUse) -> Result<String, LookupError> {
        self.variables.lookup(used, self.data_dir.as_deref())
    }
}

/// The values of one project's variables in a composed definition.
#[derive(Clone, Debug, Default)]
pub struct ProjectVariables {
    /// The root project's canonical directory.
    root: PathBuf,
    /// Every variable the project declares, by name.
    values: BTreeMap<String, VariableState>,
}

/// The value a variable takes in a composed definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VariableValue {
    String(String),
    /// A canonical path.
    Path(PathBuf),
}

/// Whether a variable has a value in a composed definition.
#[derive(Clone, Debug)]
pub(crate) enum VariableState {
    Value(VariableValue),
    /// Nothing binds the variable and it has no default.
    Unbound,
    /// The variable's value could not be resolved, for the reason given. A template using it
    /// fails to load.
    Unavailable(String),
}

impl ProjectVariables {
    pub(crate) fn new(root: PathBuf, values: BTreeMap<String, VariableState>) -> Self {
        Self { root, values }
    }

    /// The value of the variable `name`, if it is declared and has one.
    pub fn value(&self, name: &str) -> Option<&VariableValue> {
        match self.values.get(name)? {
            VariableState::Value(value) => Some(value),
            VariableState::Unbound | VariableState::Unavailable(_) => None,
        }
    }

    /// The text to put in place of `used` in a file whose relative data paths are relative to
    /// `data_dir`, given as in [`LoadContext::data_dir`].
    ///
    /// A path value is given relative to `data_dir`, so that rebasing the file's data paths
    /// turns it back into the path relative to the root project's directory. That needs the
    /// template to start its string value.
    fn lookup(&self, used: &TemplateUse, data_dir: Option<&Path>) -> Result<String, LookupError> {
        match self.values.get(&used.name) {
            None => Err(LookupError::Undefined),
            Some(VariableState::Unbound) => Err(LookupError::Rejected(
                "nothing binds it and it has no default".to_string(),
            )),
            Some(VariableState::Unavailable(reason)) => Err(LookupError::Rejected(reason.clone())),
            Some(VariableState::Value(VariableValue::String(value))) => Ok(value.clone()),
            Some(VariableState::Value(VariableValue::Path(path))) => {
                if !used.at_start {
                    return Err(LookupError::Rejected(
                        "a path variable must start the string it is used in".to_string(),
                    ));
                }
                let base = match data_dir {
                    None => self.root.clone(),
                    Some(data_dir) if data_dir.is_absolute() => data_dir.to_path_buf(),
                    Some(data_dir) => normalize_lexically(&self.root.join(data_dir)),
                };
                Ok(portable_string(&crate::manifest::v1::relative_to(&base, path)))
            }
        }
    }
}

/// `path` with `.` and `..` components resolved without touching the filesystem.
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component),
        }
    }
    normalized
}

/// `path` as text, with `/` separators if it is relative, and `.` if it is empty.
fn portable_string(path: &Path) -> String {
    if path.as_os_str().is_empty() {
        return ".".to_string();
    }
    match RelativePath::from_path(path) {
        Ok(relative) => relative.as_str().to_string(),
        Err(_) => path.display().to_string(),
    }
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
    base_context: LoadContext,
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
        let base_schema = self.base_context.read_model(&self.base_model)?;

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
    base_context: LoadContext,
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
            base_context: LoadContext::default(),
            includes: Vec::new(),
            overrides: None,
        }
    }

    /// How the base model is adjusted as it is loaded. By default, it is loaded as it is, and a
    /// template in it is an error.
    pub fn base_context(&mut self, context: LoadContext) -> &mut Self {
        self.base_context = context;
        self
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
            base_context: self.base_context,
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
