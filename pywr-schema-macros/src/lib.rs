use proc_macro::TokenStream;
use quote::quote;

/// A derive macro for Pywr nodes that implements `parameters` and `parameters_mut` methods.
#[proc_macro_derive(PywrNode)]
pub fn pywr_node_macro(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    impl_parameter_references_derive(&input)
}

enum PywrField {
    Optional(syn::Ident),
    Required(syn::Ident),
}

/// Generates a [`TokenStream`] containing the implementation of two methods, `parameters`
/// and `parameters_mut`, for the given struct.
///
/// Both method returns a [`HashMap`] of parameter names to [`DynamicFloatValue`]. This
/// is intended to be used for nodes and parameter structs in the Pywr schema.
fn impl_parameter_references_derive(ast: &syn::DeriveInput) -> TokenStream {
    // Name of the node type
    let name = &ast.ident;

    if let syn::Data::Struct(data) = &ast.data {
        // Only apply this to structs

        // Help struct for capturing parameter fields and whether they are optional.
        struct ParamField {
            field_name: syn::Ident,
            optional: bool,
        }

        // Iterate through all fields of the struct. Try to find fields that reference
        // parameters (e.g. `Option<DynamicFloatValue>` or `ParameterValue`).
        let parameter_fields: Vec<ParamField> = data
            .fields
            .iter()
            .filter_map(|field| {
                let field_ident = field.ident.as_ref()?;
                // Identify optional fields
                match type_to_ident(&field.ty) {
                    Some(PywrField::Optional(ident)) => {
                        // If optional and a parameter identifier then add to the list
                        is_parameter_ident(&ident).then_some(ParamField {
                            field_name: field_ident.clone(),
                            optional: true,
                        })
                    }
                    Some(PywrField::Required(ident)) => {
                        // Otherwise, if a parameter identifier then add to the list
                        is_parameter_ident(&ident).then_some(ParamField {
                            field_name: field_ident.clone(),
                            optional: false,
                        })
                    }
                    None => None, // All other fields are ignored.
                }
            })
            .collect();

        // Insert statements for non-mutable version
        let inserts = parameter_fields
            .iter()
            .map(|param_field| {
                let ident = &param_field.field_name;
                let key = ident.to_string();
                if param_field.optional {
                    quote! {
                        if let Some(p) = &self.#ident {
                            attributes.insert(#key, p.into());
                        }
                    }
                } else {
                    quote! {
                        let #ident = &self.#ident;
                        attributes.insert(#key, #ident.into());
                    }
                }
            })
            .collect::<Vec<_>>();

        // Insert statements for mutable version
        let inserts_mut = parameter_fields
            .iter()
            .map(|param_field| {
                let ident = &param_field.field_name;
                let key = ident.to_string();
                if param_field.optional {
                    quote! {
                        if let Some(p) = &mut self.#ident {
                            attributes.insert(#key, p.into());
                        }
                    }
                } else {
                    quote! {
                        let #ident = &mut self.#ident;
                        attributes.insert(#key, #ident.into());
                    }
                }
            })
            .collect::<Vec<_>>();

        // Create the two parameter methods using the insert statements
        let expanded = quote! {
            impl #name {
                pub fn parameters(&self) -> HashMap<&str, &DynamicFloatValue> {
                    let mut attributes = HashMap::new();
                    #(
                        #inserts
                    )*
                    attributes
                }

                pub fn parameters_mut(&mut self) -> HashMap<&str, &mut DynamicFloatValue> {
                    let mut attributes = HashMap::new();
                    #(
                        #inserts_mut
                    )*
                    attributes
                }
            }
        };

        // Hand the output tokens back to the compiler.
        TokenStream::from(expanded)
    } else {
        panic!("Only structs are supported for #[derive(PywrNode)]")
    }
}

/// Returns the last segment of a type path as an identifier
fn type_to_ident(ty: &syn::Type) -> Option<PywrField> {
    match ty {
        // Match type's that are a path and not a self type.
        syn::Type::Path(type_path) if type_path.qself.is_none() => {
            // Match on the last segment
            match type_path.path.segments.last() {
                Some(last_segment) => {
                    let ident = &last_segment.ident;

                    if ident == "Option" {
                        // The last segment is an Option, now we need to parse the argument
                        // I.e. the bit in inside the angle brackets.
                        let first_arg = match &last_segment.arguments {
                            syn::PathArguments::AngleBracketed(params) => params.args.first(),
                            _ => None,
                        };

                        // Find type arguments; ignore others
                        let arg_ty = match first_arg {
                            Some(syn::GenericArgument::Type(ty)) => Some(ty),
                            _ => None,
                        };

                        // Match on path types that are no self types.
                        let arg_type_path = match arg_ty {
                            Some(ty) => match ty {
                                syn::Type::Path(type_path) if type_path.qself.is_none() => Some(type_path),
                                _ => None,
                            },
                            None => None,
                        };

                        // Get the last segment of the path
                        let last_segment = match arg_type_path {
                            Some(type_path) => type_path.path.segments.last(),
                            None => None,
                        };

                        // Finally, if there's a last segment return this as an optional `PywrField`
                        match last_segment {
                            Some(last_segment) => {
                                let ident = &last_segment.ident;
                                Some(PywrField::Optional(ident.clone()))
                            }
                            None => None,
                        }
                    } else {
                        // Otherwise, assume this a simple required field
                        Some(PywrField::Required(ident.clone()))
                    }
                }
                None => None,
            }
        }
        _ => None,
    }
}

fn is_parameter_ident(ident: &syn::Ident) -> bool {
    // TODO this currenty omits more complex attributes, such as `factors` for AggregatedNode
    // and steps for PiecewiseLinks, that can internally contain `DynamicFloatValue` fields
    ident == "DynamicFloatValue"
}
