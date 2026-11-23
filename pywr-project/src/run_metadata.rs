use crate::manifest::v1::relative_to;
use crate::{BindingSource, Composed, DefinitionKind, LoadContext, Project, VariableValue};
use pywr_output_metadata::{ProjectRunMetadata, SourceFileMetadata, VariableMetadata, VariableSource};
use pywr_schema::template;
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
    /// where it is first used.
    ///
    /// The variables are those whose values fill in the templates of the files, once for a
    /// model and once in each network of a multi-model. Fails if a file cannot be read.
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
        let mut variables = Vec::new();
        match composed {
            Composed::Model(model) => {
                let base = std::iter::once((model.base_model_path(), model.base_context(), None));
                let includes = model
                    .includes()
                    .iter()
                    .map(|file| (file.path.as_path(), &file.context, file.source.project.as_deref()));
                self.add_variables(None, base.chain(includes), &mut variables)?;
            }
            Composed::MultiModel(model) => {
                for entry in model.networks() {
                    let network = &entry.network;
                    let files = network
                        .base_model()
                        .into_iter()
                        .chain(network.includes())
                        .map(|file| (file.path.as_path(), &file.context, file.source.project.as_deref()));
                    self.add_variables(Some(network.name()), files, &mut variables)?;
                }
            }
        }
        Ok(ProjectRunMetadata {
            definition: name.to_string(),
            files,
            variables,
        })
    }

    /// Add to `variables` each variable that the templates in `files` use and that has a value,
    /// once for `network`. Each file comes with how it is loaded and the alias of its project.
    fn add_variables<'f>(
        &self,
        network: Option<&str>,
        files: impl Iterator<Item = (&'f Path, &'f LoadContext, Option<&'f str>)>,
        variables: &mut Vec<VariableMetadata>,
    ) -> io::Result<()> {
        for (path, context, alias) in files {
            let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?).map_err(io::Error::other)?;
            for used in template::scan(&value).uses {
                let name = match alias {
                    Some(alias) => format!("{alias}::{}", used.name),
                    None => used.name.clone(),
                };
                let recorded = variables
                    .iter()
                    .any(|variable| variable.network.as_deref() == network && variable.name == name);
                // A variable without a value fails to load, so it is never run.
                let Some(bound) = context.variables.get(&used.name) else {
                    continue;
                };
                if recorded {
                    continue;
                }
                variables.push(VariableMetadata {
                    network: network.map(str::to_string),
                    name,
                    value: bound.written.clone(),
                    path: match &bound.value {
                        VariableValue::Path(path) => Some(self.portable_path(path)),
                        VariableValue::String(_) => None,
                    },
                    source: match &bound.source {
                        BindingSource::Default => VariableSource::Default,
                        BindingSource::CommandLine => VariableSource::CommandLine,
                        BindingSource::Definition(definition) => VariableSource::Definition {
                            kind: match definition.kind {
                                DefinitionKind::Model => "model",
                                DefinitionKind::MultiModel => "multi_model",
                                DefinitionKind::Network => "network",
                                DefinitionKind::MultiModelNetwork => "multi_model_network",
                                DefinitionKind::CommandLine => "command_line",
                            }
                            .to_string(),
                            name: definition.name.clone(),
                        },
                    },
                });
            }
        }
        Ok(())
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

    #[test]
    fn run_metadata_records_the_variables_used_and_where_their_values_came_from() {
        use crate::{ComposeOptions, VariableMetadata, VariableSource};

        let root = tempdir().unwrap();
        let manifest = serde_json::json!({
            "format": "v1",
            "base_model": "base.json",
            "variables": [
                {"name": "dataset", "type": "path", "default": "a", "choices": {"a": "data/a", "b": "data/b"}},
                {"name": "column", "type": "string", "default": "baseline"},
                {"name": "unused", "type": "string", "default": "x"}
            ],
            "network_sets": [{"name": "nets"}],
            "networks": [{"name": "river", "include": [{"set": "nets", "include_all": true}]}],
            "models": [{"name": "m", "networks": ["river"], "variables": {"dataset": "b"}}],
            "multi_models": [{
                "name": "joint",
                "domain": {"type": "Model", "name": "m"},
                "networks": [
                    {"name": "first", "network": {"type": "Network", "name": "river"}},
                    {"name": "second", "network": {"type": "Model", "name": "m"}}
                ]
            }]
        });
        write(&root.path().join("pywr-project.json"), &manifest.to_string());
        write(&root.path().join("base.json"), BASE);
        write(&root.path().join("data/a/.keep"), "");
        write(&root.path().join("data/b/.keep"), "");
        write(
            &root.path().join("nets/river.json"),
            &serde_json::json!({"nodes": [], "edges": [], "comment": "${column}", "path": "${dataset}/x.csv"})
                .to_string(),
        );

        let project = Project::open(root.path().join("pywr-project.json")).unwrap();
        let options = ComposeOptions {
            variables: [("column".to_string(), "cli".to_string())].into(),
        };
        let composed = project.compose_with("m", &options).unwrap();
        let variable = |network: Option<&str>, name: &str, value: &str, path: Option<&str>, source| VariableMetadata {
            network: network.map(str::to_string),
            name: name.to_string(),
            value: value.to_string(),
            path: path.map(str::to_string),
            source,
        };
        let model_source = VariableSource::Definition {
            kind: "model".to_string(),
            name: "m".to_string(),
        };
        // Variables are listed in the order the merged files use them, and `unused` is not.
        assert_eq!(
            project.run_metadata(&composed).unwrap().variables,
            [
                variable(None, "column", "cli", None, VariableSource::CommandLine),
                variable(None, "dataset", "b", Some("data/b"), model_source.clone()),
            ]
        );

        let composed = project.compose("joint").unwrap();
        assert_eq!(
            project.run_metadata(&composed).unwrap().variables,
            [
                variable(Some("first"), "column", "baseline", None, VariableSource::Default),
                variable(Some("first"), "dataset", "a", Some("data/a"), VariableSource::Default),
                variable(Some("second"), "column", "baseline", None, VariableSource::Default),
                variable(Some("second"), "dataset", "b", Some("data/b"), model_source),
            ]
        );
    }
}
