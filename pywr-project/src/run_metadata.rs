use crate::manifest::v1::relative_to;
use crate::{Composed, Project};
use pywr_output_metadata::{ProjectRunMetadata, SourceFileMetadata};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io;
use std::path::Path;

impl Project {
    /// Describe the project definition `composed` was composed from, for a run's metadata.
    ///
    /// The files are the project's manifest followed by the files `composed` merges, in merge
    /// order, each with the SHA-256 hash of its current contents. A file used more than once,
    /// such as a multi-model's domain that is also one of its networks' base model, is listed
    /// where it is first used. Fails if a file cannot be read.
    pub fn run_metadata(&self, composed: &Composed) -> io::Result<ProjectRunMetadata> {
        let (name, paths) = match composed {
            Composed::Model(model) => (model.name(), model.all_paths()),
            Composed::MultiModel(model) => (model.name(), model.all_paths()),
        };
        let manifest = self.root().join(self.manifest_filename());
        let mut seen = HashSet::new();
        let files = std::iter::once(manifest)
            .chain(paths)
            .filter(|path| seen.insert(path.clone()))
            .map(|path| {
                Ok(SourceFileMetadata {
                    path: self.portable_path(&path),
                    sha256: sha256_hex(&std::fs::read(&path)?),
                })
            })
            .collect::<io::Result<_>>()?;
        Ok(ProjectRunMetadata {
            definition: name.to_string(),
            files,
        })
    }

    /// `path` relative to the project's root with `/` separators, through `..` for a file in a
    /// dependency outside the root. It is given whole if there is no relative path, as between
    /// drives.
    fn portable_path(&self, path: &Path) -> String {
        let relative = relative_to(self.root(), path);
        if relative.is_absolute() {
            return relative.to_string_lossy().into_owned();
        }
        relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
    }
}

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::sha256_hex;
    use crate::Project;
    use std::path::Path;
    use tempfile::tempdir;

    fn write(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn files(project: &Project, definition: &str) -> Vec<(String, String)> {
        let composed = project.compose(definition).unwrap();
        let metadata = project.run_metadata(&composed).unwrap();
        assert_eq!(metadata.definition, definition);
        metadata
            .files
            .into_iter()
            .map(|file| (file.path, file.sha256))
            .collect()
    }

    fn expected(files: &[(&str, &str)]) -> Vec<(String, String)> {
        files
            .iter()
            .map(|(path, contents)| (path.to_string(), sha256_hex(contents.as_bytes())))
            .collect()
    }

    const BASE: &str = "{\"base\": true}";

    #[test]
    fn run_metadata_lists_the_manifest_then_the_merged_files_with_their_hashes() {
        let root = tempdir().unwrap();
        let manifest = serde_json::json!({
            "format": "v1",
            "base_model": "base.json",
            "network_sets": [{"name": "nets"}],
            "models": [{"name": "with-nets", "include": [{"set": "nets", "include_all": true}]}]
        })
        .to_string();
        write(&root.path().join("pywr-project.json"), &manifest);
        write(&root.path().join("base.json"), BASE);
        write(&root.path().join("nets/b.json"), "{\"b\": true}");
        write(&root.path().join("nets/a.json"), "{\"a\": true}");

        let project = Project::open(root.path().join("pywr-project.json")).unwrap();
        assert_eq!(
            files(&project, "with-nets"),
            expected(&[
                ("pywr-project.json", &manifest),
                ("base.json", BASE),
                ("nets/a.json", "{\"a\": true}"),
                ("nets/b.json", "{\"b\": true}"),
            ])
        );
        // A known digest, so the hex encoding itself is checked.
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn files_from_a_dependency_outside_the_root_are_given_through_dot_dot() {
        let workspace = tempdir().unwrap();
        let manifest = serde_json::json!({
            "format": "v1",
            "base_model": "base.json",
            "dependencies": [{"name": "shared", "manifest": "../shared/pywr-project.json"}],
            "models": [{"name": "with-shared", "networks": ["shared::river"]}]
        })
        .to_string();
        write(&workspace.path().join("main/pywr-project.json"), &manifest);
        write(&workspace.path().join("main/base.json"), BASE);
        write(
            &workspace.path().join("shared/pywr-project.json"),
            &serde_json::json!({
                "format": "v1",
                "network_sets": [{"name": "nets"}],
                "networks": [{"name": "river", "include": [{"set": "nets", "include_all": true}]}]
            })
            .to_string(),
        );
        let river = "{\"nodes\": [], \"edges\": []}";
        write(&workspace.path().join("shared/nets/river.json"), river);

        let project = Project::open(workspace.path().join("main/pywr-project.json")).unwrap();
        assert_eq!(
            files(&project, "with-shared"),
            expected(&[
                ("pywr-project.json", &manifest),
                ("base.json", BASE),
                ("../shared/nets/river.json", river),
            ])
        );
    }
}
