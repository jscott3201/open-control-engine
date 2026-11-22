//! Which parameter names a registered class declares, and the declared name an undeclared one
//! probably meant (issue #748).
//!
//! A block reads its parameters by name and falls back to a default for a name it does not find,
//! so a binding under a name the class does not declare was read by nothing: a misspelt
//! `amplitud` on `Integers.Sources.Pulse` loaded clean and the block ran on the default
//! amplitude. The load gate (`oce-validate`) therefore refuses such a name, and the
//! compiled-schedule blob decoder (`oce-graph`) refuses a blob that carries one; both ask
//! [`declares_parameter`].
//!
//! # What a class declares
//!
//! The names of [`crate::class_parameters`]: every authored default, then every name a parameter
//! rule names without a default (the `TimeTable` sources' `table_<row>_<col>`). A scalar is
//! declared under its own name; an array parameter's elements under the flattened spelling the
//! CXF import writes, each index a one-based decimal with no sign and no leading zero (`k_2`,
//! `table_1_2`; [`crate::ParamNameTemplate::names`]). An element past the array's extent is a
//! declared name: the class's extent rules refuse it, by its index.
//!
//! The catalog lists every public parameter of every vendored CDL class: the `oce-validate`
//! tests hold it equal, class by class, to the parameters the pinned Modelica sources declare.
//!
//! # Element spellings
//!
//! [`spells_array_element`] says whether a name has the shape of an element of one of the class's
//! arrays whatever its index spelling: `<base>_<a>` (vector) or `<base>_<a>_<b>` (matrix), each
//! index an optionally signed run of decimal digits. A non-canonical index (`k_01`, `k_+1`) is
//! the spelling check's refusal (`non-canonical-element-name`, GitHub #470) and an index of zero
//! the extent rules' (`parameter-out-of-range`, GitHub #635); the load gate leaves such a name to
//! them rather than refusing it twice. A suffix that is not an index at all (`k_x`, `k_`, `k_1_2`
//! on a vector) names no element and is undeclared.
//!
//! # The suggestion
//!
//! [`nearest_declared_parameter`] returns the declared name closest to an undeclared one, when
//! one is close: the same name in another letter case, or one within a few single-byte edits
//! (insertions, deletions, substitutions and swaps of two adjacent bytes; the optimal string
//! alignment distance), at most [`MAX_SUGGESTION_DISTANCE`] and at most a third of the declared
//! name's length, so a one- or two-letter name is suggested only for a case slip. An
//! element-shaped name is also compared, by its stem, with each array of the same rank
//! (`tabel_1_2` suggests `table_<row>_<col>`), and an array's bare base (`k` on `MultiSum`)
//! suggests its template. The smallest distance wins, then catalog order.
//!
//! Cost: a declared name whose length differs from the compared text by more than the allowed
//! distance is skipped after one comparison of lengths, so a long name costs one pass over the
//! class's declared names and a scan of its own bytes for the element shape. Otherwise the
//! distance is a dynamic programme over two short strings (no catalog name is longer than
//! [`MAX_COMPARED_NAME_BYTES`]), in three rows on the stack. Nothing allocates; nothing panics.

use crate::class_metadata::rule_parameters;
use crate::{ParamNameTemplate, RegistryEntry, registry};

/// The most single-byte edits between an undeclared name and the declared name it is taken to
/// mean. A declared name of fewer than three times as many bytes allows fewer (a third of its
/// length, rounded down).
pub const MAX_SUGGESTION_DISTANCE: usize = 2;

/// The longest declared name the suggestion compares. Every catalog name is shorter (a test
/// holds it), so the bound only sizes the distance table.
pub const MAX_COMPARED_NAME_BYTES: usize = 64;

/// Whether the registered class `class_path` declares the bound parameter `name`: a scalar
/// parameter's own name, or an element of one of its arrays in the canonical flattened spelling
/// (see the module documentation). `None` for an unregistered class. Total; allocates nothing.
#[must_use]
pub fn declares_parameter(class_path: &str, name: &str) -> Option<bool> {
    let entry = registry::lookup(class_path)?;
    Some(declared_names(entry).any(|declared| ParamNameTemplate::parse(declared).names(name)))
}

/// Whether `name` has the shape of an element of one of the class's array parameters, in any
/// index spelling: `<base>_<index>` for a vector, `<base>_<row>_<col>` for a matrix, each index an
/// optionally signed run of decimal digits. `false` for an unregistered class. Total; allocates
/// nothing.
#[must_use]
pub fn spells_array_element(class_path: &str, name: &str) -> bool {
    let Some(entry) = registry::lookup(class_path) else {
        return false;
    };
    declared_names(entry).any(|declared| {
        let template = ParamNameTemplate::parse(declared);
        template.rank() > 0
            && element_indices(name, template.base()).is_some_and(|indices| {
                let mut count = 0;
                let all_digits = indices.split('_').all(|index| {
                    count += 1;
                    is_signed_decimal(index)
                });
                all_digits && count == template.rank()
            })
    })
}

