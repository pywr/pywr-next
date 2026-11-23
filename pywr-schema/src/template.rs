//! `${name}` templates in the string values of a schema's JSON.
//!
//! A template is replaced by the value of the variable `name`, which is always a string; see
//! [`substitute`]. Templates are found in string values only, and are not allowed where
//! replacing them would stop references or typed deserialisation from being meaningful:
//!
//! - object keys;
//! - the value of a `type` member, which chooses the kind of a component;
//! - the `name` in a `meta` object, which other components refer to.
//!
//! `$${` gives a literal `${`. The string is read from left to right, so `$$${x}` is `$` followed
//! by the literal text `${x}`. A value substituted for a variable is inserted as it is, and is
//! not itself searched for templates.
//!
//! A variable name is made of ASCII letters, digits, `_` and `-`; see [`is_valid_name`].

use serde_json::Value;
use std::fmt;
use thiserror::Error;

/// One use of a variable in a schema's JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateUse {
    /// The name of the variable.
    pub name: String,
    /// The JSON pointer of the string value holding the template, such as
    /// `/network/time_series/0/url`.
    pub pointer: String,
    /// Whether the template starts the string value, so that its value comes first.
    pub at_start: bool,
}

/// A place in a schema's JSON where templates are not allowed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForbiddenField {
    /// An object key.
    Key,
    /// The value of a `type` member.
    Type,
    /// The `name` member of a `meta` object.
    MetaName,
}

impl fmt::Display for ForbiddenField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Key => "an object key",
            Self::Type => "a `type` value",
            Self::MetaName => "a component's `meta.name`",
        })
    }
}

/// A problem with the templates in a schema's JSON.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum TemplateError {
    #[error("A template at `{pointer}` has no closing `}}`.")]
    Unterminated { pointer: String },
    #[error("`{name}` at `{pointer}` is not a valid variable name; use ASCII letters, digits, `_` and `-`.")]
    InvalidName { pointer: String, name: String },
    #[error("A template at `{pointer}` is in {field}, which cannot be templated.")]
    Forbidden { pointer: String, field: ForbiddenField },
    #[error("Variable `{name}` used at `{pointer}` is not defined.")]
    Undefined { pointer: String, name: String },
    #[error("Variable `{name}` cannot be used at `{pointer}`: {reason}")]
    Rejected {
        pointer: String,
        name: String,
        reason: String,
    },
}

/// Why a lookup given to [`substitute`] gave no value for a variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LookupError {
    /// There is no variable of that name.
    Undefined,
    /// The variable exists but cannot be used where it is, for the reason given.
    Rejected(String),
}

/// The variables used in a schema's JSON, and the problems found with its templates.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scan {
    /// Every use of a variable, in document order.
    pub uses: Vec<TemplateUse>,
    /// Every problem, in document order. A string value with a problem contributes no uses.
    pub problems: Vec<TemplateError>,
}

/// Whether `name` can name a variable: one or more ASCII letters, digits, `_` and `-`.
pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

/// Find every template in `value`, together with every problem with them.
pub fn scan(value: &Value) -> Scan {
    let mut scan = Scan::default();
    walk(
        value,
        None,
        None,
        &mut String::new(),
        &mut |pointer, string| match parse_string(pointer, string) {
            Ok(segments) => scan
                .uses
                .extend(segments.into_iter().filter_map(|segment| match segment {
                    Segment::Variable { name, at_start } => Some(TemplateUse {
                        name: name.to_string(),
                        pointer: pointer.to_string(),
                        at_start,
                    }),
                    Segment::Text(_) => None,
                })),
            Err(problem) => scan.problems.push(problem),
        },
    );
    scan
}

/// Replace each template in `value` with the value that `lookup` gives for it, and each `$${`
/// with `${`.
///
/// Stops at the first problem, either with a template or from `lookup`, and then leaves `value`
/// unchanged. [`scan`] finds every problem with the templates themselves.
pub fn substitute<F>(value: &mut Value, mut lookup: F) -> Result<(), TemplateError>
where
    F: FnMut(&TemplateUse) -> Result<String, LookupError>,
{
    let mut replacements = Vec::new();
    let mut error = None;
    walk(value, None, None, &mut String::new(), &mut |pointer, string| {
        if error.is_some() {
            return;
        }
        // Keys are never rewritten, so only check them.
        if pointer.forbidden == Some(ForbiddenField::Key) {
            error = parse_string(pointer, string).err();
            return;
        }
        match render(pointer, string, &mut lookup) {
            Ok(Some(rendered)) => replacements.push((pointer.to_string(), rendered)),
            Ok(None) => {}
            Err(problem) => error = Some(problem),
        }
    });
    if let Some(error) = error {
        return Err(error);
    }

    for (pointer, rendered) in replacements {
        if let Some(target) = value.pointer_mut(&pointer) {
            *target = Value::String(rendered);
        }
    }
    Ok(())
}

