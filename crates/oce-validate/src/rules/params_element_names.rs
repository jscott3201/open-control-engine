//! The canonical spelling of flattened array and matrix parameter element names (GitHub #470).

use oce_blocks::ParamNameTemplate;
use oce_diag::{DiagCode, Diagnostic};
use oce_model::BlockInstance;

use super::block_subject_of;

/// Refuse a bound parameter that spells an element of one of the class's array or matrix
/// parameters with a non-canonical index (GitHub #470).
///
/// The catalog names each array parameter by template (`k_<i>`, `table_<row>_<col>`). A bound
/// name `<base>_<a>[_<b>]` whose index fields are all optionally signed decimal integers, as many
/// as the template's rank, is an element spelling; each index must be plain decimal with no sign
/// and no leading zero, as `oce_api::param_element_name` writes it. Otherwise `TimeTable`, which parses
/// indices with `usize::parse`, would read `table_01_2` and `table_1_2` as one cell and keep the
/// later in table order, and the canonical content id, which sorts parameters by name, could not
/// tell the two orders apart. Names outside the declared extent are GitHub #467, not this check.
pub(super) fn check_element_name_spelling(blk: &BlockInstance, diags: &mut Vec<Diagnostic>) {
    let Some(parameters) = oce_blocks::class_parameters(&blk.class_iri) else {
        return;
    };
    let templates: Vec<ParamNameTemplate<'_>> = parameters
        .iter()
        .map(|parameter| ParamNameTemplate::parse(parameter.name))
        .filter(|template| template.rank() > 0)
        .collect();
    for (name, _) in &blk.params.values {
        let refused = templates.iter().any(|template| {
            let Some(suffix) = name
                .strip_prefix(template.base())
                .and_then(|rest| rest.strip_prefix('_'))
            else {
                return false;
            };
            let indices: Vec<&str> = suffix.split('_').collect();
            indices.len() == template.rank()
                && indices.iter().all(|index| is_signed_decimal(index))
                && !indices.iter().all(|index| is_canonical_index(index))
        });
        if refused {
            diags.push(
                Diagnostic::error(
                    DiagCode::NonCanonicalElementName,
                    format!(
                        "parameter `{name}` on block `{}` spells an array element with a \
                         non-canonical index; write decimal indices with no sign or leading \
                         zero",
                        blk.class_iri
                    ),
                )
                .with_subject(block_subject_of(blk)),
            );
        }
    }
}

/// `[+-]?[0-9]+`: any spelling an integer parser could read as an index.
fn is_signed_decimal(text: &str) -> bool {
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// `0|[1-9][0-9]*`: the one decimal spelling of an index value. Zero is a canonical spelling
/// (of an out-of-range index, which the class's own rules refuse where it matters).
fn is_canonical_index(text: &str) -> bool {
    (text == "0"
        || text
            .bytes()
            .next()
            .is_some_and(|first| (b'1'..=b'9').contains(&first)))
        && text.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
#[path = "params_element_names_tests.rs"]
mod tests;
