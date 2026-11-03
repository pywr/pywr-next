use pywr_project::{Project, ProjectManifest};
use std::path::Path;

/// The example project in `examples/my-model` must stay valid as the manifest format changes.
#[test]
fn example_project_validates_and_every_model_composes() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/my-model/pywr-project.json");
    let project = Project::open(&path).unwrap();
    let ProjectManifest::V1(manifest) = project.manifest();

    if let Err(error) = manifest.validate(project.root()) {
        panic!("{}", error.report());
    }
    assert!(!manifest.models.is_empty());
    for model in &manifest.models {
        project
            .compose_model(&model.name)
            .unwrap()
            .load()
            .unwrap()
            .into_model_schema(&Default::default())
            .unwrap();
    }
    for multi_model in &manifest.multi_models {
        project
            .compose_multi_model(&multi_model.name)
            .unwrap()
            .load()
            .unwrap()
            .into_multi_model_schema(&Default::default())
            .unwrap();
    }
}
