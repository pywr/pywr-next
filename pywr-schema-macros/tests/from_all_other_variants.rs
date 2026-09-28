use pywr_schema_macros::{PywrFromAllOtherVariants, PywrIntoType};

#[derive(Clone, Default)]
struct Input {
    name: String,
    count: u32,
    input_only: bool,
}

#[derive(Clone)]
struct Output {
    name: String,
    count: u32,
    output_only: u32,
}

impl Default for Output {
    fn default() -> Self {
        Self {
            name: String::new(),
            count: 0,
            output_only: 42,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ComponentType {
    Input,
    Output,
}

#[derive(Clone, PywrFromAllOtherVariants, PywrIntoType)]
#[pywr_from_all_other_variants(files("tests/from_all_other_variants.rs"))]
enum Component {
    Input(Input),
    Output(Box<Output>),
}

#[test]
fn converts_between_variant_payloads() {
    let output: Output = Input {
        name: "source".into(),
        count: 3,
        input_only: true,
    }
    .into();
    assert_eq!(output.name, "source");
    assert_eq!(output.count, 3);
    assert_eq!(output.output_only, 42);

    let input: Input = output.into();
    assert_eq!(input.name, "source");
    assert_eq!(input.count, 3);
    assert!(!input.input_only);

    let variants = [Component::Input(input), Component::Output(Box::default())];
    assert!(matches!(&variants[0], Component::Input(value) if value.count == 3));
    assert!(matches!(&variants[1], Component::Output(value) if value.output_only == 42));
}

#[test]
fn converts_every_variant_pair() {
    for source in [
        Component::Input(Input {
            name: "input".into(),
            count: 3,
            input_only: true,
        }),
        Component::Output(Box::new(Output {
            name: "output".into(),
            count: 4,
            output_only: 42,
        })),
    ] {
        for target in [ComponentType::Input, ComponentType::Output] {
            let converted = source.clone().into_type(target);

            match (converted, target) {
                (Component::Input(value), ComponentType::Input) => {
                    assert_eq!(value.count, if value.name == "input" { 3 } else { 4 });
                    assert_eq!(value.input_only, value.name == "input");
                }
                (Component::Output(value), ComponentType::Output) => {
                    assert_eq!(value.count, if value.name == "input" { 3 } else { 4 });
                    assert_eq!(value.output_only, 42);
                }
                _ => panic!("wrong target variant"),
            }
        }
    }

    let value = Box::new(Output::default());
    let pointer = std::ptr::from_ref(value.as_ref());
    let Component::Output(converted) = Component::Output(value).into_type(ComponentType::Output) else {
        panic!("wrong target variant");
    };
    assert_eq!(std::ptr::from_ref(converted.as_ref()), pointer);
}