/// The string to put in place of `string`, or `None` if it holds no templates or escapes.
fn render<F>(pointer: &Field<'_>, string: &str, lookup: &mut F) -> Result<Option<String>, TemplateError>
where
    F: FnMut(&TemplateUse) -> Result<String, LookupError>,
{
    if !string.contains('$') {
        return Ok(None);
    }
    let segments = parse_string(pointer, string)?;
    if let [Segment::Text(text)] = segments.as_slice()
        && *text == string
    {
        return Ok(None);
    }

    let mut rendered = String::with_capacity(string.len());
    for segment in segments {
        match segment {
            Segment::Text(text) => rendered.push_str(text),
            Segment::Variable { name, at_start } => {
                let used = TemplateUse {
                    name: name.to_string(),
                    pointer: pointer.to_string(),
                    at_start,
                };
                match lookup(&used) {
                    Ok(value) => rendered.push_str(&value),
                    Err(LookupError::Undefined) => {
                        return Err(TemplateError::Undefined {
                            pointer: used.pointer,
                            name: used.name,
                        });
                    }
                    Err(LookupError::Rejected(reason)) => {
                        return Err(TemplateError::Rejected {
                            pointer: used.pointer,
                            name: used.name,
                            reason,
                        });
                    }
                }
            }
        }
    }
    Ok(Some(rendered))
}

/// A string found while walking the JSON: where it is, and whether it may hold templates.
struct Field<'p> {
    pointer: &'p str,
    forbidden: Option<ForbiddenField>,
}

impl fmt::Display for Field<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.pointer)
    }
}

/// Parse `string`, found at `field`, into text and variables. A variable in a forbidden field
/// is a problem; an escape is not.
fn parse_string<'s>(field: &Field<'_>, string: &'s str) -> Result<Vec<Segment<'s>>, TemplateError> {
    let segments = parse(string).map_err(|error| match error {
        ParseError::Unterminated => TemplateError::Unterminated {
            pointer: field.to_string(),
        },
        ParseError::InvalidName(name) => match field.forbidden {
            // A key such as `${not a name}` is a template in a key, not a bad name.
            Some(ForbiddenField::Key) => TemplateError::Forbidden {
                pointer: field.to_string(),
                field: ForbiddenField::Key,
            },
            _ => TemplateError::InvalidName {
                pointer: field.to_string(),
                name: name.to_string(),
            },
        },
    })?;
    if let Some(forbidden) = field.forbidden
        && segments
            .iter()
            .any(|segment| matches!(segment, Segment::Variable { .. }))
    {
        return Err(TemplateError::Forbidden {
            pointer: field.to_string(),
            field: forbidden,
        });
    }
    Ok(segments)
}

