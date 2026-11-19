use crate::{ComposedModel, Project};
use pywr_output_metadata::{ProjectRunMetadata, SourceFileMetadata};
use sha2::{Digest, Sha256};
use std::io;
use std::path::{Component, Path};

impl Project {
    /// Describe the project definition `composed` was composed from, for a run's metadata.
    ///
    /// The files are the project's manifest followed by the files `composed` merges, in merge
    /// order, each with the SHA-256 hash of its current contents. Fails if a file cannot be read.
    pub fn run_metadata(&self, composed: &ComposedModel) -> io::Result<ProjectRunMetadata> {
        let manifest = self.root().join(self.manifest_filename());
        let files = std::iter::once(manifest)
            .chain(composed.all_paths())
            .map(|path| {
                Ok(SourceFileMetadata {
                    path: portable_relative_path(self.root(), &path),
                    sha256: sha256_hex(&std::fs::read(&path)?),
                })
            })
            .collect::<io::Result<_>>()?;
        Ok(ProjectRunMetadata {
            definition: composed.name().to_string(),
            files,
        })
    }
}

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|byte| format!("{byte:02x}")).collect()
}

/// `target` relative to `base`, through `..` if needed, with `/` separators. A path that shares
/// no prefix with `base`, such as one on another drive, is given whole.
fn portable_relative_path(base: &Path, target: &Path) -> String {
    let base: Vec<_> = base.components().collect();
    let target: Vec<_> = target.components().collect();
    let common = base
        .iter()
        .zip(&target)
        .take_while(|(left, right)| left == right)
        .count();
    if common == 0 {
        return target_string(&target);
    }
    let parents = std::iter::repeat_n("..".to_string(), base.len() - common);
    let rest = target[common..]
        .iter()
        .map(|component| component.as_os_str().to_string_lossy().into_owned());
    parents.chain(rest).collect::<Vec<_>>().join("/")
}

fn target_string(components: &[Component<'_>]) -> String {
    components
        .iter()
        .collect::<std::path::PathBuf>()
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::portable_relative_path;
    use std::path::Path;

    #[test]
    fn paths_are_relative_to_the_project_with_forward_slashes() {
        let root = Path::new("/work/projects/main");
        assert_eq!(
            portable_relative_path(root, Path::new("/work/projects/main/options/a.json")),
            "options/a.json"
        );
        assert_eq!(
            portable_relative_path(root, Path::new("/work/projects/shared/base.json")),
            "../shared/base.json"
        );
    }
}
