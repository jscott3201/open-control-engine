//! Validation of flattened CDL vector parameters, `base[len]`: `k[nin]` (`MultiSum`),
//! `extract[nout]` (the `ExtractSignal` blocks) and `msk[nin]` (the `VectorFilter` blocks).
//!
//! Each rule reads the block's elements of `base` in one pass over its parameters and judges
//! every element it finds against the resolved length `n`: the `len` parameter, or the class's
//! literal default when the block omits it (GitHub #626). An element whose index is 0 or past
//! `n` is refused as `parameter-out-of-range`, so a value the author wrote is never dropped
//! without a word, as the matrix and offset rules already refuse a cell outside their shape.
//! Inside the length, each present element is checked for its kind (and the selector rule for
//! its range) as before.
//!
//! Which names are elements: `<base>_<index>` with `index` in the one decimal spelling
//! `0|[1-9][0-9]*`. A signed index or one with a leading zero is the spelling check's
//! (`non-canonical-element-name`, `params_element_names`), so it is not judged here a second
//! time. An index too large for `usize` is past every length. A suffix that is not a decimal
//! index at all (`k_x`, `k_`, `k_1_2` on a vector) names no element of this vector; a parameter
//! name the class does not declare is GitHub #467's other half, not this check.
//!
//! Cost: one pass over the block's parameters per rule, then one ordered lookup per index the
//! rule judges, with no string built per index. The pass replaced a lookup that scanned every
//! parameter for each index up to `n`, quadratic in a block's parameters at the 2^20 width.

use std::collections::BTreeMap;

use oce_blocks::{DefaultLiteral, DefaultSource, MAX_RESOLVED_PORT_WIDTH};
use oce_diag::Diagnostic;
use oce_model::{BlockInstance, Value};

use super::params_element_names::is_canonical_index;
use super::params_flattened::{
    boolean_value, find_param, integer_param_or_default, integer_value, push_range_error,
    real_value,
};

pub(super) struct IntegerArrayRangeRule {
    pub(super) base: &'static str,
    pub(super) len: &'static str,
    pub(super) len_default: i64,
    pub(super) min: i64,
    pub(super) max: &'static str,
    pub(super) max_default: i64,
    pub(super) default_to_index: bool,
}

/// The present elements of one vector inside its resolved length, by index: the element's name
/// and value.
type Elements<'a> = BTreeMap<usize, (&'a str, &'a Value)>;

/// The resolved length of a vector whose length parameter is `len`: the block's value, else the
/// class's literal Integer default. `None` when the value is not a non-negative Integer, or the
/// default is required or derived: another rule refuses the length parameter itself, and no
/// element can be judged against an unknown length.
fn resolved_length(blk: &BlockInstance, len: &str) -> Option<usize> {
    let value = match find_param(&blk.params, len) {
        Some(value) => integer_value(value)?,
        None => class_default_integer(&blk.class_iri, len)?,
    };
    usize::try_from(value).ok()
}

/// The class's literal Integer default for parameter `name`, if it has one.
fn class_default_integer(class_iri: &str, name: &str) -> Option<i64> {
    oce_blocks::class_parameters(class_iri)?
        .iter()
        .find(|parameter| parameter.name == name)
        .and_then(|parameter| match parameter.default {
            DefaultSource::Literal(DefaultLiteral::Integer(value)) => Some(value),
            _ => None,
        })
}

/// One pass over `blk`'s parameters for the elements of `base`: refuse each element whose index
/// is 0 or past `n` (an index too large for `usize` included), and return the rest by index, the
/// first of a repeated name kept (a repeat is refused as `duplicate-parameter` on its own).
fn elements<'a>(
    blk: &'a BlockInstance,
    diags: &mut Vec<Diagnostic>,
    base: &str,
    n: usize,
) -> Elements<'a> {
    let mut present = Elements::new();
    for (name, value) in &blk.params.values {
        let Some(index) = name
            .strip_prefix(base)
            .and_then(|rest| rest.strip_prefix('_'))
        else {
            continue;
        };
        if !is_canonical_index(index) {
            continue;
        }
        match index.parse::<usize>() {
            Ok(idx) if (1..=n).contains(&idx) => {
                present.entry(idx).or_insert((name.as_ref(), value));
            }
            _ => push_range_error(
                blk,
                diags,
                format!(
                    "parameter `{name}` on block `{}` is outside resolved array length {n}",
                    blk.class_iri
                ),
            ),
        }
    }
    present
}