/// Visit each string value and object key in `value` with its JSON pointer.
///
/// `key` is the name of the member `value` is the value of, and `object_key` the name of the
/// member that the object holding it is the value of; both are `None` for an array item or the
/// root. Object keys are visited as [`ForbiddenField::Key`] fields.
fn walk<'v, F>(value: &'v Value, key: Option<&str>, object_key: Option<&str>, pointer: &mut String, visit: &mut F)
where
    F: FnMut(&Field<'_>, &'v str),
{
    match value {
        Value::String(string) => {
            let forbidden = match (key, object_key) {
                (Some("type"), _) => Some(ForbiddenField::Type),
                (Some("name"), Some("meta")) => Some(ForbiddenField::MetaName),
                _ => None,
            };
            visit(&Field { pointer, forbidden }, string);
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                let len = pointer.len();
                pointer.push('/');
                pointer.push_str(&index.to_string());
                walk(item, None, None, pointer, visit);
                pointer.truncate(len);
            }
        }
        Value::Object(members) => {
            for (member, member_value) in members {
                let len = pointer.len();
                push_token(pointer, member);
                visit(
                    &Field {
                        pointer,
                        forbidden: Some(ForbiddenField::Key),
                    },
                    member,
                );
                walk(member_value, Some(member), key, pointer, visit);
                pointer.truncate(len);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

/// Append `token` to a JSON pointer, escaped as RFC 6901 requires.
fn push_token(pointer: &mut String, token: &str) {
    pointer.push('/');
    for c in token.chars() {
        match c {
            '~' => pointer.push_str("~0"),
            '/' => pointer.push_str("~1"),
            c => pointer.push(c),
        }
    }
}

/// A part of a string: literal text, or a variable.
#[derive(Debug, PartialEq, Eq)]
enum Segment<'s> {
    Text(&'s str),
    Variable { name: &'s str, at_start: bool },
}

#[derive(Debug, PartialEq, Eq)]
enum ParseError<'s> {
    Unterminated,
    InvalidName(&'s str),
}

/// Split `string` into literal text and variables, reading from left to right.
fn parse(string: &str) -> Result<Vec<Segment<'_>>, ParseError<'_>> {
    fn push_text<'s>(segments: &mut Vec<Segment<'s>>, text: &'s str) {
        if !text.is_empty() {
            segments.push(Segment::Text(text));
        }
    }

    let mut segments = Vec::new();
    let mut start = 0;
    while let Some(found) = string[start..].find('$') {
        let dollar = start + found;
        let rest = &string[dollar..];
        if rest.starts_with("$${") {
            push_text(&mut segments, &string[start..dollar]);
            segments.push(Segment::Text("${"));
            start = dollar + 3;
        } else if let Some(body) = rest.strip_prefix("${") {
            push_text(&mut segments, &string[start..dollar]);
            let end = body.find('}').ok_or(ParseError::Unterminated)?;
            let name = &body[..end];
            if !is_valid_name(name) {
                return Err(ParseError::InvalidName(name));
            }
            segments.push(Segment::Variable {
                name,
                at_start: dollar == 0,
            });
            start = dollar + 2 + end + 1;
        } else {
            push_text(&mut segments, &string[start..=dollar]);
            start = dollar + 1;
        }
    }
    push_text(&mut segments, &string[start..]);
    Ok(segments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn variables(name: &str) -> Result<String, LookupError> {
        match name {
            "dataset" => Ok("data/stoch-01".to_string()),
            "column" => Ok("2030".to_string()),
            "literal" => Ok("${dataset}".to_string()),
            _ => Err(LookupError::Undefined),
        }
    }

    fn substituted(value: Value) -> Result<Value, TemplateError> {
        let mut value = value;
        substitute(&mut value, |used| variables(&used.name))?;
        Ok(value)
    }

    #[test]
    fn strings_split_into_text_and_variables() {
        assert_eq!(
            parse("${a}/x-${b}"),
            Ok(vec![
                Segment::Variable {
                    name: "a",
                    at_start: true
                },
                Segment::Text("/x-"),
                Segment::Variable {
                    name: "b",
                    at_start: false
                },
            ])
        );
        assert_eq!(parse("cost $5"), Ok(vec![Segment::Text("cost $"), Segment::Text("5")]));
        assert_eq!(parse(""), Ok(vec![]));
        assert_eq!(parse("a${b"), Err(ParseError::Unterminated));
        assert_eq!(parse("${}"), Err(ParseError::InvalidName("")));
        assert_eq!(parse("${a b}"), Err(ParseError::InvalidName("a b")));
        assert_eq!(parse("${org::a}"), Err(ParseError::InvalidName("org::a")));
    }

    #[test]
    fn escapes_give_literal_text() {
        assert_eq!(parse("$${a}"), Ok(vec![Segment::Text("${"), Segment::Text("a}")]));
        assert_eq!(
            parse("$$${a}"),
            Ok(vec![Segment::Text("$"), Segment::Text("${"), Segment::Text("a}")])
        );
        assert_eq!(
            substituted(json!({ "url": "$${dataset}/${dataset}" })),
            Ok(json!({ "url": "${dataset}/data/stoch-01" }))
        );
    }

    #[test]
    fn valid_names_are_letters_digits_underscores_and_hyphens() {
        assert!(is_valid_name("inflow_dataset-2"));
        assert!(!is_valid_name(""));
        assert!(!is_valid_name("a.b"));
        assert!(!is_valid_name("a::b"));
        assert!(!is_valid_name("café"));
    }

    #[test]
    fn substitution_replaces_templates_throughout_the_document() {
        let value = json!({
            "time_series": [
                { "meta": { "name": "inflows" }, "type": "Csv", "url": "${dataset}/flows.csv" },
            ],
            "column": "demand-${column}",
            "count": 3,
        });
        assert_eq!(
            substituted(value),
            Ok(json!({
                "time_series": [
                    { "meta": { "name": "inflows" }, "type": "Csv", "url": "data/stoch-01/flows.csv" },
                ],
                "column": "demand-2030",
                "count": 3,
            }))
        );
    }

    #[test]
    fn substituted_values_are_not_searched_for_templates() {
        assert_eq!(substituted(json!("${literal}")), Ok(json!("${dataset}")));
    }

    #[test]
    fn lookup_sees_each_use() {
        let mut value = json!({ "a": ["${dataset}", "x/${column}"] });
        let mut seen = Vec::new();
        substitute(&mut value, |used| {
            seen.push(used.clone());
            variables(&used.name)
        })
        .unwrap();
        assert_eq!(
            seen,
            vec![
                TemplateUse {
                    name: "dataset".to_string(),
                    pointer: "/a/0".to_string(),
                    at_start: true
                },
                TemplateUse {
                    name: "column".to_string(),
                    pointer: "/a/1".to_string(),
                    at_start: false
                },
            ]
        );
    }

    #[test]
    fn lookup_failures_name_the_variable_and_leave_the_value_unchanged() {
        let original = json!({ "a": "${dataset}", "b": "${missing}" });
        let mut value = original.clone();
        assert_eq!(
            substitute(&mut value, |used| variables(&used.name)),
            Err(TemplateError::Undefined {
                pointer: "/b".to_string(),
                name: "missing".to_string()
            })
        );
        assert_eq!(value, original);

        let rejected = substitute(&mut json!({ "a": "x/${dataset}" }), |used| {
            if used.at_start {
                variables(&used.name)
            } else {
                Err(LookupError::Rejected("a path must start the string".to_string()))
            }
        });
        assert_eq!(
            rejected,
            Err(TemplateError::Rejected {
                pointer: "/a".to_string(),
                name: "dataset".to_string(),
                reason: "a path must start the string".to_string()
            })
        );
    }

    #[test]
    fn templates_are_forbidden_in_keys_types_and_component_names() {
        let value = json!({
            "${dataset}": 1,
            "nodes": [
                { "meta": { "name": "${dataset}", "comment": "${dataset}" }, "type": "${column}" },
            ],
            "name": "${dataset}",
        });
        let mut scan = scan(&value);
        scan.problems.sort_by_key(|problem| problem.to_string());
        assert_eq!(
            scan.problems,
            vec![
                TemplateError::Forbidden {
                    pointer: "/${dataset}".to_string(),
                    field: ForbiddenField::Key
                },
                TemplateError::Forbidden {
                    pointer: "/nodes/0/meta/name".to_string(),
                    field: ForbiddenField::MetaName
                },
                TemplateError::Forbidden {
                    pointer: "/nodes/0/type".to_string(),
                    field: ForbiddenField::Type
                },
            ]
        );
        // `meta.comment` and a `name` outside `meta` may be templated.
        let mut pointers: Vec<_> = scan.uses.iter().map(|used| used.pointer.as_str()).collect();
        pointers.sort();
        assert_eq!(pointers, vec!["/name", "/nodes/0/meta/comment"]);

        assert_eq!(
            substituted(json!({ "type": "${column}" })),
            Err(TemplateError::Forbidden {
                pointer: "/type".to_string(),
                field: ForbiddenField::Type
            })
        );
    }

    #[test]
    fn escapes_are_allowed_in_forbidden_fields_and_left_in_keys() {
        let value = json!({ "$${a}": { "meta": { "name": "cost $${a}" }, "type": "$x" } });
        assert_eq!(scan(&value), Scan::default());
        assert_eq!(
            substituted(value),
            Ok(json!({ "$${a}": { "meta": { "name": "cost ${a}" }, "type": "$x" } }))
        );
    }

    #[test]
    fn scan_reports_every_problem_with_escaped_pointers() {
        // `a/b` comes before `c~d` whether or not object members keep their order.
        let value = json!({ "a/b": "${x", "c~d": ["${bad name}", "${ok}"] });
        assert_eq!(
            scan(&value),
            Scan {
                uses: vec![TemplateUse {
                    name: "ok".to_string(),
                    pointer: "/c~0d/1".to_string(),
                    at_start: true
                }],
                problems: vec![
                    TemplateError::Unterminated {
                        pointer: "/a~1b".to_string()
                    },
                    TemplateError::InvalidName {
                        pointer: "/c~0d/0".to_string(),
                        name: "bad name".to_string()
                    },
                ],
            }
        );
    }
}
