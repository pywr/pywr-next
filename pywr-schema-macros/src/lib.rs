use heck::ToSnakeCase;
use proc_macro::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::Fields;

/// A derive macro for Pywr components that implement the `VisitMetrics`
/// and `VisitPaths` traits.
#[proc_macro_derive(PywrVisitAll)]
pub fn pywr_visit_all_macro(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = syn::parse_macro_input!(input as syn::DeriveInput);

    let mut ts = impl_visit_metrics(&input);
    ts.extend(impl_visit_paths(&input));

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

/// Generates a [`TokenStream`] containing the implementation of `VisitMetrics`.
fn impl_visit_metrics(ast: &syn::DeriveInput) -> TokenStream {
    // Name of the node type
    let name = &ast.ident;

    let expanded = match &ast.data {
        syn::Data::Struct(data) => {
            // Insert statements for non-mutable version
            let inserts = data
                .fields
                .iter()
                .map(|field| {
                    let name = field.ident.as_ref().expect("Field must have an identifier");
                    quote! {
                        self.#name.visit_metrics(visitor);
                    }
                })
                .collect::<Vec<_>>();

            // Insert statements for mutable version
            let inserts_mut = data
                .fields
                .iter()
                .map(|field| {
                    let name = field.ident.as_ref().expect("Field must have an identifier");
                    quote! {
                        self.#name.visit_metrics_mut(visitor);
                    }
                })
                .collect::<Vec<_>>();

            // Create the two parameter methods using the insert statements
            let mod_name = format!("{name}_visit_metrics").to_snake_case();
            let mod_name = syn::Ident::new(&mod_name, name.span());
            quote! {
                mod #mod_name {
                    use super::*;
                    use crate::visit::VisitMetrics;
                    use crate::metric::Metric;

                    impl VisitMetrics for #name {
                       fn visit_metrics<F: FnMut(&Metric)>(&self, visitor: &mut F) {

                            #(
                                #inserts
                            )*

                        }

                        fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, visitor: &mut F) {

                            #(
                                #inserts_mut
                            )*

                        }
                    }
                }
            }
        }
        syn::Data::Enum(data) => {
            let inserts = data
                .variants
                .iter()
                .map(|variant| {
                    let ident = &variant.ident;
                    match &variant.fields {
                        syn::Fields::Unnamed(_) => {
                            quote! {
                                Self::#ident(v) => v.visit_metrics(visitor),
                            }
                        }
                        syn::Fields::Named(fields) => {
                            let args: Punctuated<syn::Ident, Comma> =
                                fields.named.iter().map(|field| field.ident.clone().unwrap()).collect();

                            let inserts = &fields
                                .named
                                .iter()
                                .map(|field| {
                                    let name = field.ident.as_ref().expect("Field must have an identifier");
                                    quote! {
                                        #name.visit_metrics(visitor);
                                    }
                                })
                                .collect::<Vec<_>>();

                            quote! {
                                Self::#ident{ #args } => {
                                    #(
                                        #inserts
                                    )*
                                }
                            }
                        }
                        Fields::Unit => {
                            quote! {
                                Self::#ident => {}
                            }
                        }
                    }
                })
                .collect::<Vec<_>>();

            let inserts_mut = data
                .variants
                .iter()
                .map(|variant| {
                    let ident = &variant.ident;
                    match &variant.fields {
                        syn::Fields::Unnamed(_) => {
                            quote! {
                                Self::#ident(v) => v.visit_metrics_mut(visitor),
                            }
                        }
                        syn::Fields::Named(fields) => {
                            let args: Punctuated<syn::Ident, Comma> =
                                fields.named.iter().map(|field| field.ident.clone().unwrap()).collect();

                            let inserts = &fields
                                .named
                                .iter()
                                .map(|field| {
                                    let name = field.ident.as_ref().expect("Field must have an identifier");
                                    quote! {
                                        #name.visit_metrics_mut(visitor);
                                    }
                                })
                                .collect::<Vec<_>>();

                            quote! {
                                Self::#ident{ #args } => {
                                    #(
                                        #inserts
                                    )*
                                }
                            }
                        }
                        Fields::Unit => {
                            quote! {
                                Self::#ident => {}
                            }
                        }
                    }
                })
                .collect::<Vec<_>>();

            // Create the two parameter methods using the insert statements
            let mod_name = format!("{name}_visit_metrics").to_snake_case();
            let mod_name = syn::Ident::new(&mod_name, name.span());
            quote! {
                mod #mod_name {
                    use super::*;
                    use crate::visit::VisitMetrics;
                    use crate::metric::Metric;

                    impl VisitMetrics for #name {
                       fn visit_metrics<F: FnMut(&Metric)>(&self, visitor: &mut F) {
                            match self {
                                #(
                                    #inserts
                                )*
                            }
                        }

                        fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, visitor: &mut F) {
                            match self {
                                #(
                                    #inserts_mut
                                )*
                            }
                        }
                    }
                }
            }
        }
        syn::Data::Union(_) => panic!("Only structs are supported for #[derive(PywrNode)]"),
    };
    // Hand the output tokens back to the compiler.
    TokenStream::from(expanded)
}

