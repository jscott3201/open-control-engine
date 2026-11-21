//! Vector elements a block omits and the class defaults (issue #726): an `ExtractSignal`'s
//! omitted `extract_i` is `i`, and a `VectorFilter`'s omitted `msk_i` is `true`. One parameter,
//! `nout` or `nin`, sets up to 2^20 of them, so they are counted from the elements present and
//! never walked: one finding for the omitted selectors out of range, and a true count by
//! arithmetic.

use super::common::*;
use crate::validate_block_parameters;

const EXTRACT: &str = "CDL.Routing.RealExtractSignal";
const FILTER: &str = "CDL.Routing.RealVectorFilter";

fn integers(values: &[(&str, i64)]) -> ParamTable {
    ParamTable {
        values: values
            .iter()
            .map(|(name, value)| (Arc::from(*name), Value::Integer(*value)))
            .collect(),
    }
}

fn messages(class: &str, params: &ParamTable) -> Vec<String> {
    let diagnostics = match validate_block_parameters(class, params) {
        Ok(warnings) => warnings,
        Err(error) => error.diagnostics,
    };
    diagnostics.into_iter().map(|d| d.message).collect()
}

#[test]
fn one_omitted_selector_out_of_range_names_that_element_as_it_always_has() {
    assert_eq!(
        messages(EXTRACT, &integers(&[("nin", 2), ("nout", 3)])),
        [
            "parameter `extract_3` on block `CDL.Routing.RealExtractSignal` must be in range 1..=2; \
          got 3"
        ]
    );
}

#[test]
fn several_omitted_selectors_out_of_range_are_one_finding_with_the_count_and_first_four() {
    assert_eq!(
        messages(
            EXTRACT,
            &integers(&[("nin", 1), ("nout", 6), ("extract_1", 1)])
        ),
        [
            "parameter `extract` on block `CDL.Routing.RealExtractSignal` omits 5 elements whose \
             default, the element's index, is outside range 1..=1: extract_2, extract_3, \
             extract_4, extract_5 and 1 more"
        ]
    );
    // An element present in the gap is judged for itself and not counted with the omitted ones.
    assert_eq!(
        messages(
            EXTRACT,
            &integers(&[("nin", 1), ("nout", 4), ("extract_3", 9)])
        ),
        [
            "parameter `extract_3` on block `CDL.Routing.RealExtractSignal` must be in range \
             1..=1; got 9",
            "parameter `extract` on block `CDL.Routing.RealExtractSignal` omits 2 elements whose \
             default, the element's index, is outside range 1..=1: extract_2, extract_4",
        ]
    );
    // Every element present: nothing omitted.
    let present: Vec<(String, i64)> = (1..=4).map(|i| (format!("extract_{i}"), 1)).collect();
    let mut params = integers(&[("nin", 1), ("nout", 4)]);
    params.values.extend(
        present
            .iter()
            .map(|(name, value)| (Arc::from(name.as_str()), Value::Integer(*value))),
    );
    assert!(messages(EXTRACT, &params).is_empty());
}

#[test]
fn an_empty_range_puts_every_omitted_selector_outside_it() {
    assert_eq!(
        messages(EXTRACT, &integers(&[("nin", 0), ("nout", 2)])),
        [
            "parameter `extract` on block `CDL.Routing.RealExtractSignal` omits 2 elements whose \
             default, the element's index, is outside range 1..=0: extract_1, extract_2"
        ]
    );
}

#[test]
fn the_widest_vector_of_omitted_selectors_is_one_finding() {
    assert_eq!(
        messages(EXTRACT, &integers(&[("nin", 1), ("nout", 1 << 20)])),
        [
            "parameter `extract` on block `CDL.Routing.RealExtractSignal` omits 1048575 elements \
             whose default, the element's index, is outside range 1..=1: extract_2, extract_3, \
             extract_4, extract_5 and 1048571 more"
        ]
    );
}

#[test]
fn an_omitted_mask_member_counts_as_true_without_walking_the_vector() {
    let filter = |nin: i64, nout: i64, members: &[(&str, bool)]| {
        let mut params = integers(&[("nin", nin), ("nout", nout)]);
        params.values.extend(
            members
                .iter()
                .map(|(name, value)| (Arc::from(*name), Value::Boolean(*value))),
        );
        messages(FILTER, &params)
    };
    let wide = 1 << 20;
    assert!(
        filter(wide, wide, &[]).is_empty(),
        "every member defaults to true"
    );
    assert!(filter(wide, wide - 1, &[("msk_7", false)]).is_empty());
    // A member bound twice counts its first binding: false, so the count is nout's 2.
    assert_eq!(
        filter(3, 2, &[("msk_2", false), ("msk_2", true)]),
        ["block `CDL.Routing.RealVectorFilter` carries duplicate parameter name `msk_2`"]
    );
    assert_eq!(
        filter(wide, 5, &[("msk_7", false)]),
        [
            "Boolean mask `msk` on block `CDL.Routing.RealVectorFilter` has true count 1048575, \
          expected `nout` = 5"
        ]
    );
    // A member past `nin` is not counted (the element rule refuses it).
    assert_eq!(
        filter(2, 2, &[("msk_3", false)]),
        [
            "parameter `msk_3` on block `CDL.Routing.RealVectorFilter` is outside resolved array \
          length 2"
        ]
    );
}
