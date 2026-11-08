//! Per-class signature hash and the target-bound class flag (compiled-schedule blob, GitHub #400).
//!
//! A host that encodes an `OCEB` blob and a target that decodes it run the same function here, so a
//! class whose interface, state layout, parameter schema or placement policy differs between the
//! two builds is refused by hash instead of being executed with a different meaning
//! (`docs/design/compiled-schedule-blob.md`, decision 2).
//!
//! The hash is FNV-1a-64 over one canonical record per registered class, built from a
//! default-parameter instance. The record holds, in order:
//!
//! 1. the registered `class_path`;
//! 2. the default instance's resolved kind, `state_len` and `state_contract_revision`;
//! 3. the default instance's input and output port kinds, each with its declared name (if any);
//! 4. the naming policy and the `width_driven`, `reserved` and `target_bound` flags;
//! 5. every [`ParamRule`], as a variant tag and all of its fields;
//! 6. every [`ParamDefault`], as its name, its source tag and its literal value.
//!
//! A [`DefaultSource::Derived`] default contributes only its source tag: its formula text is
//! documentation, and a wording edit must not refuse every blob that uses the class. Counts are
//! `u32` little-endian, Reals are their IEEE-754 bits and strings are a `u32` byte length followed
//! by their UTF-8 bytes. Every tag table is an exhaustive `match`, so a new rule, default or port
//! kind cannot compile without a tag.
//!
//! The hash does not cover tick behaviour, `validate_state`, non-default arities, catalog order or
//! derived-default formula text; the blob decoder checks arity, port kinds and state layout per
//! instance instead, and behavioural identity rests on the exact engine-version match.
//!
//! Everything here is `no_std + alloc`: the decoder on a bare-metal target calls it once per class.

use oce_model::{ValueType, enum_descriptor};

use crate::{
    BlockKind, DefaultLiteral, DefaultSource, ParamDefault, ParamRule, PortKind, PortNaming,
    RegistryEntry, TimeTableValues, port_names, registry,
};

/// The classes whose outputs a host may compute differently from another architecture: their
/// state snapshots are bound to the capturing target (`docs/state-compatibility.md`). Sorted by
/// UTF-8 bytes for [`class_is_target_bound`]'s binary search. This is the one list: `oce-api`'s
/// snapshot placement policy and the compiled-schedule blob both read it through
/// [`class_is_target_bound`].
const TARGET_BOUND_CLASSES: &[&str] = &[
    "CDL.Psychrometrics.DewPoint_TDryBulPhi",
    "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi",
    "CDL.Psychrometrics.WetBulb_TDryBulPhi",
    "CDL.Reals.Acos",
    "CDL.Reals.Asin",
    "CDL.Reals.Atan",
    "CDL.Reals.Atan2",
    "CDL.Reals.Cos",
    "CDL.Reals.Exp",
    "CDL.Reals.Log",
    "CDL.Reals.Log10",
    "CDL.Reals.Sin",
    "CDL.Reals.Sources.Sin",
    "CDL.Reals.Tan",
    "CDL.Utilities.SunRiseSet",
];

/// Whether `class_path` names a target-bound class: one whose outputs depend on the host's
/// transcendental math library, so an engine state that contains it is placement-restricted to the
/// capturing architecture and operating system.
///
/// Exactly 15 registered classes are target-bound. The flag is part of the class signature record
/// ([`class_signature_hash`]); an unregistered path returns `false`. Pure, allocation-free and
/// panic-free.
#[must_use]
pub fn class_is_target_bound(class_path: &str) -> bool {
    TARGET_BOUND_CLASSES.binary_search(&class_path).is_ok()
}

/// The FNV-1a-64 class signature hash of the registered class `class_path`, or `None` when the
/// class is not registered.
///
/// The record holds the class path; the default instance's kind, state length and state-contract
/// revision; its port kinds and names; the naming policy and the width-driven, reserved and
/// target-bound flags; every parameter rule; and every parameter default, a derived default's
/// formula text excepted (it is documentation). The function constructs one
/// default-parameter instance through the registry (the same construction [`crate::catalog()`]
/// performs) and streams the record into the hash without allocating a buffer for it. Identical on
/// every architecture: every field is fixed-width little-endian and no `usize` reaches the record.
/// Never panics for a registered class.
#[must_use]
pub fn class_signature_hash(class_path: &str) -> Option<u64> {
    registry::lookup(class_path).map(entry_signature_hash)
}

