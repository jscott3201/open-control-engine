//! The pinned inverses of modelica-json's unit and quantity tables (design note decision 5).
//!
//! `modelica-json` (`lib/cxfExtractor.js`, `getQudtUnit` and `getQudtQuanityKind`, at the pin the
//! vendored README records) writes a declared Modelica `unit` as a QUDT unit identifier through
//! a fixed table, and anything the table lacks as the original string in the S231 namespace. It
//! writes a declared `quantity` as a QUDT quantity kind of the same name, except that
//! `PressureDifference` becomes `Pressure`. The generator reads those identifiers back into the
//! Modelica strings the corpus and the engine use. Both tables are closed: an identifier that is
//! neither in a table nor the S231 fallback fails the generator.

/// `(QUDT unit local name, Modelica unit string)`, the inverse of `getQudtUnit`'s table, in its
/// order.
pub const QUDT_UNITS: &[(&str, &str)] = &[
    ("BTU_IT", "Btu"),
    ("BTU_IT-PER-HR", "BtuPerHour"),
    ("W", "W"),
    ("J", "J"),
    ("BAR", "bar"),
    ("PA", "Pa"),
    ("M3-PER-MIN", "cfm"),
    ("M3-PER-SEC", "m3/s"),
    ("DEG_C", "degC"),
    ("DEG_F", "degF"),
    ("K", "K"),
    ("GAL_US", "gal"),
    ("HP", "hp"),
    ("M2", "m2"),
    ("M3", "m3"),
    ("SEC", "s"),
    ("KiloJ-PER-KiloGM", "kJ/kg"),
    ("IN_H2O_60DEG_F", "inH2O"),
    ("PSI", "psi"),
    ("QUAD", "quad"),
    ("TON_FG", "ton"),
    ("DEG", "deg"),
    ("RAD", "rad"),
    ("UNITLESS", "1"),
    ("HR", "h"),
    ("J-PER-KiloGM", "J/kg"),
    ("HZ", "Hz"),
];

/// `(Modelica quantity, QUDT quantity-kind local name)`: `getQudtQuanityKind` as written, in its
/// order. It is not injective: two Modelica quantities become `Pressure`.
pub const QUANTITY_KINDS: &[(&str, &str)] = &[
    ("Angle", "Angle"),
    ("Area", "Area"),
    ("Energy", "Energy"),
    ("Frequency", "Frequency"),
    ("Illuminance", "Illuminance"),
    ("Irradiance", "Irradiance"),
    ("MassFlowRate", "MassFlowRate"),
    ("MassFraction", "MassFraction"),
    ("Power", "Power"),
    ("PowerFactor", "PowerFactor"),
    ("Pressure", "Pressure"),
    ("PressureDifference", "Pressure"),
    ("SpecificEnergy", "SpecificEnergy"),
    ("TemperatureDifference", "TemperatureDifference"),
    ("Time", "Time"),
    ("ThermodynamicTemperature", "ThermodynamicTemperature"),
    ("Velocity", "Velocity"),
    ("VolumeFlowRate", "VolumeFlowRate"),
];

/// The compact prefix of a QUDT unit in CXF-Core's context.
const UNIT_PREFIX: &str = "unit:";
/// The compact prefix of a QUDT quantity kind in CXF-Core's context.
const QUANTITY_PREFIX: &str = "q:";
/// The two spellings of the S231 namespace an unmapped unit is written in: the compact prefix,
/// and the expanded IRI the JSON-LD writer falls back to when the unit is not a valid local name
/// (`1/s`).
const S231_FALLBACKS: [&str; 2] = ["S231:", "http://data.ashrae.org/S231#"];

/// The Modelica unit string a `qudt:hasUnit` identifier stands for: the table's entry for a
/// `unit:` identifier, or the original string of an S231 fallback. `None` for anything else, an
/// empty fallback included.
pub fn modelica_unit(identifier: &str) -> Option<&str> {
    if let Some(local) = identifier.strip_prefix(UNIT_PREFIX) {
        return QUDT_UNITS
            .iter()
            .find(|(qudt, _)| *qudt == local)
            .map(|(_, unit)| *unit);
    }
    S231_FALLBACKS
        .iter()
        .find_map(|prefix| identifier.strip_prefix(prefix))
        .filter(|unit| !unit.is_empty())
}

/// Every Modelica quantity a `qudt:hasQuantityKind` identifier can stand for, in table order.
/// Empty for an identifier outside the table; more than one where the forward table is lossy.
pub fn modelica_quantities(identifier: &str) -> Vec<&'static str> {
    let Some(local) = identifier.strip_prefix(QUANTITY_PREFIX) else {
        return Vec::new();
    };
    QUANTITY_KINDS
        .iter()
        .filter(|(_, kind)| *kind == local)
        .map(|(quantity, _)| *quantity)
        .collect()
}

/// `(class, parameter or connector, Modelica quantity)`: the declarations whose quantity kind
/// has more than one inverse, each resolved by hand against its vendored `.mo` declaration. An
/// ambiguous kind on any other subject refuses; the units test checks each row's
/// `quantity="…"` against the source.
pub const RESOLVED_QUANTITIES: &[(&str, &str, &str)] = &[(
    "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi",
    "pAtm",
    "Pressure",
)];
