//! Flattened array-parameter names and the catalog's `<i>`, `<row>`, `<col>` templates.
//!
//! A CDL array parameter reaches a block as one scalar entry per element. The CXF import mints
//! those entries from a preserved array (`k[2]`, `K[2,2]`) with a 1-based, row-major, underscore
//! spelling — `k_1`, `k_2`; `K_1_1`, `K_1_2`, `K_2_1`, `K_2_2`, last index fastest — and every
//! constructor and parameter rule in this crate reads that spelling. The catalog's parameter
//! defaults name such a parameter once, as a template: `k_<i>`, `K_<row>_<col>`.
//!
//! [`ParamNameTemplate`] parses a catalog name and expands it to the flattened element names, so
//! an authoring tool can list one `{name, value}` per element without re-implementing the
//! spelling. `oce_api`'s tests hold the expansion equal to the names the import itself mints.

use alloc::{format, string::String};

/// The placeholder for a vector element index in a catalog parameter name (`k_<i>`).
pub const PARAM_INDEX_PLACEHOLDER: &str = "<i>";
/// The placeholder for a matrix row index in a catalog parameter name (`K_<row>_<col>`).
pub const PARAM_ROW_PLACEHOLDER: &str = "<row>";
/// The placeholder for a matrix column index in a catalog parameter name (`K_<row>_<col>`).
pub const PARAM_COL_PLACEHOLDER: &str = "<col>";

/// A catalog parameter name, classified by how many flattened indices it carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamNameTemplate<'a> {
    /// A scalar parameter, named verbatim (`nin`, `k`).
    Scalar(&'a str),
    /// A vector parameter `base[n]`, written `base_<i>`.
    Vector {
        /// The array's base name (`k` for `k_<i>`).
        base: &'a str,
    },
    /// A matrix parameter `base[rows, cols]`, written `base_<row>_<col>`.
    Matrix {
        /// The array's base name (`K` for `K_<row>_<col>`).
        base: &'a str,
    },
}

impl<'a> ParamNameTemplate<'a> {
    /// Classify a catalog parameter name. A name that does not end in exactly `_<i>` or
    /// `_<row>_<col>` after a non-empty base is a scalar name, taken verbatim. Total.
    #[must_use]
    pub fn parse(name: &'a str) -> Self {
        let matrix_suffix = "_<row>_<col>";
        let vector_suffix = "_<i>";
        match (
            name.strip_suffix(matrix_suffix),
            name.strip_suffix(vector_suffix),
        ) {
            (Some(base), _) if !base.is_empty() => Self::Matrix { base },
            (_, Some(base)) if !base.is_empty() => Self::Vector { base },
            _ => Self::Scalar(name),
        }
    }

    /// The parameter's base name: the scalar name itself, or the array base.
    #[must_use]
    pub fn base(&self) -> &'a str {
        match *self {
            Self::Scalar(name) => name,
            Self::Vector { base } | Self::Matrix { base } => base,
        }
    }

    /// How many 1-based indices one element name carries: 0, 1 or 2.
    #[must_use]
    pub fn rank(&self) -> usize {
        match self {
            Self::Scalar(_) => 0,
            Self::Vector { .. } => 1,
            Self::Matrix { .. } => 2,
        }
    }

    /// The flattened name of the element at 1-based `indices` (`[]` for a scalar, `[i]` for a
    /// vector, `[row, col]` for a matrix). `None` when the index count is not [`Self::rank`] or an
    /// index is zero. Allocates the returned name.
    #[must_use]
    pub fn element_name(&self, indices: &[usize]) -> Option<String> {
        if indices.len() != self.rank() || indices.contains(&0) {
            return None;
        }
        Some(match *indices {
            [] => String::from(self.base()),
            [index] => format!("{}_{index}", self.base()),
            [row, col] => format!("{}_{row}_{col}", self.base()),
            _ => return None,
        })
    }
}