/// The class signature hash of one registry entry (see [`class_signature_hash`]).
pub(crate) fn entry_signature_hash(entry: &RegistryEntry) -> u64 {
    with_entry_record(entry, |record| record.hash())
}

/// Build one registry entry's canonical record from a default-parameter instance and hand it to
/// `f`. The record borrows the instance's resolved signature, so it cannot outlive this call.
pub(crate) fn with_entry_record<R>(
    entry: &RegistryEntry,
    f: impl FnOnce(&ClassRecord<'_>) -> R,
) -> R {
    let block = (entry.make)(&oce_model::ParamTable::default());
    let signature = block.resolved_signature();
    let names = port_names::port_names(entry.class_path);
    let (naming, width_driven) = naming_policy(entry);
    let record = ClassRecord {
        class_path: entry.class_path,
        kind: block.kind(),
        state_len: block.state_len(),
        state_contract_revision: block.state_contract_revision(),
        inputs: &signature.inputs,
        outputs: &signature.outputs,
        input_names: names.map(|names| names.inputs),
        output_names: names.map(|names| names.outputs),
        naming,
        width_driven,
        reserved: is_reserved(entry.class_path),
        target_bound: class_is_target_bound(entry.class_path),
        rules: entry.param_rules(),
        defaults: registry::param_defaults(entry.class_path),
    };
    f(&record)
}

/// The port naming policy and the `width_driven` flag of one registry entry, as the catalog
/// publishes them: a class with declared names is [`PortNaming::Named`]; otherwise a class with a
/// structural rule is [`PortNaming::WidthDriven`], and any other class is
/// [`PortNaming::Positional`].
pub(crate) fn naming_policy(entry: &RegistryEntry) -> (PortNaming, bool) {
    let width_driven = entry.param_rules().iter().any(|rule| {
        matches!(
            rule,
            ParamRule::Structural { .. } | ParamRule::StructuralArrayElements { .. }
        )
    });
    let naming = if port_names::port_names(entry.class_path).is_some() {
        PortNaming::Named
    } else if width_driven {
        PortNaming::WidthDriven
    } else {
        PortNaming::Positional
    };
    (naming, width_driven)
}

/// Whether `class_path` is an engine-reserved lowering identity (not authorable CDL).
pub(crate) fn is_reserved(class_path: &str) -> bool {
    class_path.starts_with("urn:oce:lowering#")
}

/// The canonical per-class record the signature hash covers, borrowed from a default instance and
/// the registry's static metadata. Kept separate from the registry so tests can perturb one field
/// at a time and observe the hash.
#[derive(Clone, Copy)]
pub(crate) struct ClassRecord<'a> {
    pub(crate) class_path: &'a str,
    pub(crate) kind: BlockKind,
    pub(crate) state_len: usize,
    pub(crate) state_contract_revision: u32,
    pub(crate) inputs: &'a [PortKind],
    pub(crate) outputs: &'a [PortKind],
    pub(crate) input_names: Option<&'a [&'a str]>,
    pub(crate) output_names: Option<&'a [&'a str]>,
    pub(crate) naming: PortNaming,
    pub(crate) width_driven: bool,
    pub(crate) reserved: bool,
    pub(crate) target_bound: bool,
    pub(crate) rules: &'a [ParamRule],
    pub(crate) defaults: &'a [ParamDefault],
}

impl ClassRecord<'_> {
    /// FNV-1a-64 over the canonical record bytes.
    pub(crate) fn hash(&self) -> u64 {
        let mut h = Fnv64::new();
        h.str(self.class_path);
        h.u8(match self.kind {
            BlockKind::Algebraic => 0,
            BlockKind::Stateful => 1,
        });
        h.count(self.state_len);
        h.u32(self.state_contract_revision);
        ports(&mut h, self.inputs, self.input_names);
        ports(&mut h, self.outputs, self.output_names);
        h.u8(match self.naming {
            PortNaming::Named => 0,
            PortNaming::Positional => 1,
            PortNaming::WidthDriven => 2,
        });
        h.u8(u8::from(self.width_driven));
        h.u8(u8::from(self.reserved));
        h.u8(u8::from(self.target_bound));
        h.count(self.rules.len());
        for rule in self.rules {
            param_rule(&mut h, rule);
        }
        h.count(self.defaults.len());
        for default in self.defaults {
            h.str(default.name);
            default_source(&mut h, &default.default);
        }
        h.finish()
    }
}

