use pywr_project::{Project, ProjectManifest};
use std::path::Path;

/// The example projects in `examples/` must stay valid as the manifest format changes.
#[test]
fn example_project_validates_and_every_model_composes() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/my-model/pywr-project.json");
    let project = Project::open(&path).unwrap();
    let ProjectManifest::V1(manifest) = project.manifest();

    if let Err(error) = project.validate() {
        panic!("{}", error.report());
    }
    // The project depends on the `neighbour` project, which must be valid on its own too.
    let neighbour = Project::open(path.parent().unwrap().join("../neighbour/pywr-project.json")).unwrap();
    if let Err(error) = neighbour.validate() {
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

/// A composition lists the files it will merge, with their provenance, before any are read.
#[test]
fn composed_model_lists_its_includes_with_provenance() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/my-model/pywr-project.json");
    let project = Project::open(&path).unwrap();
    let composed = project.compose_model("baseline-option1").unwrap();

    assert_eq!(composed.base_source().file.as_str(), "models/base.json");
    let sources: Vec<_> = composed
        .includes()
        .iter()
        .map(|include| {
            (
                include.source.file.as_str(),
                include.source.network.as_deref(),
                include.source.sub_network.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        sources,
        [
            (
                "catchment/inflows/historical.json",
                Some("historical"),
                Some("catchment")
            ),
            ("options/option1.json", Some("option1"), None),
        ]
    );
}
