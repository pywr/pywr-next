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

/// Generate `From<Source>` for every *other* payload struct in an enum.
///
/// The derive cannot inspect a type's definition from its name alone, so `files`
/// must list the source files containing the variant structs, relative to the
/// consuming crate's manifest directory. Each variant must contain one struct
/// (optionally boxed); conversions operate on the unboxed struct types.
/// Fields with the same name and type are moved; remaining fields retain the
/// destination struct's `Default` values.
/// `generate_defaults` can opt into generating defaults for payloads without
/// their own Default implementation; `delay_default` sets the nonzero delay.
///
/// `#[pywr_from_all_other_variants(files("src/nodes/core.rs", "src/nodes/delay.rs"))]`
#[proc_macro_derive(PywrFromAllOtherVariants, attributes(pywr_from_all_other_variants))]
pub fn pywr_from_all_other_variants_macro(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    match derive_from_all_other_variants(&input) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error().into(),
    }
}

/// Generate an exhaustive `into_type(self, target_type: EnumType) -> Self` method.
///
/// Requires `EnumDiscriminants` to create `EnumType` (e.g. `NodeType` for `Node`)
/// and `From<SourcePayload> for TargetPayload` for every distinct pair, as provided
/// by `PywrFromAllOtherVariants`. Single-field tuple variants may contain a
/// payload directly or in a `Box`. Identity conversions return the original value.
#[proc_macro_derive(PywrIntoType)]
pub fn pywr_into_type_macro(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    match derive_into_type(&input) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error().into(),
    }
}

