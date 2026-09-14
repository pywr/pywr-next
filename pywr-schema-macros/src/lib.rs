use heck::ToSnakeCase;
use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use syn::{Fields, ItemStruct, Type, parse_macro_input, parse_quote};

/// A derive macro for Pywr components that implement the `VisitMetrics`, `VisitPaths` and
/// `VisitReferences` traits.
#[proc_macro_derive(PywrVisitAll)]
pub fn pywr_visit_all_macro(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = syn::parse_macro_input!(input as syn::DeriveInput);

    let mut ts = impl_visit_metrics(&input);
    ts.extend(impl_visit_paths(&input));
    ts.extend(impl_visit_references(&input));

    ts
}

/// A derive macro for Pywr components that implement the `VisitMetrics` trait.
#[proc_macro_derive(PywrVisitMetrics)]
pub fn pywr_visit_metrics_macro(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    impl_visit_metrics(&input)
}

/// A derive macro for Pywr components that implement the `VisitPaths` trait.
#[proc_macro_derive(PywrVisitPaths)]
pub fn pywr_visit_paths_macro(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    impl_visit_paths(&input)
}

/// A derive macro for Pywr components that implement the `VisitReferences` trait.
#[proc_macro_derive(PywrVisitReferences)]
pub fn pywr_visit_references_macro(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    impl_visit_references(&input)
}

/// What distinguishes one visitor trait from another.
///
/// The three visitors differ only in the trait they implement, the values they hand the
/// callback, and what the generated module has to import. The walk over a struct's fields or an
/// enum's variants is the same for all of them, so it is written once in [`impl_visitor`].
struct VisitorSpec {
    /// The trait to implement, in `crate::visit`.
    trait_name: &'static str,
    /// The immutable method. The mutable one is this with a `_mut` suffix, and the generated
    /// module is named after the type and this.
    method: &'static str,
    /// The callback's argument type in each of the two methods.
    arg: syn::Type,
    arg_mut: syn::Type,
    /// What the generated module needs in scope beyond the trait itself.
    imports: syn::ItemUse,
}

fn impl_visit_metrics(ast: &syn::DeriveInput) -> TokenStream {
    impl_visitor(
        ast,
        &VisitorSpec {
            trait_name: "VisitMetrics",
            method: "visit_metrics",
            arg: parse_quote!(&Metric),
            arg_mut: parse_quote!(&mut Metric),
            imports: parse_quote!(
                use crate::metric::Metric;
            ),
        },
    )
}

fn impl_visit_paths(ast: &syn::DeriveInput) -> TokenStream {
    impl_visitor(
        ast,
        &VisitorSpec {
            trait_name: "VisitPaths",
            method: "visit_paths",
            arg: parse_quote!(&Path),
            arg_mut: parse_quote!(&mut PathBuf),
            imports: parse_quote!(
                use std::path::{Path, PathBuf};
            ),
        },
    )
}

fn impl_visit_references(ast: &syn::DeriveInput) -> TokenStream {
    impl_visitor(
        ast,
        &VisitorSpec {
            trait_name: "VisitReferences",
            method: "visit_references",
            arg: parse_quote!(Reference<'_>),
            arg_mut: parse_quote!(ReferenceMut<'_>),
            imports: parse_quote!(
                use crate::visit::{Reference, ReferenceMut};
            ),
        },
    )
}

/// The identifiers of a set of named fields.
fn field_names(fields: &Fields) -> impl Iterator<Item = &syn::Ident> {
    fields
        .iter()
        .map(|field| field.ident.as_ref().expect("Field must have an identifier"))
}

/// The body of one visitor method: a call per field for a struct, and a match over the variants
/// for an enum, recursing into the fields of each.
fn visitor_body(data: &syn::Data, method: &syn::Ident) -> impl ToTokens {
    match data {
        syn::Data::Struct(data) => {
            let names = field_names(&data.fields);
            quote! { #( self.#names.#method(visitor); )* }
        }
        syn::Data::Enum(data) => {
            let arms = data.variants.iter().map(|variant| {
                let ident = &variant.ident;

                match &variant.fields {
                    Fields::Unnamed(_) => quote! { Self::#ident(v) => v.#method(visitor), },
                    Fields::Named(_) => {
                        let names: Vec<_> = field_names(&variant.fields).collect();
                        quote! { Self::#ident { #( #names ),* } => { #( #names.#method(visitor); )* } }
                    }
                    Fields::Unit => quote! { Self::#ident => {} },
                }
            });

            quote! { match self { #( #arms )* } }
        }
        syn::Data::Union(_) => panic!("Union types are not supported."),
    }
}

/// Generate the implementation of one visitor trait for `ast`.
///
/// The impl goes in a private module so that the trait and the types its methods mention can be
/// imported without disturbing the surrounding scope.
fn impl_visitor(ast: &syn::DeriveInput, spec: &VisitorSpec) -> TokenStream {
    let VisitorSpec {
        trait_name,
        method,
        arg,
        arg_mut,
        imports,
    } = spec;
    let name = &ast.ident;
    let span = name.span();

    let mod_name = syn::Ident::new(&format!("{name}_{method}").to_snake_case(), span);
    let method_mut = syn::Ident::new(&format!("{method}_mut"), span);
    let method = syn::Ident::new(method, span);
    let trait_name = syn::Ident::new(trait_name, span);

    let body = visitor_body(&ast.data, &method);
    let body_mut = visitor_body(&ast.data, &method_mut);

    TokenStream::from(quote! {
        mod #mod_name {
            use super::*;
            use crate::visit::#trait_name;
            #imports

            impl #trait_name for #name {
                fn #method<F: FnMut(#arg)>(&self, visitor: &mut F) {
                    #body
                }

                fn #method_mut<F: FnMut(#arg_mut)>(&mut self, visitor: &mut F) {
                    #body_mut
                }
            }
        }
    })
}

/// An attribute macro to add `#[serde(skip_serializing_if = "Option::is_none")]` to all Option<T> fields in a struct
#[proc_macro_attribute]
pub fn skip_serializing_none(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the input as a TokenStream so we can preserve all original tokens, including doc comments

    // Parse the struct for field processing
    let input = parse_macro_input!(item as ItemStruct);
    let mut output = input.clone();

    for field in &mut output.fields {
        if let Type::Path(type_path) = &field.ty {
            if type_path
                .path
                .segments
                .last()
                .map(|s| s.ident == "Option")
                .unwrap_or(false)
            {
                // Only add if not already present
                let already_has = field.attrs.iter().any(|attr| {
                    attr.path().is_ident("serde") && attr.to_token_stream().to_string().contains("skip_serializing_if")
                });
                if !already_has {
                    field.attrs.push(syn::parse_quote!(
                        #[serde(skip_serializing_if = "Option::is_none")]
                    ));
                }
            }
        }
    }

    // Replace the struct definition in the original tokens with the modified one,
    // so that doc comments and formatting are preserved as in the original source.
    // This is a simple approach that works if the macro is only applied to structs.
    TokenStream::from(quote! { #output })
}
