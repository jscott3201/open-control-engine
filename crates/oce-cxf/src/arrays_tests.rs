//! The element names of an array parameter and what they cost the document's budgets (issue
//! #710): the names in row-major order, the bytes of their paths counted without building them,
//! and the per-document budgets at their edges, the minted-element budget (issue #756) among
//! them.

use super::{
    ArrayElementBudget, ElementNames, MAX_DOCUMENT_ARRAY_ELEMENTS, MAX_DOCUMENT_ARRAY_PATH_BYTES,
    MAX_DOCUMENT_MINTED_ARRAY_ELEMENTS, array_element_count, array_element_path_bytes,
    index_digits, minted_elements,
};
use crate::dto::CxfValue;

/// The path budget, restated: a change to the constant fails here until it is reviewed.
const PATH_BUDGET: usize = 64 * 1024 * 1024;

/// The instance IRI of the facade tests' constant, 30 bytes.
const OWNER: &str = "http://example.org#MinLoop.con";

/// The first `count` names of `base` over `dims`, read one at a time.
fn names(base: &str, dims: &[usize], count: usize) -> Vec<String> {
    let mut names = ElementNames::new(base, dims);
    (0..count).map(|_| names.next_name().to_owned()).collect()
}

/// Every name of `base` over `dims` by a second route: the Cartesian product of the 1-based
/// indices, built recursively with the first index slowest.
fn product(base: &str, dims: &[usize]) -> Vec<String> {
    match dims.split_first() {
        None => vec![base.to_owned()],
        Some((&n, rest)) => (1..=n)
            .flat_map(|i| product(&format!("{base}_{i}"), rest))
            .collect(),
    }
}

#[test]
fn the_path_budget_is_the_element_budget_at_sixty_four_bytes_an_element() {
    assert_eq!(MAX_DOCUMENT_ARRAY_PATH_BYTES, PATH_BUDGET);
    assert_eq!(
        MAX_DOCUMENT_ARRAY_PATH_BYTES,
        64 * MAX_DOCUMENT_ARRAY_ELEMENTS
    );
}