/// The present elements a rule checks for their kind: inside the length and the port-width
/// bound, in index order.
fn judged<'e, 'a>(
    present: &'e Elements<'a>,
    n: usize,
) -> impl Iterator<Item = &'e (&'a str, &'a Value)> {
    present
        .range(1..=n.min(MAX_RESOLVED_PORT_WIDTH))
        .map(|(_, element)| element)
}

pub(super) fn check_integer_array_elements(
    blk: &BlockInstance,
    diags: &mut Vec<Diagnostic>,
    base: &str,
    len: &str,
) {
    let Some(n) = resolved_length(blk, len) else {
        return;
    };
    let present = elements(blk, diags, base, n);
    for (name, value) in judged(&present, n) {
        if integer_value(value).is_none() {
            push_range_error(
                blk,
                diags,
                format!(
                    "parameter `{name}` on block `{}` must be an integer array element",
                    blk.class_iri
                ),
            );
        }
    }
}

pub(super) fn check_integer_array_elements_in_range(
    blk: &BlockInstance,
    diags: &mut Vec<Diagnostic>,
    rule: IntegerArrayRangeRule,
) {
    let Some(n) = integer_param_or_default(&blk.params, rule.len, rule.len_default) else {
        return;
    };
    let Some(upper) = integer_param_or_default(&blk.params, rule.max, rule.max_default) else {
        return;
    };
    let Ok(n) = usize::try_from(n) else {
        return;
    };
    let present = elements(blk, diags, rule.base, n);
    for idx in 1..=n.min(MAX_RESOLVED_PORT_WIDTH) {
        // The element's name when present; a defaulted selector is named only if refused.
        let (name, member): (Option<&str>, i64) = match present.get(&idx) {
            Some(&(name, value)) => {
                let Some(value) = integer_value(value) else {
                    push_range_error(
                        blk,
                        diags,
                        format!(
                            "parameter `{name}` on block `{}` must be an integer array element",
                            blk.class_iri
                        ),
                    );
                    continue;
                };
                (Some(name), value)
            }
            None if rule.default_to_index => (None, idx as i64),
            None => continue,
        };
        if member < rule.min || member > upper {
            let name = name.map_or_else(|| format!("{}_{idx}", rule.base), str::to_owned);
            push_range_error(
                blk,
                diags,
                format!(
                    "parameter `{name}` on block `{}` must be in range {}..={upper}; got {member}",
                    blk.class_iri, rule.min
                ),
            );
        }
    }
}

pub(super) fn check_real_array_elements(
    blk: &BlockInstance,
    diags: &mut Vec<Diagnostic>,
    base: &str,
    len: &str,
) {
    let Some(n) = resolved_length(blk, len) else {
        return;
    };
    let present = elements(blk, diags, base, n);
    for (name, value) in judged(&present, n) {
        if real_value(value).is_none() {
            push_range_error(
                blk,
                diags,
                format!(
                    "parameter `{name}` on block `{}` must be a numeric real array element",
                    blk.class_iri
                ),
            );
        }
    }
}

pub(super) fn check_boolean_array_elements(
    blk: &BlockInstance,
    diags: &mut Vec<Diagnostic>,
    base: &str,
    len: &str,
) {
    let Some(n) = resolved_length(blk, len) else {
        return;
    };
    let present = elements(blk, diags, base, n);
    for (name, value) in judged(&present, n) {
        if boolean_value(value).is_none() {
            push_range_error(
                blk,
                diags,
                format!(
                    "parameter `{name}` on block `{}` must be a Boolean array element",
                    blk.class_iri
                ),
            );
        }
    }
}

#[cfg(test)]
#[path = "params_arrays_tests.rs"]
mod tests;
