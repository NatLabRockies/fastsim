use crate::imports::*;
use crate::utilities::TokenStreamIterator;

lazy_static! {
    static ref ENERGY_REGEX: Regex = Regex::new(r"energy_(\w+)").unwrap();
}

pub(crate) fn state_methods_derive(input: TokenStream) -> TokenStream {
    let item_struct = syn::parse_macro_input!(input as syn::ItemStruct);
    let ident = &item_struct.ident;
    let mut impl_block = TokenStream2::default();

    let fields = if let syn::Fields::Named(syn::FieldsNamed { named, .. }) = item_struct.fields {
        named
    } else {
        abort_call_site!("`StateMethods` works only on Named Field structs.")
    };

    let struct_is_state = item_struct
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("is_state"));
    let struct_has_save_interval = fields
        .iter()
        .any(|x| *x.ident.as_ref().unwrap() == "save_interval");
    // A field is recursed into if it's marked `#[has_state]` (its type *contains* nested
    // state) or `#[is_state]` (its type itself *is* a state struct). Both are handled
    // identically today; the distinction is kept explicit to allow future divergence.
    let fields_with_state_vec: Vec<bool> = fields
        .iter()
        .map(|field| {
            field
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("has_state") || attr.path().is_ident("is_state"))
        })
        .collect();

    // fields that participate in nested state-tracking, i.e. fields explicitly marked
    // `#[has_state]` or `#[is_state]` (this includes the primary `state` field itself,
    // when present, since it must also carry `#[is_state]`)
    let fields_with_state = fields
        .iter()
        .zip(fields_with_state_vec)
        .filter(|(_f, hsv)| *hsv)
        .map(|(f, _hsv)| f.ident.as_ref().unwrap())
        .collect::<Vec<_>>();

    // whether this struct owns a primary `state: ...` field, as opposed to merely
    // containing other has_state/is_state sub-component fields
    let struct_has_state = fields_with_state.iter().any(|f| *f == "state");

    // Types of fields tagged `#[is_state]` (as opposed to `#[has_state]`). Each such type
    // is asserted below to implement the `IsState` marker trait, which is only implemented
    // for structs that themselves derive `#[is_state]`. This turns a field mistagged
    // `#[is_state]` (whose type isn't actually a state struct) into a compile error
    // instead of silent drift, since both tags are otherwise handled identically by this
    // derive. It does not catch the opposite mistake (`#[has_state]` on a field whose type
    // happens to be a state struct) since that's not observably wrong today.
    let is_state_field_types: Vec<&syn::Type> = fields
        .iter()
        .filter(|f| f.attrs.iter().any(|attr| attr.path().is_ident("is_state")))
        .map(|f| &f.ty)
        .collect();

    let all_fields = fields
        .iter()
        .map(|f| f.ident.as_ref().unwrap())
        .collect::<Vec<_>>();

    let (self_step, self_reset_step): (TokenStream2, TokenStream2) = if struct_is_state {
        (
            quote! {
                self.i.increment(1, || format_dbg!())?;
            },
            quote! {
                self.i.update_unchecked(0, || format_dbg!())?;
            },
        )
    } else {
        (quote! {}, quote! {})
    };

    impl_block.extend::<TokenStream2>(quote! {
        impl Step for #ident {
            fn step<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
                #self_step
                #(self.#fields_with_state.step(|| format!("{}\n{}", loc(), stringify!(#fields_with_state)))?;)*
                Ok(())
            }

            fn reset_step<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
                #self_reset_step
                #(self.#fields_with_state.reset_step(|| format!("{}\n{}", loc(), stringify!(#fields_with_state)))?;)*
                Ok(())
            }
        }
    });

    let self_save_state: TokenStream2 = if struct_has_state {
        quote! {self.history.push(self.state.clone());}
    } else {
        quote! {}
    };

    if struct_is_state {
        impl_block.extend::<TokenStream2>(quote! {
            #[automatically_derived]
            impl TrackedStateMethods for #ident {
                fn check_and_reset<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
                    #(
                        self.#all_fields.check_and_reset(|| format!("{}\n    `{}` has not been updated", loc(), stringify!(#all_fields)))?;
                    )*
                    Ok(())
                }

                fn mark_fresh<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
                    #(
                        self.#all_fields.mark_fresh(|| format!("{}\n    `{}` has already been updated", loc(), stringify!(#all_fields)))?;
                    )*
                    Ok(())
                }
            }
        });
    } else {
        impl_block.extend::<TokenStream2>(quote! {
            #[automatically_derived]
            impl TrackedStateMethods for #ident {
                fn check_and_reset<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
                    #(
                        self.#fields_with_state.check_and_reset(|| format!("{}\n    field in `{}` has not been updated", loc(), stringify!(#fields_with_state)))?;
                    )*
                    Ok(())
                }

                fn mark_fresh<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
                    #(
                        self.#fields_with_state.mark_fresh(|| format!("{}\n    field in `{}` has already been updated", loc(), stringify!(#fields_with_state)))?;
                    )*
                    Ok(())
                }
            }
        });
    }

    impl_block.extend::<TokenStream2>(quote! {
        impl StateMethods for #ident {}
    });

    if struct_is_state {
        impl_block.extend::<TokenStream2>(quote! {
            #[automatically_derived]
            impl IsState for #ident {}
        });
    }

    // Compile-time check that every `#[is_state]` field's type actually implements
    // `IsState` (i.e. that type's own struct derives `StateMethods` with `#[is_state]`
    // on it). `const _` items are anonymous, so this is safe to emit once per field with
    // no naming collisions.
    impl_block.extend::<TokenStream2>(
        is_state_field_types
            .iter()
            .map(|ty| {
                quote! {
                    const _: fn() = || {
                        fn assert_impl_is_state<T: IsState>() {}
                        assert_impl_is_state::<#ty>();
                    };
                }
            })
            .concat(),
    );

    if struct_has_save_interval {
        impl_block.extend::<TokenStream2>(quote! {
            #[automatically_derived]
            impl SaveState for #ident {
                /// Implementation for structs with `save_interval`
                fn save_state<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
                    if let Some(interval) = self.save_interval {
                        if *self.state.i.get_fresh(|| format!("{}\n{}\n`{}.state.i` has not been updated", loc(), format_dbg!(), stringify!(#ident)))? % interval == (0 as usize)
                            || *self.state.i.get_fresh(|| format!("{}\n{}\n`{}.state.i` has not been updated", loc(), format_dbg!(), stringify!(#ident)))? == (1 as usize)
                        {
                            #self_save_state
                            #(self.#fields_with_state.save_state(
                                || format!(
                                    "{}\n{}\n{} has not been updated",
                                    loc(),
                                    format_dbg!(),
                                    stringify!(#fields_with_state)
                                )
                            )?;)*
                        }
                    }
                    Ok(())
                }
            }
        });
    } else {
        impl_block.extend::<TokenStream2>(quote! {
            #[automatically_derived]
            impl SaveState for #ident {
                /// Implementation for objects without `save_interval`
                fn save_state<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
                    #self_save_state
                    #(self.#fields_with_state.save_state(
                        || format!(
                            "{}\n{}\n{} has not been updated",
                            loc(),
                            format_dbg!(),
                            stringify!(#fields_with_state)
                        )
                    )?;)*
                    Ok(())
                }
            }
        });
    }

    impl_block.into()
}
