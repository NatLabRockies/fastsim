use crate::imports::*;

/// Converts multiple uom unit values to a vector of token stream and the plural units name
///
/// - field_units: unit type of value being set (e.g. `uom::si::power::watt`)
macro_rules! extract_units {
    ($($field_units: ty),+) => {{
        let mut unit_impls = vec![];
        $(
            let field_units: TokenStream2 = stringify!($field_units).parse().expect("failed to parse `field_units`");
            let mut unit_name = <$field_units as uom::si::Unit>::plural().to_lowercase().replace(' ', "_");

            // UOM has a bug where ratio.plural() returns an empty string
            // Default to "ratio" for the ratio unit when plural() is empty
            if unit_name.is_empty() {
                let debug_name = stringify!($field_units);
                if debug_name.contains("ratio") {
                    unit_name = "ratio".to_string();
                }
            }

            // fix UOM pluralization edge cases
            let unit_name = unit_name.replace("kelvins", "kelvin");
            let unit_name = unit_name.replace("ratios", "ratio");
            // replace hyphens (e.g. "pounds-force") so the name is a valid identifier
            let unit_name = unit_name.replace('-', "_");
            unit_impls.push((field_units, unit_name));
        )+
        unit_impls
    }};
}

/// Like extract_units!, but for custom units defined outside of the uom crate whose types
/// are not available in the proc-macro binary. Each entry is a `("type::path", "unit_name")`
/// string-literal tuple; the type path is emitted as tokens into the generated code, and the
/// unit name is used directly as the serde rename suffix.
macro_rules! extract_custom_units {
    ($(($path:literal, $name:literal)),+) => {{
        let mut unit_impls: Vec<(TokenStream2, String)> = vec![];
        $(
            let field_units: TokenStream2 = $path.parse().expect("failed to parse custom unit path");
            unit_impls.push((field_units, $name.to_string()));
        )+
        unit_impls
    }};
}

/// Generates serde attributes for si fields
///
/// - field: struct field name as ident
/// - unit_name: plural name of units being used (generate using extract_units)
fn serde_attrs_for_si_field(field: &mut syn::Field, unit_name: &str, serialize_with: Option<&str>) {
    let ident = field.ident.clone().unwrap();
    match unit_name {
        // Empty unit name → canonical is the bare field name; no rename needed.
        "" => {}
        _ => {
            if !field_has_serde_rename(field) {
                // add the rename attribute for any fields that don't already have it
                let field_name_lit_str = format!("{ident}_{unit_name}");
                if let Some(sw_base) = serialize_with {
                    // The serialize_with path names the plain-field helper (e.g.
                    // "fastsim_core::utils::serde_helpers::power_as_kilowatts").
                    // Adjust it for wrapper types by inserting the appropriate prefix
                    // before the function name: vec_ / opt_ / tracked_.
                    let sw_path = adjust_serialize_with_for_wrapper(sw_base, &field.ty);
                    field.attrs.push(syn::parse_quote! {
                        #[serde(rename = #field_name_lit_str, serialize_with = #sw_path)]
                    });
                } else {
                    field.attrs.push(syn::parse_quote! {
                        #[serde(rename = #field_name_lit_str)]
                    });
                }
            }
        }
    }
}

/// Given a base serialize_with path (for plain `T`) and a field type, insert the
/// wrapper-appropriate prefix before the function name:
///   Vec<TrackedState<T>>  → vec_tracked_fn
///   Vec<TrackedState<Option<T>>> → vec_tracked_opt_fn
///   Vec<T>                → vec_fn
///   Option<TrackedState<T>> → opt_tracked_fn
///   Option<T>             → opt_fn
///   TrackedState<Option<T>> → tracked_opt_fn
///   TrackedState<T>       → tracked_fn
///   T                     → fn (unchanged)
fn adjust_serialize_with_for_wrapper(base_path: &str, field_ty: &syn::Type) -> String {
    let prefix = match detect_outer_wrapper(field_ty) {
        WrapperType::Vec => {
            // Check if the inner type is TrackedState (Vec<TrackedState<T>>)
            if let Some(inner) = extract_type_from_vec(field_ty) {
                if detect_outer_wrapper(inner) == WrapperType::TrackedState {
                    if let Some(tracked_inner) = extract_type_from_container(inner) {
                        if detect_outer_wrapper(tracked_inner) == WrapperType::Option {
                            "vec_tracked_opt_"
                        } else {
                            "vec_tracked_"
                        }
                    } else {
                        "vec_tracked_"
                    }
                } else {
                    "vec_"
                }
            } else {
                "vec_"
            }
        }
        WrapperType::Option => {
            if let Some(inner) = extract_type_from_container(field_ty) {
                if detect_outer_wrapper(inner) == WrapperType::TrackedState {
                    "opt_tracked_"
                } else {
                    "opt_"
                }
            } else {
                "opt_"
            }
        }
        WrapperType::TrackedState => {
            if let Some(inner) = extract_type_from_container(field_ty) {
                if detect_outer_wrapper(inner) == WrapperType::Option {
                    "tracked_opt_"
                } else {
                    "tracked_"
                }
            } else {
                "tracked_"
            }
        }
        WrapperType::None => return base_path.to_string(),
    };
    // Insert prefix before the last path segment: "a::b::fn_name" → "a::b::prefix_fn_name"
    if let Some(sep) = base_path.rfind("::") {
        format!("{}::{}{}", &base_path[..sep], prefix, &base_path[sep + 2..])
    } else {
        format!("{}{}", prefix, base_path)
    }
}

fn field_has_serde_rename(field: &syn::Field) -> bool {
    field.attrs.iter().any(|attr| {
        if let Meta::List(ml) = &attr.meta {
            // catch the `serde` in `#[serde(rename = "...")]`
            ml.path.is_ident("serde")
                &&
            // catch the `rename` in `#[serde(rename = "...")]`
            ml.tokens.to_string().contains("rename")
        } else {
            false
        }
    })
}

fn extract_type_path(ty: &syn::Type) -> Option<&syn::Path> {
    match ty {
        syn::Type::Path(type_path) if type_path.qself.is_none() => Some(&type_path.path),
        _ => None,
    }
}

fn extract_type_from_container(ty: &syn::Type) -> Option<&syn::Type> {
    fn extract_container_arg(path: &Path) -> Option<&GenericArgument> {
        let mut ident_path = String::new();
        for segment in &path.segments {
            ident_path.push_str(&segment.ident.to_string());

            // Exit when the inner brackets are found
            match &segment.arguments {
                syn::PathArguments::AngleBracketed(params) => return params.args.first(),
                syn::PathArguments::None => {}
                _ => return None,
            }

            ident_path.push_str("::");
        }
        None
    }

    extract_type_path(ty)
        .and_then(extract_container_arg)
        .and_then(|generic_arg| match *generic_arg {
            GenericArgument::Type(ref ty) => Some(ty),
            _ => None,
        })
}

/// Adapted from https://stackoverflow.com/questions/55271857/how-can-i-get-the-t-from-an-optiont-when-using-syn
/// Extracts contained type from Vec -- i.e. Vec<T> -> T
fn extract_type_from_vec(ty: &syn::Type) -> Option<&syn::Type> {
    fn extract_vec_argument(path: &syn::Path) -> Option<&syn::GenericArgument> {
        let mut ident_path = String::new();
        for segment in &path.segments {
            ident_path.push_str(&segment.ident.to_string());

            // Exit when the inner brackets are found
            match &segment.arguments {
                syn::PathArguments::AngleBracketed(params) => {
                    return match ident_path.as_str() {
                        "Vec" | "std::vec::Vec" => params.args.first(),
                        _ => None,
                    };
                }
                syn::PathArguments::None => {}
                _ => return None,
            }

            ident_path.push_str("::");
        }
        None
    }

    extract_type_path(ty)
        .and_then(extract_vec_argument)
        .and_then(|generic_arg| match generic_arg {
            syn::GenericArgument::Type(ty) => Some(ty),
            _ => None,
        })
}

