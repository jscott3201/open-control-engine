//! The engine's naming contract, read in the direction the vocabulary needs: from a catalog
//! parameter name to the parameter it is, and from a resolved port name to the connector element
//! it is.
//!
//! The facade spells an array parameter once, as a template (`k_<i>`, `K_<row>_<col>`), and names
//! the ports of a vector connector `u[n]` `u1` … `un` and of a matrix connector `y[r, c]` `y1_1` …
//! `yr_c`, row-major (`oce_api::class_ports`, `docs/cdl-coverage.md`). The vocabulary lists each
//! array parameter and each connector once, under its CDL name.

use oce_api::{PARAM_COL_PLACEHOLDER, PARAM_INDEX_PLACEHOLDER, PARAM_ROW_PLACEHOLDER};

/// The CDL name of a catalog parameter: the name itself for a scalar (`nin`), and the part
/// before the first index placeholder, without its separating underscore, for an array template
/// (`k_<i>` is `k`, `K_<row>_<col>` is `K`). Pure; never panics.
#[must_use]
pub fn parameter_base_name(catalog_name: &str) -> &str {
    let cut = [
        PARAM_INDEX_PLACEHOLDER,
        PARAM_ROW_PLACEHOLDER,
        PARAM_COL_PLACEHOLDER,
    ]
    .iter()
    .filter_map(|placeholder| catalog_name.find(placeholder))
    .min();
    match cut {
        None => catalog_name,
        Some(at) => {
            let prefix = &catalog_name[..at];
            prefix.strip_suffix('_').unwrap_or(prefix)
        }
    }
}

/// Which element of a connector a resolved port is. Indices are one-based, as the port names
/// spell them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortElement {
    /// The port is the scalar connector itself.
    Scalar,
    /// Element `i` of a vector connector: port `u3` of connector `u` is `Vector(3)`.
    Vector(usize),
    /// Element `[row, col]` of a matrix connector: port `y2_1` of connector `y`.
    Matrix {
        /// One-based row.
        row: usize,
        /// One-based column.
        col: usize,
    },
}

/// A one-based index as a port name spells it: decimal digits, no sign, no leading zero.
fn index(digits: &str) -> Option<usize> {
    if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// The element of the connector `connector_name` with `dims` dimensions that the resolved port
/// `port_name` is, or `None` when the port is not one of its elements.
///
/// A scalar connector (`dims == 0`) matches its own name only; a vector connector matches its
/// name followed by a one-based index (`u3`); a matrix connector matches its name followed by
/// `row_col` (`y2_1`). Any other `dims` matches nothing. Pure; never panics.
#[must_use]
pub fn port_element(connector_name: &str, dims: u8, port_name: &str) -> Option<PortElement> {
    let rest = port_name.strip_prefix(connector_name)?;
    match dims {
        0 => rest.is_empty().then_some(PortElement::Scalar),
        1 => index(rest).map(PortElement::Vector),
        2 => {
            let (row, col) = rest.split_once('_')?;
            Some(PortElement::Matrix {
                row: index(row)?,
                col: index(col)?,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{PortElement, parameter_base_name, port_element};

    #[test]
    fn a_scalar_parameter_is_its_own_base_name_and_a_template_loses_its_placeholders() {
        assert_eq!(parameter_base_name("nin"), "nin");
        assert_eq!(parameter_base_name("y_start"), "y_start");
        assert_eq!(parameter_base_name("k_<i>"), "k");
        assert_eq!(parameter_base_name("K_<row>_<col>"), "K");
        assert_eq!(parameter_base_name("table_<row>_<col>"), "table");
        assert_eq!(parameter_base_name(""), "");
        // Only the separator before the placeholder goes; an underscore of the name stays.
        assert_eq!(parameter_base_name("x__<i>"), "x_");
        assert_eq!(parameter_base_name("<i>"), "");
    }

    #[test]
    fn a_scalar_connector_matches_only_its_own_name() {
        assert_eq!(port_element("u", 0, "u"), Some(PortElement::Scalar));
        assert_eq!(port_element("u1", 0, "u1"), Some(PortElement::Scalar));
        assert_eq!(port_element("u", 0, "u1"), None);
        assert_eq!(port_element("u", 0, "y"), None);
        assert_eq!(port_element("index", 0, "inde"), None);
    }

    #[test]
    fn a_vector_connector_matches_a_one_based_decimal_index() {
        assert_eq!(port_element("u", 1, "u1"), Some(PortElement::Vector(1)));
        assert_eq!(port_element("u", 1, "u64"), Some(PortElement::Vector(64)));
        assert_eq!(
            port_element("yIdx", 1, "yIdx2"),
            Some(PortElement::Vector(2))
        );
        for port in ["u", "u0", "u01", "u-1", "u+1", "u1_1", "u1x", "y1", "u 1"] {
            assert_eq!(port_element("u", 1, port), None, "{port}");
        }
        // The connector `y` does not claim the ports of `yIdx`.
        assert_eq!(port_element("y", 1, "yIdx1"), None);
    }

    #[test]
    fn a_matrix_connector_matches_row_then_column() {
        assert_eq!(
            port_element("y", 2, "y2_1"),
            Some(PortElement::Matrix { row: 2, col: 1 })
        );
        for port in ["y", "y1", "y1_", "y_1", "y0_1", "y1_0", "y1_1_1", "y1_01"] {
            assert_eq!(port_element("y", 2, port), None, "{port}");
        }
    }

    #[test]
    fn an_index_past_usize_and_an_unknown_rank_match_nothing() {
        let huge = format!("u{}", "9".repeat(40));
        assert_eq!(port_element("u", 1, &huge), None);
        assert_eq!(port_element("u", 3, "u1"), None);
        assert_eq!(port_element("u", 255, "u"), None);
    }
}