/// The declared parameter of the registered class `class_path` closest to the undeclared `name`,
/// as its catalog name (an array under its template, `k_<i>`), or `None` when none is close
/// enough or the class is unregistered (see the module documentation for the measure). Total;
/// allocates nothing.
#[must_use]
pub fn nearest_declared_parameter(class_path: &str, name: &str) -> Option<&'static str> {
    let entry = registry::lookup(class_path)?;
    let mut best: Option<(usize, &'static str)> = None;
    for declared in declared_names(entry) {
        let template = ParamNameTemplate::parse(declared);
        let compared = match template.rank() {
            0 => Some(name),
            rank => element_stem(name, rank),
        };
        // An array's bare base (`k` on `MultiSum`) is the array written as a scalar: its
        // template is the nearest name there is.
        let bare_base = template.rank() > 0 && name == template.base();
        let Some(distance) = (if bare_base {
            Some(0)
        } else {
            compared.and_then(|text| suggestion_distance(text.as_bytes(), template.base()))
        }) else {
            continue;
        };
        if best.is_none_or(|(nearest, _)| distance < nearest) {
            best = Some((distance, declared));
        }
    }
    best.map(|(_, declared)| declared)
}

/// Every catalog name of a registered class's parameters, in [`crate::class_parameters`] order:
/// the authored defaults, then the names a parameter rule names (a name a rule names beside its
/// default comes again; a reader takes the first). Allocates nothing.
fn declared_names(entry: &'static RegistryEntry) -> impl Iterator<Item = &'static str> {
    registry::param_defaults(entry.class_path)
        .iter()
        .map(|param| param.name)
        .chain(
            entry
                .param_rules()
                .iter()
                .flat_map(|rule| rule_parameters(rule).into_iter().flatten())
                .map(|(name, _)| name),
        )
}

/// The index text of `name` read as an element of the array `base`: what follows `<base>_`.
fn element_indices<'a>(name: &'a str, base: &str) -> Option<&'a str> {
    name.strip_prefix(base)?.strip_prefix('_')
}

/// The stem of an element-shaped `name` of `rank` indices: `name` less its last `rank`
/// `_<digits>` fields, or `None` when it does not end in that many (or the stem is empty).
fn element_stem(name: &str, rank: usize) -> Option<&str> {
    let mut stem = name;
    for _ in 0..rank {
        let (head, index) = stem.rsplit_once('_')?;
        if !is_signed_decimal(index) {
            return None;
        }
        stem = head;
    }
    (!stem.is_empty()).then_some(stem)
}

/// `[+-]?[0-9]+`: any spelling an integer parser could read as an index.
fn is_signed_decimal(text: &str) -> bool {
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// The distance the suggestion ranks `text` by against the declared name `declared`: 0 for the
/// same name in another letter case, else the optimal string alignment distance plus one, or
/// `None` past the allowed distance or for an identical name (a declared name is never
/// suggested for itself).
fn suggestion_distance(text: &[u8], declared: &str) -> Option<usize> {
    let declared = declared.as_bytes();
    if text == declared || declared.len() > MAX_COMPARED_NAME_BYTES {
        return None;
    }
    if text.eq_ignore_ascii_case(declared) {
        return Some(0);
    }
    let limit = MAX_SUGGESTION_DISTANCE.min(declared.len() / 3);
    bounded_edit_distance(text, declared, limit).map(|distance| distance + 1)
}

/// The optimal string alignment distance between `a` and `b` (insert, delete, substitute, swap
/// two adjacent bytes), or `None` when it exceeds `limit`. `b` is at most
/// [`MAX_COMPARED_NAME_BYTES`] long; `a` is rejected by its length alone when it cannot be within
/// `limit`, before any table is filled.
fn bounded_edit_distance(a: &[u8], b: &[u8], limit: usize) -> Option<usize> {
    if a.len().abs_diff(b.len()) > limit || b.len() > MAX_COMPARED_NAME_BYTES {
        return None;
    }
    const WIDTH: usize = MAX_COMPARED_NAME_BYTES + 1;
    let mut before = [0usize; WIDTH];
    let mut previous: [usize; WIDTH] = core::array::from_fn(|column| column);
    let mut current = [0usize; WIDTH];
    for (row, &left) in a.iter().enumerate() {
        current[0] = row + 1;
        let mut row_minimum = current[0];
        for (column, &right) in b.iter().enumerate() {
            let substitution = previous[column] + usize::from(left != right);
            let mut cell = substitution
                .min(previous[column + 1] + 1)
                .min(current[column] + 1);
            if row > 0 && column > 0 && left == b[column - 1] && a[row - 1] == right {
                cell = cell.min(before[column - 1] + 1);
            }
            current[column + 1] = cell;
            row_minimum = row_minimum.min(cell);
        }
        if row_minimum > limit {
            return None;
        }
        before = previous;
        previous = current;
    }
    let distance = previous[b.len()];
    (distance <= limit).then_some(distance)
}

#[cfg(test)]
#[path = "declared_parameters_tests.rs"]
mod tests;
