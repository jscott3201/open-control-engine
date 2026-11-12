//! Parameter tables at which the width-driven and positional classes resolve their ports.
//!
//! The catalog names the ports of the 105 fixed-arity classes itself. The other 28 have ports
//! only once their width parameters are set, so the generator resolves each at one small table
//! (the [`probe`]) and the coverage tests at widths 1, 2 and the class maximum
//! ([`width_tables`]). The table of shapes below is hand-written from each class's CDL
//! declaration; a non-named class without a row fails the generator by name.

use oce_api::{CatalogRule, Value, catalog};

/// A flattened parameter table, as `oce_api::class_ports` reads one.
pub type Table = Vec<(String, Value)>;

/// How a class's port arity follows its parameters.
#[derive(Clone, Copy, Debug)]
enum Shape {
    /// One Integer width parameter.
    One(&'static str),
    /// Two Integer width parameters that the scenarios keep equal (a filter keeps every element,
    /// a selector's default picks element `i`).
    Equal(&'static str, &'static str),
    /// Two Integer width parameters whose product is bounded: a matrix, or a vector replicated.
    Product(&'static str, &'static str),
    /// A `TimeTable` source: one output per table column after the time column.
    TimeTable {
        /// Whether the class requires a `period`.
        period: bool,
    },
}

/// The shape of every class whose ports the catalog does not name.
const SHAPES: &[(&str, Shape)] = &[
    (
        "CDL.Reals.Sources.TimeTable",
        Shape::TimeTable { period: false },
    ),
    ("CDL.Reals.MultiMax", Shape::One("nin")),
    ("CDL.Reals.MultiMin", Shape::One("nin")),
    ("CDL.Reals.MultiSum", Shape::One("nin")),
    ("CDL.Reals.MatrixGain", Shape::Product("nout", "nin")),
    ("CDL.Reals.MatrixMax", Shape::Product("nRow", "nCol")),
    ("CDL.Reals.MatrixMin", Shape::Product("nRow", "nCol")),
    ("CDL.Reals.Sort", Shape::One("nin")),
    (
        "CDL.Logical.Sources.TimeTable",
        Shape::TimeTable { period: true },
    ),
    ("CDL.Logical.MultiAnd", Shape::One("nin")),
    ("CDL.Logical.MultiOr", Shape::One("nin")),
    (
        "CDL.Integers.Sources.TimeTable",
        Shape::TimeTable { period: true },
    ),
    ("CDL.Integers.MultiSum", Shape::One("nin")),
    (
        "CDL.Routing.BooleanExtractSignal",
        Shape::Equal("nin", "nout"),
    ),
    ("CDL.Routing.BooleanExtractor", Shape::One("nin")),
    ("CDL.Routing.BooleanScalarReplicator", Shape::One("nout")),
    (
        "CDL.Routing.BooleanVectorFilter",
        Shape::Equal("nin", "nout"),
    ),
    (
        "CDL.Routing.BooleanVectorReplicator",
        Shape::Product("nin", "nout"),
    ),
    (
        "CDL.Routing.IntegerExtractSignal",
        Shape::Equal("nin", "nout"),
    ),
    ("CDL.Routing.IntegerExtractor", Shape::One("nin")),
    ("CDL.Routing.IntegerScalarReplicator", Shape::One("nout")),
    (
        "CDL.Routing.IntegerVectorFilter",
        Shape::Equal("nin", "nout"),
    ),
    (
        "CDL.Routing.IntegerVectorReplicator",
        Shape::Product("nin", "nout"),
    ),
    ("CDL.Routing.RealExtractSignal", Shape::Equal("nin", "nout")),
    ("CDL.Routing.RealExtractor", Shape::One("nin")),
    ("CDL.Routing.RealScalarReplicator", Shape::One("nout")),
    ("CDL.Routing.RealVectorFilter", Shape::Equal("nin", "nout")),
    (
        "CDL.Routing.RealVectorReplicator",
        Shape::Product("nin", "nout"),
    ),
];

fn shape(class_path: &str) -> Option<Shape> {
    SHAPES
        .iter()
        .find(|(class, _)| *class == class_path)
        .map(|(_, shape)| *shape)
}

/// Every class path [`SHAPES`] has a row for.
pub fn shaped_classes() -> impl Iterator<Item = &'static str> {
    SHAPES.iter().map(|(class, _)| *class)
}