// Extract the quantity name from an absolue uom path or an si path
fn extract_si_quantity(path: &syn::Path) -> Option<String> {
    if path.segments.len() <= 1 {
        return None;
    }
    let mut i = 0;
    if path.segments[i].ident == "uom" {
        i += 1;
        if path.segments.len() <= i + 1 {
            return None;
        }
    }
    if path.segments[i].ident != "si" {
        return None;
    }
    if path.segments[i + 1].ident == "f64" {
        i += 1;
        if path.segments.len() <= i + 1 {
            return None;
        }
    }

    Some(path.segments[i + 1].ident.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SIUnitOverride {
    Named(String),
    Unitless,
}

/// Extracts the si-unit override from `#[si_unit(...)]` and removes the attribute.
///
/// Supported forms:
/// - `#[si_unit(kilowatts)]` / `#[si_unit("kilowatts")]`
/// - `#[si_unit(unitless)]` / `#[si_unit("unitless")]`
fn extract_and_strip_si_unit(field: &mut syn::Field) -> Option<SIUnitOverride> {
    let mut unit_override = None;
    field.attrs.retain(|attr| {
        if !attr.path().is_ident("si_unit") {
            return true; // keep
        }
        if let syn::Meta::List(list) = &attr.meta {
            if list.tokens.is_empty() {
                abort!(
                    attr.span(),
                    "#[si_unit()]: empty form is not allowed; use #[si_unit(unitless)]"
                );
            } else if let Ok(ident) = syn::parse2::<syn::Ident>(list.tokens.clone()) {
                if ident == "unitless" {
                    unit_override = Some(SIUnitOverride::Unitless);
                } else {
                    unit_override = Some(SIUnitOverride::Named(ident.to_string()));
                }
            } else if let Ok(lit) = syn::parse2::<syn::LitStr>(list.tokens.clone()) {
                if lit.value() == "unitless" {
                    unit_override = Some(SIUnitOverride::Unitless);
                } else {
                    unit_override = Some(SIUnitOverride::Named(lit.value()));
                }
            }
        }
        false // strip
    });
    unit_override
}

fn has_unitless_si_unit_override(field: &syn::Field) -> bool {
    field.attrs.iter().any(|attr| {
        if !attr.path().is_ident("si_unit") {
            return false;
        }
        if let syn::Meta::List(list) = &attr.meta {
            if list.tokens.is_empty() {
                abort!(
                    attr.span(),
                    "#[si_unit()]: empty form is not allowed; use #[si_unit(unitless)]"
                );
            }
            if let Ok(ident) = syn::parse2::<syn::Ident>(list.tokens.clone()) {
                return ident == "unitless";
            }
            if let Ok(lit) = syn::parse2::<syn::LitStr>(list.tokens.clone()) {
                return lit.value() == "unitless";
            }
        }
        false
    })
}

/// Converts a CamelCase identifier to snake_case.
/// Used to derive the `serialize_with` helper path from a quantity name:
/// `"Power"` → `"power"`, `"ThermodynamicTemperature"` → `"thermodynamic_temperature"`.
fn to_snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

pub(crate) fn serde_attrs_for_si_fields(field: &mut syn::Field) -> Option<()> {
    let ftype = field.ty.clone();
    let mut vec_layers: u8 = 0;
    let mut inner_type = &ftype;

    while let Some(opt_inner_type) = extract_type_from_container(inner_type) {
        inner_type = opt_inner_type;
    }

    // pull out `inner_type` from `Vec<inner_type>`, recursively if there is any nesting
    while let Some(vec_inner_type) = extract_type_from_vec(inner_type) {
        inner_type = vec_inner_type;
        vec_layers += 1;
        if vec_layers >= 4 {
            abort!(ftype.span(), "Too many nested vec layers!");
        }
    }

    let inner_path = extract_type_path(inner_type)?;

    // Strip `#[si_unit(...)]` now so it is never emitted in the output struct.
    // If the field is not an SI type we abort below.
    let unit_override = extract_and_strip_si_unit(field);

    if let Some(quantity) = extract_si_quantity(inner_path) {
        // quantity_config is the single source of truth for all SI quantities.
        // Only the first (canonical) unit is applied here for the serialization rename;
        // all accepted deserialization units are handled by get_units_for_quantity.
        let Some((unit_impls, global_serialize_with)) = quantity_config(quantity.as_str()) else {
            abort!(
                inner_path.span(),
                "Unknown si quantity! Make sure it's implemented in `quantity_config`"
            );
        };

        // Determine the canonical (serialization) unit and serialize_with path.
        //
        // With no field-level override: use the first entry from quantity_config,
        // except Ratio which preserves a legacy bare-name canonical serialization key.
        // With `#[si_unit(name)]`: use the specified unit, constructing a serialize_with
        // path of the form `fastsim_core::utils::serde_helpers::{quantity_snake}_as_{unit}`
        // unless the unit is the SI base unit (first entry, no global serialize_with),
        // in which case the raw uom float serializes correctly without a converter.
        //
        // With `#[si_unit(unitless)]`: serialize bare using
        // the raw SI-base scalar (no suffix, no serialize_with conversion).
        let (canonical_unit, sw_owned): (String, Option<String>) =
            if let Some(ref unit_override) = unit_override {
                match unit_override {
                    SIUnitOverride::Unitless => (String::new(), None),
                    SIUnitOverride::Named(override_name) => {
                        let valid = unit_impls.iter().any(|(_, n)| n == override_name);
                        if !valid {
                            let choices: Vec<&str> =
                                unit_impls.iter().map(|(_, n)| n.as_str()).collect();
                            abort!(
                                field.span(),
                                "#[si_unit]: unknown unit '{}' for quantity '{}'. \
                                 Valid units: {}",
                                override_name,
                                quantity,
                                choices.join(", ")
                            );
                        }

                        // The raw uom float is always in the SI base unit.  Serializing it without
                        // a conversion helper is correct if and only if the target unit IS the base unit.
                        let is_base = base_unit_for_quantity(&quantity)
                            .map(|base| base == override_name.as_str())
                            .unwrap_or(false);

                        let serialize_with = if is_base {
                            None
                        } else {
                            Some(format!(
                                "fastsim_core::utils::serde_helpers::{}_as_{}",
                                to_snake_case(&quantity),
                                override_name
                            ))
                        };
                        (override_name.clone(), serialize_with)
                    }
                }
            } else if quantity == "Ratio" {
                // Legacy escape hatch: keep bare field names canonical for Ratio fields
                // unless an explicit override is provided.
                (String::new(), None)
            } else {
                let canonical = unit_impls
                    .first()
                    .map(|(_, n)| n.clone())
                    .unwrap_or_default();
                (canonical, global_serialize_with.map(String::from))
            };

        serde_attrs_for_si_field(field, &canonical_unit, sw_owned.as_deref());
    } else if unit_override.is_some() {
        abort!(
            field.span(),
            "#[si_unit] can only be applied to fields whose type is an SI quantity"
        );
    }
    Some(())
}

#[derive(Clone, Debug)]
pub struct SIFieldData {
    pub field_ident: syn::Ident,
    pub quantity: String,
    pub units: Vec<(TokenStream2, String)>, // (unit_type, plural_name)
    pub unitless_canonical: bool,
}

/// Collect SI field information for helper struct generation
pub fn collect_si_field_data(field: &syn::Field) -> Option<SIFieldData> {
    let field_ident = field.ident.as_ref()?.clone();

    // Fields with #[serde(skip)] are excluded from the SI helper path.
    if has_serde_skip(field) {
        return None;
    }

    let mut inner_type = &field.ty;

    // Unwrap known wrappers recursively (Vec, Option, TrackedState) so nested
    // combinations like Vec<TrackedState<Option<si::Ratio>>> are recognized.
    loop {
        match detect_outer_wrapper(inner_type) {
            WrapperType::Vec => {
                if let Some(vec_inner) = extract_type_from_vec(inner_type) {
                    inner_type = vec_inner;
                } else {
                    break;
                }
            }
            WrapperType::Option | WrapperType::TrackedState => {
                if let Some(container_inner) = extract_type_from_container(inner_type) {
                    inner_type = container_inner;
                } else {
                    break;
                }
            }
            WrapperType::None => break,
        }
    }

    let inner_path = extract_type_path(inner_type)?;
    let quantity = extract_si_quantity(inner_path)?;

    // Get units for this quantity
    let units = get_units_for_quantity(&quantity);
    let unitless_canonical = has_unitless_si_unit_override(field);

    Some(SIFieldData {
        field_ident,
        quantity,
        units,
        unitless_canonical,
    })
}

/// NOTE: this is where each available unit is defined for each quantity.
/// Both the serialization rename (first entry = canonical) and the accepted
/// deserialization alternates are derived from this single function.
///
/// To support a new unit in deserialization, add it to the appropriate quantity arm.
/// To change the serialized unit for a quantity, move its entry to first position
/// and set serialize_with to a matching helper path.
fn quantity_config(quantity: &str) -> Option<(Vec<(TokenStream2, String)>, Option<&'static str>)> {
    match quantity {
        "Acceleration" => Some((
            extract_units!(
                uom::si::acceleration::meter_per_second_squared,
                uom::si::acceleration::foot_per_second_squared,
                uom::si::acceleration::standard_gravity
            ),
            None,
        )),
        "Angle" => Some((extract_units!(uom::si::angle::radian), None)),
        "Area" => Some((extract_units!(uom::si::area::square_meter), None)),
        "Curvature" => Some((extract_units!(uom::si::curvature::radian_per_meter), None)),
        "DynamicViscosity" => Some((
            extract_units!(uom::si::dynamic_viscosity::pascal_second),
            None,
        )),
        "Energy" => Some((
            extract_units!(uom::si::energy::joule, uom::si::energy::kilowatt_hour),
            None,
        )),
        // "EnergyDensity" => Some((
        //     vec![(
        //         quote! {EnergyDensity},
        //         String::from("joule_per_cubic_meter"),
        //     )],
        //     None,
        // )),
        "Force" => Some((
            extract_units!(uom::si::force::newton, uom::si::force::pound_force),
            None,
        )),
        "HeatCapacity" => Some((
            extract_units!(uom::si::heat_capacity::joule_per_kelvin),
            None,
        )),
        "HeatTransferCoeff" => Some((
            extract_units!(uom::si::heat_transfer::watt_per_square_meter_kelvin),
            None,
        )),
        "InverseVelocity" => Some((
            extract_units!(uom::si::inverse_velocity::second_per_meter),
            None,
        )),
        "Length" => Some((
            extract_units!(
                uom::si::length::meter,
                uom::si::length::kilometer,
                uom::si::length::mile,
                uom::si::length::foot
            ),
            None,
        )),
        "Mass" => Some((
            extract_units!(uom::si::mass::kilogram, uom::si::mass::pound),
            None,
        )),
        "MassDensity" => Some((
            extract_units!(uom::si::mass_density::kilogram_per_cubic_meter),
            None,
        )),
        "MomentOfInertia" => Some((
            extract_units!(uom::si::moment_of_inertia::kilogram_square_meter),
            None,
        )),
        // First entry = canonical serialization unit.
        // To serialize Power in kilowatts, move kilowatt to first and set serialize_with:
        // "Power" => Some((
        //     extract_units!(
        //         uom::si::power::kilowatt,
        //         uom::si::power::watt,
        //         uom::si::power::horsepower
        //     ),
        //     Some("fastsim_core::utils::serde_helpers::power_as_kilowatts"),
        // )),
        "Power" => Some((
            extract_units!(
                uom::si::power::watt,
                uom::si::power::kilowatt,
                uom::si::power::horsepower
            ),
            None,
        )),
        "PowerRate" => Some((extract_units!(uom::si::power_rate::watt_per_second), None)),
        "Pressure" => Some((
            extract_units!(
                uom::si::pressure::pascal,
                uom::si::pressure::kilopascal,
                uom::si::pressure::bar,
                uom::si::pressure::pound_force_per_square_inch
            ),
            None,
        )),
        "Ratio" => Some((
            extract_units!(uom::si::ratio::ratio, uom::si::ratio::percent),
            None,
        )),
        "SpecificEnergy" => {
            // fastsim_core::si defines watt_hour_per_kilogram and kilowatt_hour_per_kilogram
            // as custom uom units (not present upstream). extract_custom_units! takes
            // ("path", "name") string literals because the proc-macro binary cannot link
            // against downstream crates to call Unit::plural().
            let mut units = extract_units!(
                uom::si::available_energy::joule_per_kilogram,
                uom::si::available_energy::kilojoule_per_kilogram
            );
            units.extend(extract_custom_units!(
                (
                    "fastsim_core::si::watt_hour_per_kilogram",
                    "watt_hours_per_kilogram"
                ),
                (
                    "fastsim_core::si::kilowatt_hour_per_kilogram",
                    "kilowatt_hours_per_kilogram"
                )
            ));
            Some((units, None))
        }
        "SpecificPower" => Some((
            extract_units!(
                uom::si::specific_power::watt_per_kilogram,
                uom::si::specific_power::kilowatt_per_kilogram
            ),
            None,
        )),
        "Temperature" => Some((
            extract_units!(
                uom::si::thermodynamic_temperature::kelvin,
                uom::si::thermodynamic_temperature::degree_celsius,
                uom::si::thermodynamic_temperature::degree_fahrenheit
            ),
            None,
        )),
        "TemperatureInterval" => Some((
            extract_units!(
                uom::si::temperature_interval::kelvin,
                uom::si::temperature_interval::degree_celsius,
                uom::si::temperature_interval::degree_fahrenheit
            ),
            None,
        )),
        "ThermalConductance" => Some((
            extract_units!(uom::si::thermal_conductance::watt_per_kelvin),
            None,
        )),
        "ThermalConductivity" => Some((
            extract_units!(uom::si::thermal_conductivity::watt_per_meter_kelvin),
            None,
        )),
        "Time" => Some((
            extract_units!(
                uom::si::time::second,
                uom::si::time::minute,
                uom::si::time::hour
            ),
            None,
        )),
        "Velocity" => Some((
            extract_units!(
                uom::si::velocity::meter_per_second,
                uom::si::velocity::kilometer_per_hour,
                uom::si::velocity::mile_per_hour
            ),
            None,
        )),
        "Volume" => Some((
            extract_units!(
                uom::si::volume::cubic_meter,
                uom::si::volume::liter,
                uom::si::volume::gallon
            ),
            None,
        )),
        _ => None,
    }
}

/// Returns the accepted deserialization units for `quantity`, primary first.
/// All quantities in quantity_config get multi-unit helper support.
/// To add a new quantity, add it to quantity_config and add the corresponding
/// `uom::si::f64::Foo` arm to `quantity_type` in generate_try_from_impl.
fn get_units_for_quantity(quantity: &str) -> Vec<(TokenStream2, String)> {
    quantity_config(quantity)
        .map(|(units, _)| units)
        .unwrap_or_default()
}

/// Returns the unit name that corresponds to the raw internal SI float for each quantity.
/// uom always stores values in the SI base unit regardless of how the field was constructed,
/// so serializing WITHOUT a `serialize_with` function gives the value in THIS unit.
/// Any other unit requires a `serialize_with` conversion helper.
fn base_unit_for_quantity(quantity: &str) -> Option<&'static str> {
    match quantity {
        "Acceleration" => Some("meters_per_second_squared"),
        "Angle" => Some("radians"),
        "Area" => Some("square_meters"),
        "Curvature" => Some("radians_per_meter"),
        "DynamicViscosity" => Some("pascal_seconds"),
        "Energy" => Some("joules"),
        // "EnergyDensity" => Some("joule_per_cubic_meter"), // commented out — no active uom type
        "Force" => Some("newtons"),
        "HeatCapacity" => Some("joules_per_kelvin"),
        "HeatTransferCoeff" => Some("watts_per_square_meter_kelvin"),
        "InverseVelocity" => Some("seconds_per_meter"),
        "Length" => Some("meters"),
        "Mass" => Some("kilograms"),
        "MassDensity" => Some("kilograms_per_cubic_meter"),
        "MomentOfInertia" => Some("kilogram_square_meters"),
        "Power" => Some("watts"),
        "PowerRate" => Some("watts_per_second"),
        "Pressure" => Some("pascals"),
        "Ratio" => Some("ratio"),
        "SpecificEnergy" => Some("joules_per_kilogram"),
        "SpecificPower" => Some("watts_per_kilogram"),
        "Temperature" => Some("kelvin"),
        "TemperatureInterval" => Some("kelvin"),
        "ThermalConductance" => Some("watts_per_kelvin"),
        "ThermalConductivity" => Some("watts_per_meter_kelvin"),
        "Time" => Some("seconds"),
        "Velocity" => Some("meters_per_second"),
        "Volume" => Some("cubic_meters"),
        _ => None,
    }
}

pub fn generate_helper_struct(
    helper_name: &syn::Ident,
    struct_ast: &syn::ItemStruct,
    si_fields: &[SIFieldData],
) -> TokenStream2 {
    // Build a map of SI field idents for quick lookup
    let si_field_idents: std::collections::HashSet<_> = si_fields
        .iter()
        .map(|f| f.field_ident.to_string())
        .collect();

    let mut helper_fields = vec![];

    if let syn::Fields::Named(syn::FieldsNamed { named, .. }) = &struct_ast.fields {
        for field in named {
            let field_ident = field.ident.as_ref().unwrap();
            let field_ident_str = field_ident.to_string();

            if si_field_idents.contains(&field_ident_str) {
                // This is an SI field - generate helper fields for each unit variant
                if let Some(si_field) = si_fields
                    .iter()
                    .find(|f| f.field_ident.to_string() == field_ident_str)
                {
                    let bare_name = field_ident.to_string();
                    // Collect any extra name aliases (alternative prefixes) from the field.
                    // Each alias is treated as an alternative field-name prefix: the macro
                    // expands it into unit-suffixed variants for every unit, mirroring how the
                    // canonical name is expanded.  The bare alias also routes to the base unit.
                    //
                    // Example: `budget_power: si::Power` with `#[serde(alias = "budget")]`
                    //   base unit helper (watts) → rename "budget_power_watts",  alias "budget_power",
                    //                              alias "budget" (bare), alias "budget_watts"
                    //   other helpers             → rename "budget_power_kilowatts", alias "budget_kilowatts"
                    let extra_aliases = extract_serde_aliases(field);

                    // The bare field name (no unit suffix) is a stable alias for the SI base
                    // unit, regardless of what the current canonical serialization unit is.
                    // This preserves backward compatibility: if the default changes from e.g.
                    // `ratio` to `percent`, files with bare `grade: 0.05` still mean 0.05 ratio.
                    let base_unit_idx = base_unit_for_quantity(&si_field.quantity)
                        .and_then(|base| {
                            si_field.units.iter().position(|(_, n)| n.as_str() == base)
                        })
                        .unwrap_or(0);

                    for (idx, (_unit_type, unit_name)) in si_field.units.iter().enumerate() {
                        let helper_field_name = syn::Ident::new(
                            &format!("__{}_{}", field_ident, unit_name),
                            field_ident.span(),
                        );

                        let field_ty = &field.ty;
                        let wrapper_type = detect_outer_wrapper(field_ty);

                        let field_type = match wrapper_type {
                            WrapperType::Vec => quote! { Option<Vec<f64>> },
                            _ => quote! { Option<f64> },
                        };

                        // The serde key is "field_unit"
                        let serde_key = format!("{}_{}", field_ident, unit_name);

                        // Field-unit is canonical by default, with bare-field aliasing for the
                        // SI base unit. A field-level `#[si_unit(unitless)]` override switches
                        // canonical naming to bare field for the base unit.
                        // Legacy escape hatch: unannotated Ratio fields also serialize bare.
                        let ratio_escape_hatch = si_field.quantity == "Ratio";
                        let use_bare_as_canonical = (si_field.unitless_canonical
                            || ratio_escape_hatch)
                            && idx == base_unit_idx;
                        let (canonical_name, unit_alias) = if use_bare_as_canonical {
                            (bare_name.clone(), serde_key.clone())
                        } else {
                            (serde_key.clone(), bare_name.clone())
                        };

                        // The SI base unit also accepts the bare field name as alias.
                        // Extra aliases: each alias A contributes alias "A_{unit}" to every
                        // unit's helper, plus bare "A" on the base unit only.
                        let is_base_unit = idx == base_unit_idx;
                        let expanded_aliases: Vec<String> = if is_base_unit {
                            extra_aliases
                                .iter()
                                .flat_map(|a| [a.clone(), format!("{}_{}", a, unit_name)])
                                .collect()
                        } else {
                            extra_aliases
                                .iter()
                                .map(|a| format!("{}_{}", a, unit_name))
                                .collect()
                        };

                        let serde_attr = if is_base_unit {
                            let extra = std::iter::once(unit_alias.as_str())
                                .chain(expanded_aliases.iter().map(String::as_str))
                                .map(|a| quote! { , alias = #a });
                            quote! { #[serde(default, rename = #canonical_name #(#extra)*)] }
                        } else if expanded_aliases.is_empty() {
                            quote! { #[serde(default, rename = #serde_key)] }
                        } else {
                            let extra = expanded_aliases.iter().map(|a| quote! { , alias = #a });
                            quote! { #[serde(default, rename = #serde_key #(#extra)*)] }
                        };

                        helper_fields.push(quote! {
                            #serde_attr
                            pub #helper_field_name: #field_type
                        });
                    }
                }
            } else {
                // Non-SI field
                let field_ty = &field.ty;
                let has_skip = has_serde_skip(field);
                let contains_self = type_contains_self(field_ty);
                let has_default = has_serde_default(field);

                if has_skip {
                    // Skip: omit from helper struct entirely; From impl will use Default::default()
                } else if contains_self && !has_default {
                    // Self-referential container without skip/default: emit a compile error
                    let field_name = field_ident.to_string();
                    helper_fields.push(quote! {
                        compile_error!(concat!(
                            "Field `", #field_name,
                            "` contains `Self` and is used with #[serde_api]. ",
                            "Add #[serde(skip)] or #[serde(default)] to this field."
                        ));
                    });
                } else {
                    let struct_name = &struct_ast.ident;
                    let final_ty = replace_self_in_type(field_ty, struct_name);
                    // Preserve only serde attributes (not custom derive helpers like #[has_state])
                    let serde_attrs: Vec<_> = field
                        .attrs
                        .iter()
                        .filter(|a| a.path().is_ident("serde"))
                        .collect();
                    helper_fields.push(quote! {
                        #(#serde_attrs)*
                        pub #field_ident: #final_ty
                    });
                }
            }
        }
    }

    // Propagate struct-level serde attributes to the helper.
    let deny_unknown = if has_serde_deny_unknown_fields(struct_ast) {
        quote! { #[serde(deny_unknown_fields)] }
    } else {
        quote! {}
    };
    let (struct_default_attr, default_derive) = if has_serde_struct_default(struct_ast) {
        (quote! { #[serde(default)] }, quote! { Default, })
    } else {
        (quote! {}, quote! {})
    };

    quote! {
        #[derive(::serde::Deserialize, #default_derive)]
        #[serde(crate = "::serde")]
        #deny_unknown
        #struct_default_attr
        struct #helper_name {
            #(#helper_fields),*
        }
    }
}

/// Generate `impl TryFrom<Helper> for Struct` that converts the helper's optional
/// unit-variant fields into the original struct's SI field values.
///
/// Returns `Err(String)` (surfaced by serde as a proper deserialization error with
/// field context) when a required SI field has no populated unit variant in the input.
pub fn generate_try_from_impl(
    original_name: &syn::Ident,
    helper_name: &syn::Ident,
    struct_ast: &syn::ItemStruct,
    si_fields: &[SIFieldData],
) -> TokenStream2 {
    // Build a map of SI field idents for quick lookup
    let si_field_idents: std::collections::HashSet<_> = si_fields
        .iter()
        .map(|f| f.field_ident.to_string())
        .collect();

    let mut conflict_checks = vec![];
    let mut field_conversions = vec![];

    if let syn::Fields::Named(syn::FieldsNamed { named, .. }) = &struct_ast.fields {
        for field in named {
            let field_ident = field.ident.as_ref().unwrap();
            let field_ident_str = field_ident.to_string();

            if si_field_idents.contains(&field_ident_str) {
                // This is an SI field - convert from helper unit variants
                if let Some(si_field) = si_fields
                    .iter()
                    .find(|f| f.field_ident.to_string() == field_ident_str)
                {
                    let field_default_expr = serde_default_expr(field);
                    let quantity_str = &si_field.quantity;
                    let quantity_type = match quantity_str.as_str() {
                        "Acceleration" => quote! { uom::si::f64::Acceleration },
                        "Angle" => quote! { uom::si::f64::Angle },
                        "Area" => quote! { uom::si::f64::Area },
                        "Curvature" => quote! { uom::si::f64::Curvature },
                        "DynamicViscosity" => quote! { uom::si::f64::DynamicViscosity },
                        "Energy" => quote! { uom::si::f64::Energy },
                        // "EnergyDensity" => quote! { uom::si::f64::Pressure }, // commented out — no active uom type
                        "Force" => quote! { uom::si::f64::Force },
                        "HeatCapacity" => quote! { uom::si::f64::HeatCapacity },
                        "HeatTransferCoeff" => quote! { uom::si::f64::HeatTransfer },
                        "InverseVelocity" => quote! { uom::si::f64::InverseVelocity },
                        "Length" => quote! { uom::si::f64::Length },
                        "Mass" => quote! { uom::si::f64::Mass },
                        "MassDensity" => quote! { uom::si::f64::MassDensity },
                        "MomentOfInertia" => quote! { uom::si::f64::MomentOfInertia },
                        "Power" => quote! { uom::si::f64::Power },
                        "PowerRate" => quote! { uom::si::f64::PowerRate },
                        "Pressure" => quote! { uom::si::f64::Pressure },
                        "Ratio" => quote! { uom::si::f64::Ratio },
                        "SpecificEnergy" => quote! { uom::si::f64::AvailableEnergy },
                        "SpecificPower" => quote! { uom::si::f64::SpecificPower },
                        "Temperature" => quote! { uom::si::f64::ThermodynamicTemperature },
                        "TemperatureInterval" => quote! { uom::si::f64::TemperatureInterval },
                        "ThermalConductance" => quote! { uom::si::f64::ThermalConductance },
                        "ThermalConductivity" => quote! { uom::si::f64::ThermalConductivity },
                        "Time" => quote! { uom::si::f64::Time },
                        "Velocity" => quote! { uom::si::f64::Velocity },
                        "Volume" => quote! { uom::si::f64::Volume },
                        _ => continue,
                    };

                    // Check if field is wrapped in TrackedState, Option, or Vec
                    let field_ty = &field.ty;
                    let wrapper_type = detect_outer_wrapper(field_ty);

                    let mut match_arms = vec![];

                    if wrapper_type == WrapperType::Vec {
                        // For Vec fields, convert each element in the vector
                        for (unit_type, unit_name) in &si_field.units {
                            let helper_field_name = syn::Ident::new(
                                &format!("__{}_{}", field_ident, unit_name),
                                field_ident.span(),
                            );

                            match_arms.push(quote! {
                                helper.#helper_field_name.map(|vals| vals.into_iter().map(|v| #quantity_type::new::<#unit_type>(v)).collect::<Vec<_>>())
                            });
                        }
                    } else {
                        // For non-Vec fields, convert the single value
                        for (unit_type, unit_name) in &si_field.units {
                            let helper_field_name = syn::Ident::new(
                                &format!("__{}_{}", field_ident, unit_name),
                                field_ident.span(),
                            );

                            match_arms.push(quote! {
                                helper.#helper_field_name.map(|val| #quantity_type::new::<#unit_type>(val))
                            });
                        }
                    }

                    if match_arms.is_empty() {
                        continue;
                    }

                    // Reject inputs that supply more than one unit variant for the same field.
                    // The .or_else() chain would otherwise silently use whichever came first.
                    let conflict_check = if si_field.units.len() > 1 {
                        let field_name_str = field_ident.to_string();
                        let unit_names: Vec<String> = si_field
                            .units
                            .iter()
                            .map(|(_, u)| format!("{}_{}", field_ident, u))
                            .collect();
                        let unit_names_display = unit_names.join(", ");
                        let populated_flags: Vec<TokenStream2> = si_field
                            .units
                            .iter()
                            .map(|(_, unit_name)| {
                                let helper_field_name = syn::Ident::new(
                                    &format!("__{}_{}", field_ident, unit_name),
                                    field_ident.span(),
                                );
                                quote! { helper.#helper_field_name.is_some() }
                            })
                            .collect();
                        quote! {
                            {
                                let __count = [#(#populated_flags),*]
                                    .iter()
                                    .filter(|&&b| b)
                                    .count();
                                if __count > 1 {
                                    return ::std::result::Result::Err(::std::format!(
                                        "field `{}`: multiple unit variants supplied; \
                                         provide exactly one of: {}",
                                        #field_name_str, #unit_names_display
                                    ));
                                }
                            }
                        }
                    } else {
                        quote! {}
                    };

                    // Combine all match arms with .or_else()
                    let conversion = if match_arms.len() == 1 {
                        match_arms.into_iter().next().unwrap()
                    } else {
                        let mut result = match_arms[0].clone();
                        for arm in &match_arms[1..] {
                            result = quote! {
                                #result.or_else(|| #arm)
                            };
                        }
                        result
                    };

                    let wrapped_conversion = match wrapper_type {
                        WrapperType::TrackedState => {
                            let ts_path = outer_type_path_tokens(field_ty).unwrap_or_else(
                                || quote! { fastsim_core::utils::tracked_state::TrackedState },
                            );
                            let tracked_inner_is_option = extract_type_from_container(field_ty)
                                .map(|inner| detect_outer_wrapper(inner) == WrapperType::Option)
                                .unwrap_or(false);
                            if tracked_inner_is_option {
                                if let Some(default_expr) = &field_default_expr {
                                    quote! {
                                        (#conversion).map(|val| #ts_path::new(Some(val)))
                                            .unwrap_or_else(|| #default_expr)
                                    }
                                } else {
                                    quote! {
                                        (#conversion).map(|val| #ts_path::new(Some(val)))
                                            .unwrap_or_else(|| #ts_path::new(None))
                                    }
                                }
                            } else if let Some(default_expr) = &field_default_expr {
                                quote! {
                                    (#conversion).map(|val| #ts_path::new(val))
                                        .unwrap_or_else(|| #default_expr)
                                }
                            } else if has_serde_struct_default(struct_ast) {
                                quote! {
                                    (#conversion).map(|val| #ts_path::new(val))
                                        .unwrap_or_default()
                                }
                            } else {
                                quote! {
                                    (#conversion).map(|val| #ts_path::new(val))
                                        .ok_or_else(|| format!("Missing field {} in any unit variant", stringify!(#field_ident)))?
                                }
                            }
                        }
                        WrapperType::Option => {
                            if let Some(default_expr) = &field_default_expr {
                                quote! {
                                    (#conversion).or_else(|| #default_expr)
                                }
                            } else {
                                quote! {
                                    (#conversion)
                                }
                            }
                        }
                        WrapperType::Vec => {
                            let vec_inner_ty = extract_type_from_vec(field_ty);
                            let vec_inner_is_tracked = vec_inner_ty
                                .map(|ty| detect_outer_wrapper(ty) == WrapperType::TrackedState)
                                .unwrap_or(false);
                            if vec_inner_is_tracked {
                                let ts_path = vec_inner_ty
                                    .and_then(outer_type_path_tokens)
                                    .unwrap_or_else(|| {
                                        quote! { fastsim_core::utils::tracked_state::TrackedState }
                                    });
                                let tracked_inner_is_option = vec_inner_ty
                                    .and_then(extract_type_from_container)
                                    .map(|inner| detect_outer_wrapper(inner) == WrapperType::Option)
                                    .unwrap_or(false);
                                if tracked_inner_is_option {
                                    if let Some(default_expr) = &field_default_expr {
                                        quote! {
                                            (#conversion)
                                                .map(|vals| vals.into_iter().map(|v| #ts_path::new(Some(v))).collect())
                                                .unwrap_or_else(|| #default_expr)
                                        }
                                    } else {
                                        quote! {
                                            (#conversion)
                                                .map(|vals| vals.into_iter().map(|v| #ts_path::new(Some(v))).collect())
                                                .unwrap_or_default()
                                        }
                                    }
                                } else {
                                    if let Some(default_expr) = &field_default_expr {
                                        quote! {
                                            (#conversion)
                                                .map(|vals| vals.into_iter().map(|v| #ts_path::new(v)).collect())
                                                .unwrap_or_else(|| #default_expr)
                                        }
                                    } else {
                                        quote! {
                                            (#conversion)
                                                .map(|vals| vals.into_iter().map(|v| #ts_path::new(v)).collect())
                                                .unwrap_or_default()
                                        }
                                    }
                                }
                            } else if let Some(default_expr) = &field_default_expr {
                                quote! {
                                    (#conversion).unwrap_or_else(|| #default_expr)
                                }
                            } else {
                                quote! {
                                    (#conversion).unwrap_or_default()
                                }
                            }
                        }
                        WrapperType::None => {
                            if let Some(default_expr) = &field_default_expr {
                                quote! {
                                    (#conversion).unwrap_or_else(|| #default_expr)
                                }
                            } else if has_serde_struct_default(struct_ast) {
                                quote! {
                                    (#conversion).unwrap_or_default()
                                }
                            } else {
                                quote! {
                                    (#conversion).ok_or_else(|| format!("Missing field {} in any unit variant", stringify!(#field_ident)))?
                                }
                            }
                        }
                    };

                    if !conflict_check.is_empty() {
                        conflict_checks.push(conflict_check);
                    }
                    field_conversions.push(quote! {
                        #field_ident: #wrapped_conversion
                    });
                }
            } else {
                // Non-SI field
                if has_serde_skip(field) {
                    field_conversions.push(quote! {
                        #field_ident: Default::default()
                    });
                } else {
                    field_conversions.push(quote! {
                        #field_ident: helper.#field_ident
                    });
                }
            }
        }
    }

    quote! {
        impl ::std::convert::TryFrom<#helper_name> for #original_name {
            type Error = ::std::string::String;

            fn try_from(helper: #helper_name) -> ::std::result::Result<Self, ::std::string::String> {
                #(#conflict_checks)*
                ::std::result::Result::Ok(Self {
                    #(#field_conversions),*
                })
            }
        }
    }
}

/// Returns true if the field has `#[serde(skip)]`
fn has_serde_skip(field: &syn::Field) -> bool {
    has_serde_flag(field, "skip")
}

/// Returns true if the field has `#[serde(default)]`
fn has_serde_default(field: &syn::Field) -> bool {
    has_serde_flag(field, "default")
}

/// Builds an expression to use when a field has `#[serde(default)]`.
///
/// - `#[serde(default)]` -> `::std::default::Default::default()`
/// - `#[serde(default = "path::to::func")]` -> `path::to::func()`
fn serde_default_expr(field: &syn::Field) -> Option<TokenStream2> {
    for attr in &field.attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }

        let mut default_kind: Option<Option<String>> = None;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("default") {
                if meta.input.peek(syn::Token![=]) {
                    let _: syn::Token![=] = meta.input.parse().unwrap();
                    let lit: syn::LitStr = meta.input.parse().unwrap();
                    default_kind = Some(Some(lit.value()));
                } else {
                    default_kind = Some(None);
                }
            } else if meta.input.peek(syn::Token![=]) {
                // consume unrelated `key = value` entries
                let _: syn::Token![=] = meta.input.parse().unwrap();
                let _: proc_macro2::TokenTree = meta.input.parse().unwrap();
            }
            Ok(())
        });

        if let Some(kind) = default_kind {
            return match kind {
                Some(path_str) => {
                    let path: syn::Path = syn::parse_str(&path_str).unwrap_or_else(|_| {
                        abort!(
                            field.span(),
                            "Invalid serde default path `{}`. Expected a path like `Type::func`",
                            path_str
                        )
                    });
                    Some(quote! { #path() })
                }
                None => Some(quote! { ::std::default::Default::default() }),
            };
        }
    }

    None
}

fn has_serde_flag(field: &syn::Field, flag: &str) -> bool {
    field.attrs.iter().any(|attr| {
        if !attr.path().is_ident("serde") {
            return false;
        }
        let mut found = false;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident(flag) {
                found = true;
            }
            // consume "= value" if present so parse_nested_meta doesn't error
            if meta.input.peek(syn::Token![=]) {
                let _: syn::Token![=] = meta.input.parse().unwrap();
                let _: proc_macro2::TokenTree = meta.input.parse().unwrap();
            }
            Ok(())
        });
        found
    })
}

/// Extract all `alias = "..."` string values from a field's `#[serde(...)]` attributes.
fn extract_serde_aliases(field: &syn::Field) -> Vec<String> {
    let mut aliases = vec![];
    for attr in &field.attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("alias") && meta.input.peek(syn::Token![=]) {
                let _: syn::Token![=] = meta.input.parse().unwrap();
                let lit: syn::LitStr = meta.input.parse().unwrap();
                aliases.push(lit.value());
            } else if meta.input.peek(syn::Token![=]) {
                // consume other "= value" tokens so parse_nested_meta doesn't error
                let _: syn::Token![=] = meta.input.parse().unwrap();
                let _: proc_macro2::TokenTree = meta.input.parse().unwrap();
            }
            Ok(())
        });
    }
    aliases
}

/// Returns true if the struct has `#[serde(deny_unknown_fields)]`.
fn has_serde_deny_unknown_fields(struct_ast: &syn::ItemStruct) -> bool {
    has_serde_struct_flag(struct_ast, "deny_unknown_fields")
}

/// Returns true if the struct has `#[serde(default)]` at the struct level.
fn has_serde_struct_default(struct_ast: &syn::ItemStruct) -> bool {
    has_serde_struct_flag(struct_ast, "default")
}

fn has_serde_struct_flag(struct_ast: &syn::ItemStruct, flag: &str) -> bool {
    struct_ast.attrs.iter().any(|attr| {
        if !attr.path().is_ident("serde") {
            return false;
        }
        let mut found = false;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident(flag) {
                found = true;
            }
            if meta.input.peek(syn::Token![=]) {
                let _: syn::Token![=] = meta.input.parse().unwrap();
                let _: proc_macro2::TokenTree = meta.input.parse().unwrap();
            }
            Ok(())
        });
        found
    })
}

/// Replace bare `Self` path segments with `replacement` in a type.
fn replace_self_in_type(ty: &syn::Type, replacement: &syn::Ident) -> syn::Type {
    match ty {
        syn::Type::Path(type_path) => {
            let mut new_path = type_path.clone();
            for segment in &mut new_path.path.segments {
                if segment.ident == "Self" {
                    segment.ident = replacement.clone();
                }
                if let syn::PathArguments::AngleBracketed(ref mut args) = segment.arguments {
                    for arg in &mut args.args {
                        if let syn::GenericArgument::Type(inner_ty) = arg {
                            *inner_ty = replace_self_in_type(inner_ty, replacement);
                        }
                    }
                }
            }
            syn::Type::Path(new_path)
        }
        _ => ty.clone(),
    }
}

/// Returns true if the type tree contains a bare `Self` path segment.
fn type_contains_self(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Path(type_path) => type_path.path.segments.iter().any(|seg| {
            seg.ident == "Self"
                || if let syn::PathArguments::AngleBracketed(ref args) = seg.arguments {
                    args.args.iter().any(|arg| {
                        if let syn::GenericArgument::Type(inner) = arg {
                            type_contains_self(inner)
                        } else {
                            false
                        }
                    })
                } else {
                    false
                }
        }),
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WrapperType {
    TrackedState,
    Option,
    Vec,
    None,
}

fn detect_outer_wrapper(ty: &syn::Type) -> WrapperType {
    if let Some(path) = extract_type_path(ty) {
        if let Some(segment) = path.segments.last() {
            if segment.ident == "TrackedState" {
                return WrapperType::TrackedState;
            } else if segment.ident == "Option" {
                return WrapperType::Option;
            } else if segment.ident == "Vec" {
                return WrapperType::Vec;
            }
        }
    }
    WrapperType::None
}

/// Extract the outer type path without generic args as a token stream.
/// e.g. `fastsim_core::utils::TrackedState<si::Power>` → `fastsim_core::utils::TrackedState`
/// Used to call `::new()` on the outer wrapper type with the correct path.
fn outer_type_path_tokens(ty: &syn::Type) -> Option<TokenStream2> {
    if let syn::Type::Path(type_path) = ty {
        let mut path = type_path.path.clone();
        // Strip generic args from the last segment
        if let Some(last) = path.segments.last_mut() {
            last.arguments = syn::PathArguments::None;
        }
        Some(quote! { #path })
    } else {
        None
    }
}

/// Generate the return type tokens and body expression for a getter on an SI field,
/// handling all wrapper combinations (plain T, Option<T>, TrackedState<T>,
/// TrackedState<Option<T>>, Vec<T>, Vec<TrackedState<T>>, Vec<TrackedState<Option<T>>>).
fn generate_getter_body(
    field_ident: &syn::Ident,
    field_ty: &syn::Type,
    unit_type: &TokenStream2,
) -> (TokenStream2, TokenStream2) {
    match detect_outer_wrapper(field_ty) {
        WrapperType::None => (
            quote! { f64 },
            quote! { self.#field_ident.get::<#unit_type>() },
        ),
        WrapperType::Option => (
            quote! { ::std::option::Option<f64> },
            quote! { self.#field_ident.map(|v| v.get::<#unit_type>()) },
        ),
        WrapperType::TrackedState => {
            let inner_is_option = extract_type_from_container(field_ty)
                .map(|inner| detect_outer_wrapper(inner) == WrapperType::Option)
                .unwrap_or(false);
            if inner_is_option {
                (
                    quote! { ::std::option::Option<f64> },
                    // inner() returns &Option<T>; dereference to copy the Option<T> (Copy when T: Copy)
                    quote! { (*self.#field_ident.inner()).map(|v| v.get::<#unit_type>()) },
                )
            } else {
                (
                    quote! { f64 },
                    // inner() returns &T; auto-deref for method call
                    quote! { self.#field_ident.inner().get::<#unit_type>() },
                )
            }
        }
        WrapperType::Vec => {
            let inner_ty = extract_type_from_vec(field_ty);
            let inner_is_tracked = inner_ty
                .map(|t| detect_outer_wrapper(t) == WrapperType::TrackedState)
                .unwrap_or(false);
            if inner_is_tracked {
                let tracked_inner_is_option = inner_ty
                    .and_then(extract_type_from_container)
                    .map(|t| detect_outer_wrapper(t) == WrapperType::Option)
                    .unwrap_or(false);
                if tracked_inner_is_option {
                    (
                        quote! { ::std::vec::Vec<::std::option::Option<f64>> },
                        quote! { self.#field_ident.iter().map(|ts| (*ts.inner()).map(|v| v.get::<#unit_type>())).collect() },
                    )
                } else {
                    (
                        quote! { ::std::vec::Vec<f64> },
                        quote! { self.#field_ident.iter().map(|ts| ts.inner().get::<#unit_type>()).collect() },
                    )
                }
            } else {
                (
                    quote! { ::std::vec::Vec<f64> },
                    quote! { self.#field_ident.iter().map(|v| v.get::<#unit_type>()).collect() },
                )
            }
        }
    }
}

/// Data for a field marked with `#[py_get]`.
#[derive(Clone, Debug)]
pub struct PyGetFieldData {
    pub field_ident: syn::Ident,
    pub field_ty: syn::Type,
}

/// Detect and strip `#[py_get]` from a field, returning its data if present.
/// Must be called before [`serde_attrs_for_si_fields`] so the attribute is
/// removed before the struct is emitted.
pub fn collect_and_strip_py_get(field: &mut syn::Field) -> Option<PyGetFieldData> {
    let field_ident = field.ident.as_ref()?.clone();
    let mut found = false;
    field.attrs.retain(|attr| {
        if attr.path().is_ident("py_get") {
            found = true;
            false // strip
        } else {
            true
        }
    });
    if found {
        Some(PyGetFieldData {
            field_ident,
            field_ty: field.ty.clone(),
        })
    } else {
        None
    }
}

/// Generate `(return_type, body_expr)` for a `#[py_get]` getter.
///
/// `TrackedState` wrappers are automatically unwrapped so the inner value is
/// returned; all other field types are returned as-is via `.clone()`.
fn generate_py_get_body(
    field_ident: &syn::Ident,
    field_ty: &syn::Type,
) -> (TokenStream2, TokenStream2) {
    match detect_outer_wrapper(field_ty) {
        WrapperType::TrackedState => {
            // TrackedState<T> or TrackedState<Option<T>> → unwrap with .inner()
            match extract_type_from_container(field_ty) {
                Some(inner_ty) => (
                    quote! { #inner_ty },
                    quote! { self.#field_ident.inner().clone() },
                ),
                None => (
                    quote! { #field_ty },
                    quote! { self.#field_ident.inner().clone() },
                ),
            }
        }
        WrapperType::Vec => {
            match extract_type_from_vec(field_ty).map(|elem| (elem, detect_outer_wrapper(elem))) {
                Some((elem_ty, WrapperType::TrackedState)) => {
                    // Vec<TrackedState<T>> → Vec<T>, Vec<TrackedState<Option<T>>> → Vec<Option<T>>
                    match extract_type_from_container(elem_ty) {
                        Some(inner_ty) => (
                            quote! { ::std::vec::Vec<#inner_ty> },
                            quote! { self.#field_ident.iter().map(|ts| ts.inner().clone()).collect() },
                        ),
                        None => (quote! { #field_ty }, quote! { self.#field_ident.clone() }),
                    }
                }
                _ => (quote! { #field_ty }, quote! { self.#field_ident.clone() }),
            }
        }
        // Plain T, Option<T>, Option<TrackedState<T>> — return as-is
        _ => (quote! { #field_ty }, quote! { self.#field_ident.clone() }),
    }
}

/// Returns true if the struct has a `#[pyclass]` or `#[cfg_attr(..., pyclass(...))]` attribute,
/// meaning it is a PyO3 class and can have `#[pymethods]` blocks.
fn struct_is_pyclass(struct_ast: &syn::ItemStruct) -> bool {
    struct_ast.attrs.iter().any(|attr| {
        // Direct #[pyclass] or #[pyclass(...)]
        if attr.path().is_ident("pyclass") {
            return true;
        }
        // #[cfg_attr(..., pyclass, ...)] or #[cfg_attr(..., pyclass(...), ...)]
        if attr.path().is_ident("cfg_attr") {
            if let syn::Meta::List(list) = &attr.meta {
                return list.tokens.to_string().contains("pyclass");
            }
        }
        false
    })
}

/// Generate a `#[cfg(feature = "pyo3")] #[pymethods] impl StructName { ... }` block
/// containing:
///   - one `#[getter("field_unit")]` method per (SI field × unit) combination, and
///   - one `#[getter("field")]` method per field annotated with `#[py_get]`.
///
/// SI getter return types depend on the wrapper:
/// `T` → `f64`, `Option<T>` → `Option<f64>`, `TrackedState<T>` → `f64`, etc.
///
/// `#[py_get]` getters return the field type directly (`.clone()`); `TrackedState`
/// wrappers are automatically unwrapped.
pub fn generate_py_getters(
    struct_name: &syn::Ident,
    struct_ast: &syn::ItemStruct,
    si_fields: &[SIFieldData],
    py_get_fields: &[PyGetFieldData],
) -> TokenStream2 {
    // Only emit #[pymethods] for structs that are #[pyclass] (or #[cfg_attr(..., pyclass)]).
    // Non-pyclass structs with #[serde_api] don't need Python property getters.
    if !struct_is_pyclass(struct_ast) {
        return quote! {};
    }

    let si_field_idents: std::collections::HashSet<_> = si_fields
        .iter()
        .map(|f| f.field_ident.to_string())
        .collect();

    let mut getter_methods: Vec<TokenStream2> = vec![];

    if let syn::Fields::Named(syn::FieldsNamed { named, .. }) = &struct_ast.fields {
        for field in named {
            let field_ident = field.ident.as_ref().unwrap();
            if !si_field_idents.contains(&field_ident.to_string()) {
                continue;
            }
            let si_field = match si_fields
                .iter()
                .find(|f| f.field_ident.to_string() == field_ident.to_string())
            {
                Some(f) => f,
                None => continue,
            };

            let field_ty = &field.ty;

            for (unit_type, unit_name) in &si_field.units {
                let property_name = format!("{}_{}", field_ident, unit_name);
                let fn_name = syn::Ident::new(
                    &format!("get_{}_{}_py", field_ident, unit_name),
                    field_ident.span(),
                );
                let (return_ty, body) = generate_getter_body(field_ident, field_ty, unit_type);
                getter_methods.push(quote! {
                    #[getter(#property_name)]
                    pub fn #fn_name(&self) -> #return_ty {
                        #body
                    }
                });
            }
        }
    }

    // Generate plain clone getters for #[py_get]-annotated fields.
    for py_field in py_get_fields {
        let field_ident = &py_field.field_ident;
        let field_ty = &py_field.field_ty;
        let property_name = field_ident.to_string();
        let fn_name = syn::Ident::new(&format!("get_{}_py", field_ident), field_ident.span());
        let (return_ty, body) = generate_py_get_body(field_ident, field_ty);
        getter_methods.push(quote! {
            #[getter(#property_name)]
            pub fn #fn_name(&self) -> #return_ty {
                #body
            }
        });
    }

    if getter_methods.is_empty() {
        return quote! {};
    }

    quote! {
        #[allow(non_snake_case)]
        #[::pyo3::pymethods]
        #[cfg(feature = "pyo3")]
        impl #struct_name {
            #(#getter_methods)*
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asserts the invariant: for every quantity whose `quantity_config` entry has no
    /// `serialize_with` function, the first unit listed must be the SI base unit.
    ///
    /// If this test fails for a quantity it means the serialized key will be mislabeled
    /// (the raw internal float is always in the SI base unit, so the label must match).
    /// Fix it by either:
    ///   a) placing the base unit first in `quantity_config` (preferred, no conversion needed), or
    ///   b) adding a `serialize_with` helper that converts from base to the desired unit.
    #[test]
    fn no_serialize_with_implies_first_unit_is_si_base() {
        let quantities = [
            "Acceleration",
            "Angle",
            "Area",
            "Curvature",
            "DynamicViscosity",
            "Energy",
            // "EnergyDensity", // commented out — no active uom type
            "Force",
            "HeatCapacity",
            "HeatTransferCoeff",
            "InverseVelocity",
            "Length",
            "Mass",
            "MassDensity",
            "MomentOfInertia",
            "Power",
            "PowerRate",
            "Pressure",
            "Ratio",
            "SpecificEnergy",
            "SpecificPower",
            "Temperature",
            "TemperatureInterval",
            "ThermalConductance",
            "ThermalConductivity",
            "Time",
            "Velocity",
            "Volume",
        ];

        for qty in &quantities {
            let (units, serialize_with) = quantity_config(qty)
                .unwrap_or_else(|| panic!("quantity_config returned None for '{qty}'"));

            if serialize_with.is_some() {
                // Explicit converter present — first entry need not be the base unit.
                continue;
            }

            let first = units.first().map(|(_, n)| n.as_str()).unwrap_or("");
            let base = base_unit_for_quantity(qty)
                .unwrap_or_else(|| panic!("base_unit_for_quantity returned None for '{qty}'"));

            assert_eq!(
                first, base,
                "Quantity '{qty}': first unit '{first}' ≠ SI base unit '{base}'. \
                 Either place '{base}' first in quantity_config or add a serialize_with helper."
            );
        }
    }
}
