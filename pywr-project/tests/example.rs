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

/// A saved manifest leaves out empty lists and unset options, as a hand-written one would.
#[test]
fn saved_manifests_omit_empty_and_unset_fields() {
    let minimal = serde_json::json!({
        "format": "v1",
        "base_model": "base.json",
        "network_sets": [{ "name": "options" }],
        "networks": [{ "name": "option1", "include": [{ "set": "options", "files": ["option1.json"] }] }],
        "models": [{ "name": "baseline" }]
    });
    let manifest: ProjectManifest = serde_json::from_value(minimal.clone()).unwrap();
    assert_eq!(serde_json::to_value(&manifest).unwrap(), minimal);

    for example in ["my-model", "neighbour"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../examples/{example}/pywr-project.json"));
        let project = Project::open(&path).unwrap();
        let saved = serde_json::to_string(project.manifest()).unwrap();
        assert!(!saved.contains("null"), "{example}: {saved}");
        assert!(!saved.contains("[]"), "{example}: {saved}");
    }
}

/// A layout lists the projects, sets and sub-networks, and the paths to watch for changes.
#[test]
fn example_project_layout_lists_projects_sets_and_watch_paths() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/my-model/pywr-project.json");
    let project = Project::open(&path).unwrap();
    let layout = project.layout();
    assert!(layout.is_valid(), "{:?}", layout.problems);

    let examples = path.parent().unwrap().parent().unwrap().canonicalize().unwrap();
    let my_model = examples.join("my-model");
    let neighbour = examples.join("neighbour");

    let projects: Vec<_> = layout
        .projects
        .iter()
        .map(|project| {
            (
                project.alias.as_deref(),
                project.manifest.clone(),
                project.base_model.clone(),
            )
        })
        .collect();
    assert_eq!(
        projects,
        [
            (
                None,
                Some(my_model.join("pywr-project.json")),
                Some(my_model.join("models/base.json"))
            ),
            (Some("neighbour"), Some(neighbour.join("pywr-project.json")), None),
        ]
    );

    let sets: Vec<_> = layout
        .network_sets
        .iter()
        .map(|set| (set.name.as_str(), set.dir.clone(), set.files.len()))
        .collect();
    assert_eq!(
        sets,
        [
            ("options", my_model.join("options"), 2),
            ("catchment::inflows", my_model.join("catchment/inflows"), 2),
            ("neighbour::supply", neighbour.join("supply"), 1),
        ]
    );
    let file = &layout.network_sets[1].files[0];
    assert_eq!(file.source.file.as_str(), "catchment/inflows/historical.json");
    assert_eq!(file.source.network_set.as_deref(), Some("inflows"));
    assert_eq!(file.source.sub_network.as_deref(), Some("catchment"));

    let sub_networks: Vec<_> = layout
        .sub_networks
        .iter()
        .map(|sub_network| (sub_network.name.as_str(), sub_network.dir.clone()))
        .collect();
    assert_eq!(sub_networks, [("catchment", my_model.join("catchment"))]);

    let watch = layout.watch_paths();
    for expected in [
        my_model.join("pywr-project.json"),
        neighbour.join("pywr-project.json"),
        my_model.join("models/base.json"),
        my_model.join("catchment"),
        my_model.join("options"),
        my_model.join("options/option1.json"),
        neighbour.join("supply/base.json"),
    ] {
        assert!(watch.contains(&expected), "{expected:?} not in {watch:?}");
    }
    assert_eq!(watch.len(), 12, "{watch:#?}");
}