fn integers(values: &[(&str, i64)]) -> Table {
    values
        .iter()
        .map(|(name, value)| ((*name).to_owned(), Value::Integer(*value)))
        .collect()
}

/// A `rows` by `cols` time table (strictly increasing times in the first column, zeros
/// elsewhere), with a `period` past the last time where the class requires one.
fn time_table(rows: usize, cols: usize, period: bool) -> Table {
    let mut table = Table::new();
    for row in 1..=rows {
        for col in 1..=cols {
            let value = if col == 1 { (row - 1) as f64 } else { 0.0 };
            table.push((format!("table_{row}_{col}"), Value::Real(value)));
        }
    }
    if period {
        table.push(("period".to_owned(), Value::Real(10.0)));
    }
    table
}

/// The one table the generator resolves a class at: every vector connector two wide, every
/// matrix connector two by three, so a vector cannot be mistaken for a scalar or a matrix for a
/// vector. `None` for a class with no row in [`SHAPES`].
pub fn probe(class_path: &str) -> Option<Table> {
    Some(match shape(class_path)? {
        Shape::One(width) => integers(&[(width, 2)]),
        Shape::Equal(a, b) => integers(&[(a, 2), (b, 2)]),
        Shape::Product(a, b) => integers(&[(a, 2), (b, 3)]),
        Shape::TimeTable { period } => time_table(2, 3, period),
    })
}

/// The largest value the catalog's rules allow the Integer parameter `name` of `class_path`.
fn maximum(class_path: &str, name: &str) -> i64 {
    let entry = catalog()
        .iter()
        .find(|entry| entry.class_path == class_path)
        .unwrap_or_else(|| panic!("{class_path} is not a catalog class"));
    entry
        .param_rules
        .iter()
        .find_map(|rule| match rule {
            CatalogRule::IntegerLessOrEqualConstant { name: ruled, max } if *ruled == name => {
                Some(*max)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("{class_path} has no upper bound on {name}"))
}

/// The tables the coverage tests resolve a class at: widths 1, 2 and the maximum the catalog's
/// rules allow (for two bounded-product widths, each at the maximum with the other at 1), and
/// for a time table one, two and three output columns. `None` for a class with no row.
pub fn width_tables(class_path: &str) -> Option<Vec<Table>> {
    Some(match shape(class_path)? {
        Shape::One(width) => [1, 2, maximum(class_path, width)]
            .iter()
            .map(|n| integers(&[(width, *n)]))
            .collect(),
        Shape::Equal(a, b) => [1, 2, maximum(class_path, a).min(maximum(class_path, b))]
            .iter()
            .map(|n| integers(&[(a, *n), (b, *n)]))
            .collect(),
        Shape::Product(a, b) => vec![
            integers(&[(a, 1), (b, 1)]),
            integers(&[(a, 2), (b, 3)]),
            integers(&[(a, maximum(class_path, a)), (b, 1)]),
            integers(&[(a, 1), (b, maximum(class_path, b))]),
        ],
        Shape::TimeTable { period } => vec![
            time_table(2, 2, period),
            time_table(3, 3, period),
            time_table(2, 4, period),
        ],
    })
}

/// A table borrowed the way `oce_api::class_ports` takes it.
pub fn borrowed(table: &Table) -> Vec<(&str, Value)> {
    table
        .iter()
        .map(|(name, value)| (name.as_str(), value.clone()))
        .collect()
}