fn derive_into_type(input: &syn::DeriveInput) -> syn::Result<TokenStream> {
    let syn::Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(&input.ident, "PywrIntoType requires an enum"));
    };

    let mut variants = Vec::new();
    for variant in &data.variants {
        let Fields::Unnamed(fields) = &variant.fields else {
            return Err(syn::Error::new_spanned(
                variant,
                "expected a single-field tuple variant",
            ));
        };
        if fields.unnamed.len() != 1 {
            return Err(syn::Error::new_spanned(
                variant,
                "expected a single-field tuple variant",
            ));
        }
        let boxed = match &fields.unnamed[0].ty {
            Type::Path(path) => path.path.segments.last().is_some_and(|segment| segment.ident == "Box"),
            _ => false,
        };
        variants.push((&variant.ident, boxed));
    }

    let name = &input.ident;
    let type_name = syn::Ident::new(&format!("{name}Type"), name.span());
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let mut arms = Vec::new();
    for (from, from_boxed) in &variants {
        for (to, to_boxed) in &variants {
            let value = if from == to {
                quote!(value)
            } else {
                let source = if *from_boxed { quote!(*value) } else { quote!(value) };
                let converted = quote!(::std::convert::Into::into(#source));
                if *to_boxed {
                    quote!(::std::boxed::Box::new(#converted))
                } else {
                    converted
                }
            };
            arms.push(quote! { (Self::#from(value), #type_name::#to) => Self::#to(#value), });
        }
    }

    Ok(quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            /// Convert to another variant, preserving fields shared by the payloads.
            pub fn into_type(self, target_type: #type_name) -> Self {
                match (self, target_type) {
                    #(#arms)*
                }
            }
        }
    }
    .into())
}

fn derive_from_all_other_variants(input: &syn::DeriveInput) -> syn::Result<TokenStream> {
    let syn::Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "PywrFromAllOtherVariants requires an enum",
        ));
    };

    let mut files = Vec::new();
    let mut generate_defaults = false;
    let mut delay_default: Option<syn::Expr> = None;
    for attr in &input.attrs {
        if attr.path().is_ident("pywr_from_all_other_variants") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("files") {
                    let content;
                    syn::parenthesized!(content in meta.input);
                    let paths = content.parse_terminated(|input| input.parse::<syn::LitStr>(), syn::Token![,])?;
                    files.extend(paths);
                    Ok(())
                } else if meta.path.is_ident("generate_defaults") {
                    generate_defaults = true;
                    Ok(())
                } else if meta.path.is_ident("delay_default") {
                    delay_default = Some(meta.value()?.parse()?);
                    Ok(())
                } else {
                    Err(meta.error("expected files(...), generate_defaults, or delay_default = expr"))
                }
            })?;
        }
    }
    if files.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "provide #[pywr_from_all_other_variants(files(\"src/path.rs\", ...))]",
        ));
    }

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map_err(|_| syn::Error::new_spanned(&input.ident, "CARGO_MANIFEST_DIR is not set"))?;
    let mut structs = std::collections::HashMap::new();
    let mut file_dependencies = Vec::new();
    for path in files {
        let full_path = std::path::Path::new(&manifest_dir).join(path.value());
        let source = std::fs::read_to_string(&full_path)
            .map_err(|err| syn::Error::new_spanned(&path, format!("cannot read {}: {err}", full_path.display())))?;
        let parsed = syn::parse_file(&source)
            .map_err(|err| syn::Error::new_spanned(&path, format!("cannot parse {}: {err}", full_path.display())))?;
        for item in parsed.items {
            if let syn::Item::Struct(item) = item {
                structs.insert(item.ident.to_string(), item);
            }
        }
        file_dependencies.push(quote! {
            const _: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", #path));
        });
    }

    let mut payloads = Vec::new();
    for variant in &data.variants {
        let Fields::Unnamed(fields) = &variant.fields else {
            return Err(syn::Error::new_spanned(
                variant,
                "expected a single-field tuple variant",
            ));
        };
        if fields.unnamed.len() != 1 {
            return Err(syn::Error::new_spanned(
                variant,
                "expected a single-field tuple variant",
            ));
        }
        let mut ty = &fields.unnamed[0].ty;
        if let Type::Path(path) = ty {
            if path.path.segments.last().is_some_and(|segment| segment.ident == "Box") {
                let segment = path.path.segments.last().unwrap();
                let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
                    return Err(syn::Error::new_spanned(ty, "expected Box<Struct>"));
                };
                let Some(syn::GenericArgument::Type(inner)) = args.args.first().filter(|_| args.args.len() == 1) else {
                    return Err(syn::Error::new_spanned(ty, "expected Box<Struct>"));
                };
                ty = inner;
            }
        }
        let syn::Type::Path(ty_path) = ty else {
            return Err(syn::Error::new_spanned(ty, "expected a struct payload type"));
        };
        let ident = &ty_path.path.segments.last().unwrap().ident;
        let item = structs
            .get(&ident.to_string())
            .ok_or_else(|| syn::Error::new_spanned(ty, format!("struct {ident} not found in the listed files")))?;
        if !matches!(item.fields, Fields::Named(_)) || !item.generics.params.is_empty() {
            return Err(syn::Error::new_spanned(
                ty,
                "expected a named-field, non-generic struct",
            ));
        }
        if payloads
            .iter()
            .any(|(_, previous): &(&Type, &ItemStruct)| previous.ident == *ident)
        {
            return Err(syn::Error::new_spanned(ty, "variant payload types must be distinct"));
        }
        payloads.push((ty, item));
    }

    let mut tokens = TokenStream::new();
    for dependency in file_dependencies {
        tokens.extend(TokenStream::from(dependency));
    }
    if generate_defaults {
        for (ty, item) in &payloads {
            let fields = item.fields.iter().map(|field| {
                let name = field.ident.as_ref().unwrap();
                let value = if name == "delay" && field.ty.to_token_stream().to_string() == "NonZeroU64" {
                    delay_default.as_ref().map(|expr| quote!(#expr))
                } else {
                    None
                }
                .unwrap_or_else(|| quote!(::std::default::Default::default()));
                quote!(#name: #value,)
            });
            tokens.extend(TokenStream::from(quote! {
                impl ::std::default::Default for #ty {
                    fn default() -> Self {
                        Self { #(#fields)* }
                    }
                }
            }));
        }
    }
    for (to_index, (to_type, to_struct)) in payloads.iter().enumerate() {
        for (from_index, (from_type, from_struct)) in payloads.iter().enumerate() {
            if to_index != from_index {
                tokens.extend(implement_from(to_type, to_struct, from_type, from_struct));
            }
        }
    }
    Ok(tokens)
}

/// Build a migration function that converts from one struct to another, field by field.
fn implement_from(into_ty: &Type, into_item: &ItemStruct, from_ty: &Type, from_item: &ItemStruct) -> TokenStream {
    let mut field_migrations = Vec::new();

    for into_field in &into_item.fields {
        let into_field_name = into_field.ident.as_ref().unwrap();
        let into_field_ty = &into_field.ty;

        if let Some(from_field) = from_item
            .fields
            .iter()
            .find(|f| f.ident.as_ref().unwrap() == into_field_name)
        {
            let from_field_ty = &from_field.ty;

            if into_field_ty.to_token_stream().to_string() == from_field_ty.to_token_stream().to_string() {
                field_migrations.push(quote! { into.#into_field_name = old.#into_field_name; });
            }
        }
    }

    TokenStream::from(quote! {
        impl ::std::convert::From<#from_ty> for #into_ty {
            fn from(old: #from_ty) -> Self {
                let mut into = Self::default();
                #(#field_migrations)*
                into
            }
        }
    })
}
