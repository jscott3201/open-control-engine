//! Unit suffixes against declared units: `unit-suffix-mismatch` (vocabulary note, "Units",
//! decision 14 and ruling 6; issue #501).
//!
//! A suffix is text-only and never reaches the engine, so this check is stage 1's alone and
//! pre-empts no engine code. On a block parameter, a suffix must reduce to the unit the CDL source
//! declares for it (`oce-vocab`'s `unit`): a base-unit expression reduces to itself, a scaled time
//! unit to its base (`h` to `s`), and the two must be spelled alike (`delayTime = 5 K` and
//! `delayTime = 1 Hz` are refused; `1 h` and `3600 s` are accepted). On a parameter that declares no
//! unit, and on a sequence parameter (which never declares one), a base-unit suffix stays an
//! annotation, as in M2, and a scaled one is refused, because nothing says what it scales to. A
//! suffix that is no unit expression at all, which only a model built in code can hold, is
//! refused the same way.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use oce_api::ClassParameter;

use crate::text::{SuffixKind, check_suffix};

/// The unit `class_path`'s parameter `parameter` declares (`s`), or `""` when it declares none.
/// An index over the embedded vocabulary, built once per process; an array template (`k_<i>`)
/// is looked up under its base name (`k`), as the vocabulary lists it.
pub(super) fn declared_unit(class_path: &str, parameter: &ClassParameter) -> &'static str {
    static INDEX: OnceLock<BTreeMap<(&'static str, &'static str), &'static str>> = OnceLock::new();
    let index = INDEX.get_or_init(|| {
        oce_vocab::vocabulary()
            .generated()
            .classes
            .iter()
            .flat_map(|class| {
                class.parameters.iter().map(move |p| {
                    (
                        (class.class_path.as_str(), p.name.as_str()),
                        p.unit.as_str(),
                    )
                })
            })
            .collect()
    });
    let name = oce_vocab::parameter_base_name(parameter.name);
    index.get(&(class_path, name)).copied().unwrap_or("")
}

/// Why `suffix` cannot stand on a parameter declared in `declared` (`""` for none), as a finding
/// detail about `what` (`` parameter `delayTime` of `CDL.Logical.TrueDelay` ``); `None` when it
/// can.
pub(super) fn mismatch(what: &str, declared: &str, suffix: &str) -> Option<String> {
    let kind = match check_suffix(suffix) {
        Ok(kind) => kind,
        Err((_, why)) => return Some(format!("{what}: the suffix is no unit: {why}")),
    };
    let reduced = kind.reduced(suffix);
    match (declared, kind) {
        ("", SuffixKind::Base) => None,
        ("", SuffixKind::Scaled(row)) => Some(format!(
            "{what} declares no unit, so the scaled suffix `{suffix}` has nothing to scale to; write the value in `{}` or leave the suffix out",
            row.base
        )),
        (declared, _) if reduced == declared => None,
        (declared, SuffixKind::Scaled(_)) => Some(format!(
            "{what} is declared in `{declared}`; the suffix `{suffix}` is a multiple of `{reduced}`"
        )),
        (declared, SuffixKind::Base) => Some(format!(
            "{what} is declared in `{declared}`; the suffix `{suffix}` is not that unit"
        )),
    }
}

#[cfg(test)]
#[path = "suffixes_tests.rs"]
mod tests;
