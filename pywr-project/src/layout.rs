use crate::manifest::ProjectManifestValidationError;
use pywr_schema::meta::ProvenanceSource;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// A project as resolved against the filesystem: where its own and its dependencies' manifests,
/// base models, network sets and sub-networks are, together with any problems found.
///
/// A layout is produced even for a project with problems, and lists everything that resolved;
/// what could not be resolved is described in [`ProjectLayout::problems`], and where it was
/// expected in [`ProjectLayout::unresolved_paths`]. All paths are canonical, except the
/// unresolved paths, which are absolute.
#[derive(Debug)]
pub struct ProjectLayout {
    /// The root project followed by each project it depends on, directly or indirectly. A
    /// project that several others depend on is listed once.
    pub projects: Vec<ProjectLocation>,
    /// Network sets that resolved, in declaration order: each project's own sets, then its
    /// sub-networks' sets.
    pub network_sets: Vec<NetworkSetLayout>,
    /// Sub-networks that resolved, in declaration order.
    pub sub_networks: Vec<SubNetworkLayout>,
    /// Other files that definitions read: model definitions' own base models, multi-model base
    /// models and multi-model transfers files.
    pub definition_files: Vec<PathBuf>,
    /// The paths at which items were expected but not found, or found but of the wrong kind,
    /// such as a missing base model or network set directory. Each is listed once.
    pub unresolved_paths: Vec<PathBuf>,
    /// Every problem found, as validation reports them; `None` if the project is valid.
    pub problems: Option<ProjectManifestValidationError>,
}

/// Where a project in a [`ProjectLayout`] is.
#[derive(Clone, Debug)]
pub struct ProjectLocation {
    /// The dependency names leading to the project from the root, such as `org::shared`;
    /// `None` for the root project.
    pub alias: Option<String>,
    /// The project's directory.
    pub root: PathBuf,
    /// The project's manifest file, if known. It is unknown only for a root project resolved
    /// from a manifest that was not read from a file.
    pub manifest: Option<PathBuf>,
    /// The project-level base model, if one is given and it resolved.
    pub base_model: Option<PathBuf>,
}

/// A network set in a [`ProjectLayout`].
#[derive(Clone, Debug)]
pub struct NetworkSetLayout {
    /// The set's name as the root project refers to it, such as `catchment::inflows` or
    /// `neighbour::options`.
    pub name: String,
    /// The alias of the set's project, if it is a dependency.
    pub project: Option<String>,
    /// The sub-network the set belongs to, if any.
    pub sub_network: Option<String>,
    /// The set's name within its sub-network or project.
    pub set: String,
    /// The set's directory.
    pub dir: PathBuf,
    /// The directory that relative data paths in the set's files, such as a table's path, are
    /// relative to. For the root project's own sets this is the root project's directory, which
    /// is the default data path when the model is run.
    pub data_dir: PathBuf,
    pub min_files: Option<usize>,
    pub max_files: Option<usize>,
    /// The JSON files in the directory, sorted by name.
    pub files: Vec<NetworkFileLayout>,
}

/// A sub-network in a [`ProjectLayout`].
#[derive(Clone, Debug)]
pub struct SubNetworkLayout {
    /// The sub-network's name as the root project refers to it.
    pub name: String,
    /// The alias of the sub-network's project, if it is a dependency.
    pub project: Option<String>,
    /// The sub-network's directory.
    pub dir: PathBuf,
    /// The directory that relative data paths in the sub-network's files are relative to.
    pub data_dir: PathBuf,
    pub base_network: Option<NetworkFileLayout>,
}

/// A network file in a [`ProjectLayout`].
#[derive(Clone, Debug)]
pub struct NetworkFileLayout {
    pub path: PathBuf,
    /// The provenance that components from this file record, without a network definition,
    /// since a file may be included through several.
    pub source: ProvenanceSource,
}

impl ProjectLayout {
    /// Whether no problems were found.
    pub fn is_valid(&self) -> bool {
        self.problems.is_none()
    }

    /// The files and directories whose changes can change the project: every manifest, base
    /// model, network set and sub-network directory, network file and other definition file.
    ///
    /// Directories are included because adding or removing a file in a network set changes
    /// what the set contains. For each [unresolved path](Self::unresolved_paths), the path
    /// itself is included if it exists, and otherwise its nearest existing parent, so that
    /// creating the missing file or directory is noticed. Paths are listed once each, in a
    /// stable order.
    pub fn watch_paths(&self) -> Vec<PathBuf> {
        let projects = self
            .projects
            .iter()
            .flat_map(|project| project.manifest.iter().chain(&project.base_model));
        let sub_networks = self.sub_networks.iter().flat_map(|sub_network| {
            std::iter::once(&sub_network.dir).chain(sub_network.base_network.iter().map(|file| &file.path))
        });
        let sets = self
            .network_sets
            .iter()
            .flat_map(|set| std::iter::once(&set.dir).chain(set.files.iter().map(|file| &file.path)));
        let unresolved: Vec<_> = self
            .unresolved_paths
            .iter()
            .filter_map(|path| path.ancestors().find(|ancestor| ancestor.exists()))
            .map(|path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf()))
            .collect();

        unique_paths(
            projects
                .chain(sub_networks)
                .chain(sets)
                .chain(&self.definition_files)
                .chain(&unresolved)
                .map(PathBuf::as_path),
        )
    }
}

/// The given paths, each listed once, at its first position.
pub(crate) fn unique_paths<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|path| seen.insert(*path))
        .map(Path::to_path_buf)
        .collect()
}