/// Generates a [`TokenStream`] containing the implementation of `VisitPaths`.
fn impl_visit_paths(ast: &syn::DeriveInput) -> TokenStream {
    // Name of the node type
    let name = &ast.ident;

    let expanded = match &ast.data {
        syn::Data::Struct(data) => {
            // Only apply this to structs

            // Insert statements for non-mutable version
            let inserts = data
                .fields
                .iter()
                .map(|field| {
                    let name = field.ident.as_ref().expect("Field must have an identifier");
                    quote! {
                        self.#name.visit_paths(visitor);
                    }
                })
                .collect::<Vec<_>>();

            // Insert statements for mutable version
            let inserts_mut = data
                .fields
                .iter()
                .map(|field| {
                    let name = field.ident.as_ref().expect("Field must have an identifier");
                    quote! {
                        self.#name.visit_paths_mut(visitor);
                    }
                })
                .collect::<Vec<_>>();

            let mod_name = format!("{name}_visit_paths").to_snake_case();
            let mod_name = syn::Ident::new(&mod_name, name.span());
            // Create the two parameter methods using the insert statements
            quote! {
                mod #mod_name {
                    use super::*;
                    use crate::visit::VisitPaths;
                    use std::path::{Path, PathBuf};

                    impl VisitPaths for #name {
                       fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {

                            #(
                                #inserts
                            )*

                        }

                        fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {

                            #(
                                #inserts_mut
                            )*

                        }
                    }
                }
            }
        }
        syn::Data::Enum(data) => {
            let inserts = data
                .variants
                .iter()
                .map(|variant| {
                    let ident = &variant.ident;
                    match &variant.fields {
                        syn::Fields::Unnamed(_) => {
                            quote! {
                                Self::#ident(v) => v.visit_paths(visitor),
                            }
                        }
                        syn::Fields::Named(fields) => {
                            let args: Punctuated<syn::Ident, Comma> =
                                fields.named.iter().map(|field| field.ident.clone().unwrap()).collect();

                            let inserts = &fields
                                .named
                                .iter()
                                .map(|field| {
                                    let name = field.ident.as_ref().expect("Field must have an identifier");
                                    quote! {
                                        #name.visit_paths(visitor);
                                    }
                                })
                                .collect::<Vec<_>>();

                            quote! {
                                Self::#ident{ #args } => {
                                    #(
                                        #inserts
                                    )*
                                }
                            }
                        }
                        Fields::Unit => {
                            quote! {
                                Self::#ident => {}
                            }
                        }
                    }
                })
                .collect::<Vec<_>>();

            let inserts_mut = data
                .variants
                .iter()
                .map(|variant| {
                    let ident = &variant.ident;
                    match &variant.fields {
                        syn::Fields::Unnamed(_) => {
                            quote! {
                                Self::#ident(v) => v.visit_paths_mut(visitor),
                            }
                        }
                        syn::Fields::Named(fields) => {
                            let args: Punctuated<syn::Ident, Comma> =
                                fields.named.iter().map(|field| field.ident.clone().unwrap()).collect();

                            let inserts = &fields
                                .named
                                .iter()
                                .map(|field| {
                                    let name = field.ident.as_ref().expect("Field must have an identifier");
                                    quote! {
                                        #name.visit_paths_mut(visitor);
                                    }
                                })
                                .collect::<Vec<_>>();

                            quote! {
                                Self::#ident{ #args } => {
                                    #(
                                        #inserts
                                    )*
                                }
                            }
                        }
                        Fields::Unit => {
                            quote! {
                                Self::#ident => {}
                            }
                        }
                    }
                })
                .collect::<Vec<_>>();

            // Create the two parameter methods using the insert statements
            let mod_name = format!("{name}_visit_paths").to_snake_case();
            let mod_name = syn::Ident::new(&mod_name, name.span());
            quote! {
                mod #mod_name {
                    use super::*;
                    use crate::visit::VisitPaths;
                    use std::path::{Path, PathBuf};

                    impl VisitPaths for #name {
                       fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
                            match self {
                                #(
                                    #inserts
                                )*
                            }
                        }

                        fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
                            match self {
                                #(
                                    #inserts_mut
                                )*
                            }
                        }
                    }
                }
            }
        }
        syn::Data::Union(_) => panic!("Only structs are supported for #[derive(PywrNode)]"),
    };
    // Hand the output tokens back to the compiler.
    TokenStream::from(expanded)
}