fn ports(h: &mut Fnv64, kinds: &[PortKind], names: Option<&[&str]>) {
    h.count(kinds.len());
    for (index, kind) in kinds.iter().enumerate() {
        h.u8(port_kind_tag(*kind));
        h.opt_str(names.and_then(|names| names.get(index).copied()));
    }
}

fn port_kind_tag(kind: PortKind) -> u8 {
    match kind {
        PortKind::Real => 0,
        PortKind::Integer => 1,
        PortKind::Boolean => 2,
    }
}

fn value_type(h: &mut Fnv64, value_type: ValueType) {
    match value_type {
        ValueType::Real => h.u8(0),
        ValueType::Integer => h.u8(1),
        ValueType::Boolean => h.u8(2),
        ValueType::String => h.u8(3),
        ValueType::Enum(class) => {
            h.u8(4);
            h.opt_str(enum_descriptor(class).map(|descriptor| descriptor.class_path));
            h.u32(class.0);
        }
    }
}

fn time_table_values_tag(values: TimeTableValues) -> u8 {
    match values {
        TimeTableValues::Real => 0,
        TimeTableValues::Integer => 1,
        TimeTableValues::Boolean => 2,
    }
}

/// One rule as a variant tag followed by every field in declaration order. Exhaustive on purpose:
/// a new [`ParamRule`] variant does not compile until it has a tag here.
fn param_rule(h: &mut Fnv64, rule: &ParamRule) {
    match *rule {
        ParamRule::Required { name, kind } => {
            h.u8(0);
            h.str(name);
            value_type(h, kind);
        }
        ParamRule::Structural { name } => {
            h.u8(1);
            h.str(name);
        }
        ParamRule::StructuralArrayElements { base } => {
            h.u8(2);
            h.str(base);
        }
        ParamRule::Boolean { name } => {
            h.u8(3);
            h.str(name);
        }
        ParamRule::Real { name } => {
            h.u8(4);
            h.str(name);
        }
        ParamRule::RealFinite { name } => {
            h.u8(5);
            h.str(name);
        }
        ParamRule::RealGreaterThan { name, min } => {
            h.u8(6);
            h.str(name);
            h.f64(min);
        }
        ParamRule::RealFiniteGreaterThan { name, min } => {
            h.u8(7);
            h.str(name);
            h.f64(min);
        }
        ParamRule::IntegerGreaterOrEqual { name, min } => {
            h.u8(8);
            h.str(name);
            h.i64(min);
        }
        ParamRule::IntegerLessOrEqualConstant { name, max } => {
            h.u8(9);
            h.str(name);
            h.i64(max);
        }
        ParamRule::IntegerArrayElements { base, len } => {
            h.u8(10);
            h.str(base);
            h.str(len);
        }
        ParamRule::IntegerArrayElementsInRange {
            base,
            len,
            len_default,
            min,
            max,
            max_default,
            default_to_index,
        } => {
            h.u8(11);
            h.str(base);
            h.str(len);
            h.i64(len_default);
            h.i64(min);
            h.str(max);
            h.i64(max_default);
            h.u8(u8::from(default_to_index));
        }
        ParamRule::RealArrayElements { base, len } => {
            h.u8(12);
            h.str(base);
            h.str(len);
        }
        ParamRule::RealMatrixElements {
            base,
            rows,
            default_rows,
            cols,
            default_cols,
        } => {
            h.u8(13);
            h.str(base);
            h.str(rows);
            h.i64(default_rows);
            h.str(cols);
            h.i64(default_cols);
        }
        ParamRule::TimeTableMatrix {
            base,
            values,
            time_scale,
            period,
            extrapolation,
        } => {
            h.u8(14);
            h.str(base);
            h.u8(time_table_values_tag(values));
            h.str(time_scale);
            h.opt_str(period);
            h.opt_str(extrapolation);
        }
        ParamRule::TimeTableOffset { base, table } => {
            h.u8(15);
            h.str(base);
            h.str(table);
        }
        ParamRule::BooleanArrayElements { base, len } => {
            h.u8(16);
            h.str(base);
            h.str(len);
        }
        ParamRule::BooleanArrayTrueCountEquals {
            base,
            len,
            count,
            default,
        } => {
            h.u8(17);
            h.str(base);
            h.str(len);
            h.str(count);
            h.u8(u8::from(default));
        }
        ParamRule::EnumMembers { name, members } => {
            h.u8(18);
            h.str(name);
            h.count(members.len());
            for member in members {
                h.str(member);
            }
        }
        ParamRule::RealGreaterOrEqual { name, min } => {
            h.u8(19);
            h.str(name);
            h.f64(min);
        }
        ParamRule::RealLessOrEqualConstant { name, max } => {
            h.u8(20);
            h.str(name);
            h.f64(max);
        }
        ParamRule::RealTimesIntegerInclusiveRange {
            real,
            integer,
            min,
            max,
        } => {
            h.u8(21);
            h.str(real);
            h.str(integer);
            h.f64(min);
            h.f64(max);
        }
        ParamRule::IntegerProductLessOrEqualConstant { left, right, max } => {
            h.u8(22);
            h.str(left);
            h.str(right);
            h.i64(max);
        }
        ParamRule::RealLessOrEqual { lower, upper } => {
            h.u8(23);
            h.str(lower);
            h.str(upper);
        }
        ParamRule::RealLessOrEqualWarning { lower, upper } => {
            h.u8(24);
            h.str(lower);
            h.str(upper);
        }
        ParamRule::RealGreaterOrEqualScaledWarning {
            left,
            right,
            factor,
        } => {
            h.u8(25);
            h.str(left);
            h.str(right);
            h.f64(factor);
        }
        ParamRule::RealEqualWarning { left, right } => {
            h.u8(26);
            h.str(left);
            h.str(right);
        }
    }
}

