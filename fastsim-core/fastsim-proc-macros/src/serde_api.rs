use crate::imports::*;
mod serde_utils;
use serde_utils::*;

/// Returns true if the struct's derive list includes `Default`.
fn struct_derives_default(ast: &syn::ItemStruct) -> bool {
    ast.attrs.iter().any(|attr| {
        if let Meta::List(ml) = &attr.meta {
            if !ml.path.is_ident("derive") {
                return false;
            }
            ml.tokens
                .to_string()
                .split(',')
                .any(|tok| tok.trim() == "Default")
        } else {
            false
        }
    })
}

/// Returns true if the field has `#[serde(default = "...")]` (a function form, not bare `default`).
fn field_has_serde_default_fn(field: &syn::Field) -> bool {
    field.attrs.iter().any(|attr| {
        if let Meta::List(ml) = &attr.meta {
            if !ml.path.is_ident("serde") {
                return false;
            }
            // Parse the comma-separated meta items inside #[serde(...)] and look
            // specifically for `default = "..."` (NameValue form), not bare `default`.
            let nested = ml.parse_args_with(
                syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
            );
            if let Ok(metas) = nested {
                metas
                    .iter()
                    .any(|m| matches!(m, Meta::NameValue(nv) if nv.path.is_ident("default")))
            } else {
                false
            }
        } else {
            false
        }
    })
}

pub(crate) fn serde_api(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut output = TokenStream2::default();

    let mut struct_ast = syn::parse_macro_input!(item as syn::ItemStruct);
    let struct_name = struct_ast.ident.clone();
    let helper_name = syn::Ident::new(
        &format!("{}DeserializeHelper", struct_name),
        struct_name.span(),
    );

    // Collect SI field data and #[py_get] field data
    let mut si_fields = vec![];
    let mut py_get_fields = vec![];

    let derives_default = struct_derives_default(&struct_ast);

    if let syn::Fields::Named(syn::FieldsNamed { named, .. }) = &mut struct_ast.fields {
        // struct with named fields
        for field in named.iter_mut() {
            // Strip #[py_get] FIRST so it doesn't appear in the emitted struct.
            if let Some(data) = serde_utils::collect_and_strip_py_get(field) {
                py_get_fields.push(data);
            }
            if derives_default && field_has_serde_default_fn(field) {
                let field_name = field
                    .ident
                    .as_ref()
                    .map(|i| i.to_string())
                    .unwrap_or_default();
                abort!(
                    field.span(),
                    "Field `{}` uses `#[serde(default = \"...\")]` but the struct derives \
                     `Default`. The derived `Default` will use `<FieldType>::default()` (typically \
                     zero) instead of the serde default function, causing inconsistent behavior \
                     when the struct is constructed in code vs. deserialized with the field absent. \
                     Remove `Default` from the `#[derive(...)]` list and implement it manually, \
                     calling the same default function for this field.",
                    field_name
                );
            }
            // Collect SI field data before modifying
            if let Some(data) = serde_utils::collect_si_field_data(field) {
                // Only include SI fields that have unit definitions
                if !data.units.is_empty() {
                    si_fields.push(data);
                }
            }
            serde_attrs_for_si_fields(field);
        }
    } else if let syn::Fields::Unnamed(syn::FieldsUnnamed { unnamed, .. }) = &mut struct_ast.fields
    {
        for field in unnamed.iter_mut() {
            serde_attrs_for_si_fields(field);
        }
    }

    // Add serde(try_from = "Helper") if we have SI fields.
    if !si_fields.is_empty() {
        let helper_name_str = helper_name.to_string();
        let try_from_attr: syn::Attribute = syn::parse_quote! {
            #[serde(try_from = #helper_name_str)]
        };
        struct_ast.attrs.push(try_from_attr);
    }

    output.extend(struct_ast.to_token_stream());

    // Generate serde helper struct + TryFrom when there are SI fields.
    if !si_fields.is_empty() {
        let helper_struct =
            serde_utils::generate_helper_struct(&helper_name, &struct_ast, &si_fields);
        let from_impl = serde_utils::generate_try_from_impl(
            &struct_name,
            &helper_name,
            &struct_ast,
            &si_fields,
        );
        output.extend(helper_struct);
        output.extend(from_impl);
    }

    // Generate #[pymethods] getter block when there are SI or #[py_get] fields.
    if !si_fields.is_empty() || !py_get_fields.is_empty() {
        let py_getters =
            serde_utils::generate_py_getters(&struct_name, &struct_ast, &si_fields, &py_get_fields);
        output.extend(py_getters);
    }

    output.into()
}
