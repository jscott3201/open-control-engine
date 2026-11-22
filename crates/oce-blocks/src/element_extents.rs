//! Whether an array or matrix element a block binds lies inside its class's shape (issue #772).
//!
//! The load gate (`oce-validate`) refuses an element past its array's resolved extent
//! (`parameter-out-of-range`, GitHub #635) and a table whose cells do not fill their rectangle.
//! The compiled-schedule blob decoder (`oce-graph`) has no load gate in front of it, so it asks
//! [`element_outside_extent`] the same question of a decoded parameter table before the block is
//! built from it. Before this check a blob could carry an element the load gate refuses: the
//! block dropped it (`k_3` on a `MultiSum` of `nin = 2`), or read it and changed shape
//! (`table_3_2` on a two-row `TimeTable` changed its output).
//!
//! # The shape rules
//!
//! Each class's [`ParamRule`]s say where its elements' extents come from:
//!
//! - a vector, `base_<i>` ([`ParamRule::RealArrayElements`], [`ParamRule::IntegerArrayElements`],
//!   [`ParamRule::BooleanArrayElements`], [`ParamRule::IntegerArrayElementsInRange`]): `1..=n`, `n`
//!   the block's `len` parameter, else the class's literal default (a negative `n` admits no
//!   element);
//! - a matrix whose cells set its shape, `base_<r>_<c>` ([`ParamRule::RealMatrixElements`],
//!   `MatrixGain.K`; [`ParamRule::TimeTableMatrix`], the `TimeTable` sources' `table`): the cells
//!   bound must fill the rectangle their largest row and column span, every index from 1;
//! - a `TimeTable` offset, `base_<i>` ([`ParamRule::TimeTableOffset`]): `1..=cols - 1`, `cols` the
//!   table's column count (no element when the table has fewer than two columns).
//!
//! An index of zero, or one too large for `usize`, is outside every extent. A name that is no
//! element of any of these is not judged here ([`crate::declares_parameter`] judges names).
//!
//! # Cost
//!
//! One pass over the parameters per rule the class publishes (a vector's length is found in the
//! same kind of pass), constant work per name, nothing allocated: the completeness of a matrix is
//! a count of its distinct bound cells against the product of its largest indices, so a hostile
//! table of a million names costs a few passes over them. Total; never panics.

use oce_model::{ParamTable, Value};

use crate::{DefaultLiteral, DefaultSource};
use crate::{ParamRule, registry};

/// The cells of a matrix parameter `base_<row>_<col>` a parameter table binds: the largest row and
/// column among them, how many distinct cells there are, and the position (in the table) of the
/// cell with the largest row, then column. Indices are canonical decimals; a cell with an index
/// of zero is counted in `zero_index`, not in the shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BoundCells {
    /// The largest row index bound.
    pub rows: usize,
    /// The largest column index bound.
    pub cols: usize,
    /// How many cells with both indices from 1 are bound (names are unique in a decoded table;
    /// a repeated name in a resolved model is counted each time it appears).
    pub count: usize,
    /// The table position of the bound cell with the largest `(row, col)`, if any.
    pub last: Option<usize>,
    /// The table position of the first cell with an index of zero or one past `usize`, if any.
    pub outside: Option<usize>,
}

impl BoundCells {
    /// Whether the bound cells fill the `rows × cols` rectangle they span (an empty matrix
    /// fills its empty one). `false` when the product overflows.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.rows
            .checked_mul(self.cols)
            .is_some_and(|area| area == self.count)
    }
}

/// The cells of the matrix `base` that `params` binds (see [`BoundCells`]). One pass; allocates
/// nothing.
#[must_use]
pub fn bound_cells(params: &ParamTable, base: &str) -> BoundCells {
    let mut cells = BoundCells::default();
    let mut last = (0, 0);
    for (position, (name, _)) in params.values.iter().enumerate() {
        let Some((row, col)) = element_suffix(name, base).and_then(|rest| rest.split_once('_'))
        else {
            continue;
        };
        if col.contains('_') {
            continue;
        }
        match (index(row), index(col)) {
            (Some(Some(row)), Some(Some(col))) => {
                if (row, col) > last {
                    last = (row, col);
                    cells.last = Some(position);
                }
                cells.rows = cells.rows.max(row);
                cells.cols = cells.cols.max(col);
                cells.count += 1;
            }
            (Some(_), Some(_)) => {
                cells.outside.get_or_insert(position);
            }
            _ => {}
        }
    }
    cells
}