/// A default's source tag, then (for a literal) the literal's tag and value. A derived default's
/// formula text is documentation and deliberately contributes nothing beyond its tag.
fn default_source(h: &mut Fnv64, source: &DefaultSource) {
    match *source {
        DefaultSource::Literal(literal) => {
            h.u8(0);
            match literal {
                DefaultLiteral::Real(value) => {
                    h.u8(0);
                    h.f64(value);
                }
                DefaultLiteral::Integer(value) => {
                    h.u8(1);
                    h.i64(value);
                }
                DefaultLiteral::Boolean(value) => {
                    h.u8(2);
                    h.u8(u8::from(value));
                }
                DefaultLiteral::Str(value) => {
                    h.u8(3);
                    h.str(value);
                }
                DefaultLiteral::EnumMember(value) => {
                    h.u8(4);
                    h.str(value);
                }
            }
        }
        DefaultSource::Derived { formula: _ } => h.u8(1),
        DefaultSource::Required => h.u8(2),
    }
}

/// Streaming FNV-1a-64 (offset basis `0xcbf29ce484222325`, prime `0x100000001b3`).
pub(crate) struct Fnv64(u64);

impl Fnv64 {
    pub(crate) const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    pub(crate) fn bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn u8(&mut self, value: u8) {
        self.bytes(&[value]);
    }

    fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }

    /// A length or count as `u32` LE, saturating at `u32::MAX` (no registered class comes close).
    fn count(&mut self, value: usize) {
        self.u32(u32::try_from(value).unwrap_or(u32::MAX));
    }

    fn i64(&mut self, value: i64) {
        self.bytes(&value.to_le_bytes());
    }

    fn f64(&mut self, value: f64) {
        self.bytes(&value.to_bits().to_le_bytes());
    }

    fn str(&mut self, value: &str) {
        self.count(value.len());
        self.bytes(value.as_bytes());
    }

    fn opt_str(&mut self, value: Option<&str>) {
        match value {
            None => self.u8(0),
            Some(value) => {
                self.u8(1);
                self.str(value);
            }
        }
    }

    pub(crate) const fn finish(&self) -> u64 {
        self.0
    }
}