/// The largest one-dimensional array whose paths fit the budget, when the instance IRI and the
/// base name take `prefix` bytes together, found by bisection over the counted bytes.
fn largest_admitted(prefix: usize) -> usize {
    let fits = |n: usize| array_element_path_bytes(prefix, 0, &[n], n) <= PATH_BUDGET as u128;
    let (mut low, mut high) = (0, MAX_DOCUMENT_ARRAY_ELEMENTS);
    while low < high {
        let middle = (low + high).div_ceil(2);
        if fits(middle) {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    low
}

#[test]
fn what_the_path_budget_admits_is_what_its_documentation_states() {
    // `k[1048576]`: its `.` and indices take 8,326,080 bytes, so the instance IRI and the base
    // name may take 56 bytes together, and not 57. The figures beside it in the documentation.
    let dims = [MAX_DOCUMENT_ARRAY_ELEMENTS];
    let count = array_element_count(&dims).expect("bounded");
    assert_eq!(index_digits(count) + 2 * count as u128, 8_326_080);
    assert_eq!(OWNER.len(), 30);
    assert_eq!(array_element_path_bytes(30, 26, &dims, count), 67_046_336);
    assert!(array_element_path_bytes(30, 26, &dims, count) <= PATH_BUDGET as u128);
    assert!(array_element_path_bytes(30, 27, &dims, count) > PATH_BUDGET as u128);
    for (prefix, elements) in [
        (56, 1 << 20),
        (57, 1_033_635),
        (64, 933_610),
        (128, 494_264),
    ] {
        assert_eq!(largest_admitted(prefix), elements, "{prefix} bytes");
    }
    // An array of 65,536, the reviewer's document: at most 1,017 bytes of the two.
    let wide = [65_536];
    assert!(array_element_path_bytes(1_017, 0, &wide, 65_536) <= PATH_BUDGET as u128);
    assert!(array_element_path_bytes(1_018, 0, &wide, 65_536) > PATH_BUDGET as u128);
}

#[test]
fn element_names_are_row_major_with_the_last_index_fastest() {
    assert_eq!(names("k", &[2], 2), ["k_1", "k_2"]);
    assert_eq!(names("B", &[2, 2], 4), ["B_1_1", "B_1_2", "B_2_1", "B_2_2"]);
    assert_eq!(
        names("T", &[2, 1, 3], 6),
        [
            "T_1_1_1", "T_1_1_2", "T_1_1_3", "T_2_1_1", "T_2_1_2", "T_2_1_3"
        ]
    );
    // An index gains a digit without disturbing the base or the other indices.
    assert_eq!(names("k", &[10], 10)[8..], ["k_9", "k_10"]);
    assert_eq!(
        names("K", &[1, 12], 12)[9..],
        ["K_1_10", "K_1_11", "K_1_12"]
    );
}

#[test]
fn element_names_match_the_cartesian_product_of_their_indices() {
    for dims in [
        &[1][..],
        &[7],
        &[3, 4],
        &[4, 3],
        &[2, 3, 4],
        &[1, 1, 1],
        &[11, 1, 2],
        &[2, 10, 3],
    ] {
        let count = array_element_count(dims).expect("bounded");
        assert_eq!(names("x", dims, count), product("x", dims), "{dims:?}");
    }
}

#[test]
fn a_name_past_the_last_starts_again_from_the_first() {
    assert_eq!(names("k", &[2, 2], 6)[4..], ["k_1_1", "k_1_2"]);
}

#[test]
fn a_multibyte_base_is_kept_whole_in_every_name() {
    // The buffer is cut back to the base at a byte length: it must be a character boundary.
    assert_eq!(names("Δθ", &[2], 2), ["Δθ_1", "Δθ_2"]);
}

#[test]
fn index_digits_count_each_decade_at_its_width() {
    for (n, digits) in [
        (0, 0),
        (1, 1),
        (9, 9),
        (10, 11),
        (12, 15),
        (99, 189),
        (100, 192),
        (999, 2_889),
        (1_000, 2_893),
        (1_024, 2_989),
        // 9 + 180 + 2,700 + 36,000 + 450,000 + 5,400,000 + 48,577 × 7.
        (1_048_576, 6_228_928),
    ] {
        assert_eq!(index_digits(n), digits, "{n}");
    }
}

#[test]
fn index_digits_of_the_largest_usize_are_finite() {
    // Every full decade below 10^19, then the partial one from 10^19 to usize::MAX at twenty
    // digits. Nothing overflows.
    let max = usize::MAX as u128;
    let full: u128 = (1..=19u32)
        .map(|w| 9 * 10u128.pow(w - 1) * u128::from(w))
        .sum();
    let last = (max - 10u128.pow(19) + 1) * 20;
    assert_eq!(index_digits(usize::MAX), full + last);
}

#[test]
fn counted_path_bytes_equal_the_bytes_of_the_paths_built() {
    // The oracle builds every name, spells each path as the facade does, `<instance>.<name>`, and
    // adds their lengths: digit-count edges, several ranks, multibyte and empty instances and
    // bases, and the largest one-dimensional array the per-parameter cap admits.
    for (owner, base, dims) in [
        (OWNER, "k", &[1][..]),
        (OWNER, "k", &[9]),
        (OWNER, "k", &[10]),
        (OWNER, "k", &[99]),
        (OWNER, "k", &[100]),
        (OWNER, "k", &[1_000]),
        ("urn:ahu.supFan", "VAdjPopBreZon_flow", &[12, 10]),
        (OWNER, "B", &[10, 10, 10]),
        (OWNER, "T", &[3, 1, 101]),
        ("http://example.org#Δ.θ", "Δθ", &[11]),
        ("", "", &[5]),
        (OWNER, "p", &[1 << 20]),
    ] {
        let count = array_element_count(dims).expect("bounded");
        let mut names = ElementNames::new(base, dims);
        let built: usize = (0..count)
            .map(|_| format!("{owner}.{}", names.next_name()).len())
            .sum();
        assert_eq!(
            array_element_path_bytes(owner.len(), base.len(), dims, count),
            built as u128,
            "{owner:?} {base:?} over {dims:?}"
        );
    }
}

#[test]
fn an_empty_array_has_no_path_bytes() {
    for dims in [&[0][..], &[3, 0], &[0, 5, 2]] {
        let count = array_element_count(dims).expect("bounded");
        assert_eq!(count, 0);
        assert_eq!(
            array_element_path_bytes(1 << 20, 1 << 20, dims, count),
            0,
            "{dims:?}"
        );
    }
}

#[test]
fn counted_path_bytes_saturate_rather_than_wrap() {
    assert_eq!(
        array_element_path_bytes(usize::MAX, usize::MAX, &[usize::MAX], usize::MAX),
        u128::MAX
    );
    // Within the per-parameter cap an instance IRI and a name of any length are counted exactly.
    // Each path is the IRI, `.`, the base and `_`, and then its index.
    let count = 1 << 20;
    let each = usize::MAX as u128 + 1 + usize::MAX as u128 + 1;
    assert_eq!(
        array_element_path_bytes(usize::MAX, usize::MAX, &[count], count),
        count as u128 * each + index_digits(count)
    );
}

/// What the path budget's refusal says, stated by hand.
fn path_refusal(count: usize, bytes: u128, used: usize) -> String {
    format!(
        "array parameter's {count} element path(s) (instance IRI, `.`, element name) need \
         {bytes} byte(s) but the element paths of the document's array parameters already use \
         {used} of the 67108864-byte per-document budget"
    )
}

#[test]
fn the_path_budget_admits_exactly_its_limit_and_refuses_one_byte_more() {
    let mut budget = ArrayElementBudget::per_document();
    budget.take(3, PATH_BUDGET as u128 - 1, 0).expect("within");
    budget.take(1, 1, 0).expect("the last byte fits");
    // Nothing is left: one more byte refuses.
    assert_eq!(budget.take(1, 1, 0), Err(path_refusal(1, 1, PATH_BUDGET)));
    // An empty array needs nothing, so it still fits.
    budget
        .take(0, 0, 0)
        .expect("an empty array fits an exhausted budget");

    let mut fresh = ArrayElementBudget::per_document();
    assert_eq!(
        fresh.take(7, PATH_BUDGET as u128 + 1, 0),
        Err(path_refusal(7, PATH_BUDGET as u128 + 1, 0))
    );
}

#[test]
fn a_refusal_for_path_bytes_reserves_no_element_and_no_byte() {
    let mut budget = ArrayElementBudget::per_document();
    budget.take(10, 100, 0).expect("within");
    assert_eq!(
        budget.take(5, PATH_BUDGET as u128, 0),
        Err(path_refusal(5, PATH_BUDGET as u128, 100))
    );
    // The whole of what was left is still there to take.
    budget
        .take(
            MAX_DOCUMENT_ARRAY_ELEMENTS - 10,
            PATH_BUDGET as u128 - 100,
            0,
        )
        .expect("the refused parameter drew nothing");
}

#[test]
fn a_refusal_for_elements_reserves_no_byte_and_is_reported_first() {
    let mut budget = ArrayElementBudget::per_document();
    // Over both budgets: the element budget, checked first, is the one named.
    assert_eq!(
        budget.take(MAX_DOCUMENT_ARRAY_ELEMENTS + 1, u128::MAX, 0),
        Err(format!(
            "array parameter needs {} element(s) but the document's array parameters already \
             use 0 of the 1048576-element per-document budget",
            MAX_DOCUMENT_ARRAY_ELEMENTS + 1
        ))
    );
    budget
        .take(MAX_DOCUMENT_ARRAY_ELEMENTS, PATH_BUDGET as u128, 0)
        .expect("the refused parameter drew nothing");
}

#[test]
fn a_byte_count_past_usize_is_refused_and_stated_whole() {
    let mut budget = ArrayElementBudget::per_document();
    let bytes = usize::MAX as u128 * 4;
    assert_eq!(budget.take(2, bytes, 0), Err(path_refusal(2, bytes, 0)));
}

/// The minted-element refusal of `minted` elements when `used` are already minted, stated by
/// hand.
fn minted_refusal(minted: usize, used: usize) -> String {
    format!(
        "array parameter mints {minted} element(s) from a value it writes once (a broadcast or \
         an array expression) but the document's array parameters already mint {used} of the \
         65536 minted elements a document may hold"
    )
}

#[test]
fn the_minted_budget_is_sixty_five_thousand_five_hundred_thirty_six() {
    assert_eq!(MAX_DOCUMENT_MINTED_ARRAY_ELEMENTS, 65_536);
}

#[test]
fn the_minted_budget_admits_exactly_its_limit_and_refuses_one_element_more() {
    let mut budget = ArrayElementBudget::per_document();
    budget.take(65_535, 0, 65_535).expect("within");
    budget.take(1, 0, 1).expect("the last minted element fits");
    assert_eq!(budget.take(2, 0, 2), Err(minted_refusal(2, 65_536)));
    // Elements written out draw nothing from it, so they still fit.
    budget
        .take(1_000, 0, 0)
        .expect("written-out elements fit an exhausted minted budget");
    let mut fresh = ArrayElementBudget::per_document();
    assert_eq!(
        fresh.take(65_537, 0, 65_537),
        Err(minted_refusal(65_537, 0))
    );
}

#[test]
fn a_minted_refusal_reserves_nothing_and_comes_after_the_other_budgets() {
    let mut budget = ArrayElementBudget::per_document();
    budget.take(100, 10, 100).expect("within");
    assert_eq!(
        budget.take(70_000, 10, 70_000),
        Err(minted_refusal(70_000, 100))
    );
    // Nothing of the refused parameter was reserved: the rest of each budget is still there.
    budget
        .take(
            MAX_DOCUMENT_ARRAY_ELEMENTS - 100,
            PATH_BUDGET as u128 - 10,
            65_436,
        )
        .expect("the refused parameter drew nothing");
    // Over the element budget and the minted one: the element budget is named, as before.
    let mut fresh = ArrayElementBudget::per_document();
    let refusal = fresh
        .take(
            MAX_DOCUMENT_ARRAY_ELEMENTS + 1,
            0,
            MAX_DOCUMENT_ARRAY_ELEMENTS + 1,
        )
        .expect_err("refused");
    assert!(refusal.starts_with("array parameter needs"), "{refusal}");
    // Over the path budget and the minted one: the path budget is named, as before.
    let refusal = fresh
        .take(70_000, PATH_BUDGET as u128 + 1, 70_000)
        .expect_err("refused");
    assert!(
        refusal.starts_with("array parameter's 70000 element path(s)"),
        "{refusal}"
    );
}

#[test]
fn minted_elements_also_count_against_the_element_budget() {
    let mut budget = ArrayElementBudget::per_document();
    budget.take(65_536, 0, 65_536).expect("within");
    budget
        .take(MAX_DOCUMENT_ARRAY_ELEMENTS - 65_536, 0, 0)
        .expect("the rest of the element budget");
    assert!(
        budget
            .take(1, 0, 0)
            .expect_err("refused")
            .starts_with("array parameter needs")
    );
}

#[test]
fn a_broadcast_and_an_array_expression_mint_every_element_and_a_written_list_none() {
    let one = CxfValue::List(vec![CxfValue::Float(1.5)]);
    let three = CxfValue::List(vec![CxfValue::Float(1.5); 3]);
    // A broadcast mints all its elements, on one dimension or many (the count is the product).
    assert_eq!(minted_elements(&one, 2), 2);
    assert_eq!(minted_elements(&one, 1_048_576), 1_048_576);
    // One written value on one element is that element written out, as a list of three on three.
    assert_eq!(minted_elements(&one, 1), 0);
    assert_eq!(minted_elements(&three, 3), 0);
    // A written-out list draws nothing however long: the rule reads only the list's length
    // against the count, so a list of the whole element budget is accounted as this one is.
    assert_eq!(minted_elements(&three, 1_048_576), 0);
    // An array expression mints every element, whatever it spells.
    for text in ["fill(1.5, 7)", "1:7", "{1, 2, 3, 4, 5, 6, 7}"] {
        assert_eq!(
            minted_elements(&CxfValue::Expr(text.to_owned()), 7),
            7,
            "{text}"
        );
    }
    // Any other value is refused before an element is made, and mints none.
    assert_eq!(minted_elements(&CxfValue::Float(1.5), 7), 0);
    assert_eq!(minted_elements(&CxfValue::List(Vec::new()), 0), 0);
}