/// The position in `params.values` of the first element the registered class `class_path` binds
/// outside its shape (see the module documentation), or `None` when every element is inside it
/// or the class is unregistered.
#[must_use]
pub fn element_outside_extent(class_path: &str, params: &ParamTable) -> Option<usize> {
    let entry = registry::lookup(class_path)?;
    entry
        .param_rules()
        .iter()
        .find_map(|rule| rule_outside_extent(class_path, params, rule))
}

fn rule_outside_extent(class_path: &str, params: &ParamTable, rule: &ParamRule) -> Option<usize> {
    match *rule {
        ParamRule::RealArrayElements { base, len }
        | ParamRule::IntegerArrayElements { base, len }
        | ParamRule::BooleanArrayElements { base, len } => {
            let n = vector_length(params, len, class_default_integer(class_path, len))?;
            vector_outside(params, base, n)
        }
        ParamRule::IntegerArrayElementsInRange {
            base,
            len,
            len_default,
            ..
        } => vector_outside(params, base, vector_length(params, len, Some(len_default))?),
        ParamRule::RealMatrixElements { base, .. } | ParamRule::TimeTableMatrix { base, .. } => {
            let cells = bound_cells(params, base);
            cells
                .outside
                .or_else(|| (!cells.is_complete()).then_some(cells.last).flatten())
        }
        ParamRule::TimeTableOffset { base, table } => {
            let nout = bound_cells(params, table).cols.saturating_sub(1);
            vector_outside(params, base, nout)
        }
        _ => None,
    }
}

/// The position of the first element `base_<i>` of `params` outside `1..=n`.
fn vector_outside(params: &ParamTable, base: &str, n: usize) -> Option<usize> {
    params.values.iter().position(|(name, _)| {
        element_suffix(name, base)
            .filter(|rest| !rest.contains('_'))
            .and_then(index)
            .is_some_and(|index| index.is_none_or(|index| index > n))
    })
}

/// A vector's resolved length: the block's Integer `len` (a negative one is 0), else `default`.
/// `None` when the block binds `len` to a value of another kind, or omits it and the class has no
/// literal default: no element can be judged against an unknown length.
fn vector_length(params: &ParamTable, len: &str, default: Option<i64>) -> Option<usize> {
    let value = match params.values.iter().find(|(name, _)| name.as_ref() == len) {
        Some((_, Value::Integer(value))) => *value,
        Some(_) => return None,
        None => default?,
    };
    Some(usize::try_from(value).unwrap_or(0))
}

/// The class's literal Integer default for parameter `name`, if it has one. Reads the static
/// default table; allocates nothing.
fn class_default_integer(class_path: &str, name: &str) -> Option<i64> {
    registry::param_defaults(class_path)
        .iter()
        .find(|parameter| parameter.name == name)
        .and_then(|parameter| match parameter.default {
            DefaultSource::Literal(DefaultLiteral::Integer(value)) => Some(value),
            _ => None,
        })
}

/// `name`'s text after `base_`, when it starts so.
fn element_suffix<'a>(name: &'a str, base: &str) -> Option<&'a str> {
    name.strip_prefix(base)?.strip_prefix('_')
}

/// A canonical decimal index (`0|[1-9][0-9]*`): `Some(Some(i))` for `i >= 1`, `Some(None)` for
/// zero or a number too large for `usize` (outside every extent), `None` for any other text (no
/// element; the name check refuses it).
fn index(text: &str) -> Option<Option<usize>> {
    let bytes = text.as_bytes();
    let canonical = match bytes {
        [] => false,
        [b'0'] => true,
        [first, rest @ ..] => (b'1'..=b'9').contains(first) && rest.iter().all(u8::is_ascii_digit),
    };
    canonical.then(|| text.parse::<usize>().ok().filter(|index| *index > 0))
}

#[cfg(test)]
#[path = "element_extents_tests.rs"]
mod tests;
