//! What the reads of one instance's parameters share (issues #731, #736 and #711): the sibling
//! names built once, a value read more than once grounded once, the string budget at its edge, and
//! a seeded comparison of whole documents against a reference that shares nothing.

use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic, Severity};
use oce_expr::EvalResult;
use oce_model::{ModelGraph, Value};

use super::{
    DocumentGroundings, Groundings, MAX_DOCUMENT_STRING_PARAMETER_BYTES, StringParameterBudget,
    reads_counted_by,
};
use crate::dto::{CxfValue, Node};
use crate::ground::{ground_failure, ground_param_value};
use crate::resolve::local_name;
use crate::scope_chain::{IndexedEntries, ParamScope, ScopeFrame};
use crate::{CxfError, ResolveOptions, import_cxf};

const MODEL: &str = include_str!("../tests/fixtures/minimal_loop.jsonld");
const K_NODE: &str = "{ \"@id\": \"http://example.org#MinLoop.con.k\",\n      \"S231:value\": { \"@value\": \"2.0\", \"@type\": \"http://www.w3.org/2001/XMLSchema#double\" } }";
const K_REFERENCE: &str =
    "\"S231:hasParameter\": { \"@id\": \"http://example.org#MinLoop.con.k\" }";
const CON: &str = "http://example.org#MinLoop.con";

/// A parameter table as the reader builds it.
type Table = Vec<(Arc<str>, Value)>;

/// The groundings [`MODEL`]'s own parameters take, which the grounding count counts beside the
/// values a test adds: the constant's `k`, the delay's `y_start` and `samplePeriod`, the gain's
/// `k`.
const MODEL_GROUNDINGS: u64 = 4;

/// The string budget, restated: a change to the constant fails here until it is reviewed.
const STRING_BUDGET: usize = 8 * 1024 * 1024;

/// [`MODEL`] with its constant listing `k` and then `references`, and `nodes` declared after
/// `k`'s node.
fn document(references: &[String], nodes: &[String]) -> Vec<u8> {
    let listed: String = references
        .iter()
        .map(|id| format!(", {{ \"@id\": \"{id}\" }}"))
        .collect();
    let declared: String = nodes.iter().map(|node| format!(",\n    {node}")).collect();
    MODEL
        .replacen(
            K_REFERENCE,
            &format!("\"S231:hasParameter\": [ {{ \"@id\": \"{CON}.k\" }}{listed} ]"),
            1,
        )
        .replacen(K_NODE, &format!("{K_NODE}{declared}"), 1)
        .into_bytes()
}

/// A preserved array parameter node `id` of the dimensions `dims` with the JSON value `value`.
fn array(id: &str, dims: &[usize], value: &str) -> String {
    let size: Vec<String> = dims.iter().map(usize::to_string).collect();
    format!(
        "{{ \"@id\": \"{id}\", \"S231:isArray\": true, \"S231:numberDimensions\": {}, \
         \"S231:sizeOfDimensions\": \"({})\", \"S231:value\": {value} }}",
        dims.len(),
        size.join(", ")
    )
}

/// A scalar parameter node `id` with the JSON value `value`.
fn scalar(id: &str, value: &str) -> String {
    format!("{{ \"@id\": \"{id}\", \"S231:value\": {value} }}")
}

/// A string literal of `bytes` bytes, as the JSON value of a parameter.
fn quoted(bytes: usize) -> String {
    format!("\"\\\"{}\\\"\"", "s".repeat(bytes))
}

/// The constant's parameter table after resolving `bytes`.
fn constant_table(bytes: &[u8]) -> Table {
    let (model, _) = import_cxf(bytes, &ResolveOptions::default()).expect("resolves");
    table_of(&model)
}

fn table_of(model: &ModelGraph) -> Table {
    model
        .blocks
        .iter()
        .find(|block| block.instance_iri.as_deref() == Some(CON))
        .expect("the constant")
        .params
        .values
        .clone()
}

/// The diagnostics of resolving `bytes`, which must be refused.
fn refused(bytes: &[u8]) -> Vec<Diagnostic> {
    match import_cxf(bytes, &ResolveOptions::default()) {
        Err(CxfError::Validation(diagnostics)) => diagnostics,
        Err(other) => panic!("{other}"),
        Ok(_) => panic!("the document resolved"),
    }
}

/// `diagnostics` in one order, for comparing lists that two readers raise in their own order.
fn sorted(mut diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    diagnostics.sort_by(|a, b| {
        (a.code.as_str(), &a.subject, &a.message).cmp(&(b.code.as_str(), &b.subject, &b.message))
    });
    diagnostics
}

#[test]
fn the_string_budget_is_eight_mebibytes() {
    assert_eq!(MAX_DOCUMENT_STRING_PARAMETER_BYTES, STRING_BUDGET);
}

#[test]
fn the_string_budget_reserves_string_bytes_alone_and_refuses_without_reserving() {
    let mut budget = StringParameterBudget::per_document();
    for value in [
        Value::Real(1.0),
        Value::Integer(7),
        Value::Boolean(true),
        Value::String(Arc::from("")),
    ] {
        assert!(budget.take(&value).is_ok());
    }
    let most = Value::String(Arc::from("s".repeat(STRING_BUDGET - 1)));
    assert!(budget.take(&most).is_ok());
    let two = Value::String(Arc::from("ss"));
    let refusal = budget.take(&two).expect_err("one byte over");
    assert_eq!(
        refusal.diagnostic("urn:p.q", "urn:o"),
        Diagnostic::error(
            DiagCode::ResourceLimitExceeded,
            "string parameter value of 2 byte(s) on `urn:o` does not fit: the parameter tables \
             of the document already hold 8388607 of the 8388608 bytes of string values a \
             document may hold, each table entry counted",
        )
        .with_subject("urn:p.q".to_owned())
    );
    // The refusal reserved nothing: the last byte still fits, and then nothing does.
    assert!(budget.take(&Value::String(Arc::from("s"))).is_ok());
    assert!(budget.take(&Value::String(Arc::from("s"))).is_err());
    assert!(budget.take(&Value::Real(0.0)).is_ok());
}

#[test]
fn the_string_budget_counts_bytes_not_characters() {
    // Two-byte characters filling the budget by their bytes leave nothing; a count of characters
    // would leave half of it.
    let mut budget = StringParameterBudget::per_document();
    let wide = Value::String(Arc::from("é".repeat(STRING_BUDGET / 2)));
    assert!(budget.take(&wide).is_ok());
    assert!(budget.take(&Value::String(Arc::from("s"))).is_err());
}

/// The groundings of a document whose nodes no two owners list: nothing is shared.
fn unshared() -> DocumentGroundings {
    DocumentGroundings::of(std::iter::empty(), &std::collections::HashMap::new())
}

/// A parameter node `urn:q` with no value, standing for the node a value belongs to.
fn node() -> Node {
    serde_json::from_str("{ \"@id\": \"urn:q\" }").expect("a node")
}

#[test]
fn every_parameter_listed_more_than_once_is_found_once() {
    let mut document = unshared();
    let reads = Groundings::of(&["b", "a", "c", "a", "b", "a", "d"], &mut document);
    assert_eq!(reads.repeated, ["a", "b"]);
    for (iri, repeated) in [
        ("a", true),
        ("b", true),
        ("c", false),
        ("d", false),
        ("e", false),
    ] {
        assert_eq!(reads.is_repeated(iri), repeated, "{iri}");
    }
    assert!(Groundings::of(&[], &mut unshared()).repeated.is_empty());
    assert!(Groundings::of(&["a"], &mut unshared()).repeated.is_empty());
    assert_eq!(Groundings::of(&["a", "a"], &mut unshared()).repeated, ["a"]);
    assert!(
        Groundings::of(&["a", "b"], &mut unshared())
            .repeated
            .is_empty()
    );
}

#[test]
fn only_a_kept_read_is_grounded_once() {
    let mut document = unshared();
    let mut reads = Groundings::of(&[], &mut document);
    let (node, value) = (node(), CxfValue::Expr("\"s\"".to_owned()));
    let scope = ParamScope::empty();
    let (_, grounded, ()) = reads_counted_by(|| {
        for _ in 0..3 {
            assert!(reads.ground(&node, &value, &scope, false).is_ok());
        }
    });
    assert_eq!(
        grounded, 3,
        "a read that is not kept grounds the value again"
    );
    let (_, grounded, ()) = reads_counted_by(|| {
        for _ in 0..3 {
            assert!(reads.ground(&node, &value, &scope, true).is_ok());
        }
    });
    assert_eq!(grounded, 1);
}

#[test]
fn sibling_names_are_built_once_per_instance_however_many_arrays_ask() {
    // `P` one-element arrays on one leaf, beside `k`: the names of the `P + 1` parameters are
    // inserted once, where each array used to insert the `P` others again (`P × P`).
    for arrays in [1, 10, 100, 1_000] {
        let ids: Vec<String> = (0..arrays).map(|i| format!("{CON}.e{i}")).collect();
        let nodes: Vec<String> = ids.iter().map(|id| array(id, &[1], "[ 1.0 ]")).collect();
        let bytes = document(&ids, &nodes);
        let (inserted, _, table) = reads_counted_by(|| constant_table(&bytes));
        assert_eq!(inserted, arrays as u64 + 1, "{arrays} arrays");
        assert_eq!(table.len(), arrays + 1);
    }
    // The issue's document, empty arrays, mints no name and so builds no set at all; neither
    // does an instance with no array.
    let ids: Vec<String> = (0..1_000).map(|i| format!("{CON}.e{i}")).collect();
    let nodes: Vec<String> = ids.iter().map(|id| array(id, &[0], "[]")).collect();
    let (inserted, _, table) = reads_counted_by(|| constant_table(&document(&ids, &nodes)));
    assert_eq!((inserted, table.len()), (0, 1));
    let ids = [format!("{CON}.a"), format!("{CON}.b")];
    let nodes = [scalar(&ids[0], "1.0"), scalar(&ids[1], "2.0")];
    let (inserted, _, table) = reads_counted_by(|| constant_table(&document(&ids, &nodes)));
    assert_eq!((inserted, table.len()), (0, 3));
}

#[test]
fn a_minted_name_still_collides_with_every_sibling_and_never_with_its_own_array() {
    // The set now holds the array's own local name too. A sibling named like an element still
    // refuses that element, and the array's own name, decorated or not, refuses none.
    let q = format!("{CON}.q[3]");
    let ids = [q.clone(), format!("{CON}.q_2"), format!("{CON}.q")];
    let nodes = [
        array(&q, &[3], "[ 1.0 ]"),
        scalar(&ids[1], "5.0"),
        scalar(&ids[2], "6.0"),
    ];
    assert_eq!(
        refused(&document(&ids, &nodes)),
        [Diagnostic::error(
            DiagCode::ArrayFlattenCollision,
            "array element \"q_2\" collides with an existing sibling parameter of the same name",
        )
        .with_subject(q)]
    );
}

#[test]
fn a_sibling_listed_first_still_refuses_the_element_it_names() {
    // The set is built from every parameter the instance lists, the first among them.
    let q = format!("{CON}.q");
    let sibling = format!("{CON}.q_1");
    let bytes = String::from_utf8(document(
        std::slice::from_ref(&q),
        &[array(&q, &[2], "[ 1.0 ]"), scalar(&sibling, "5.0")],
    ))
    .expect("UTF-8")
    .replacen(
        &format!("[ {{ \"@id\": \"{CON}.k\" }}"),
        &format!("[ {{ \"@id\": \"{sibling}\" }}, {{ \"@id\": \"{CON}.k\" }}"),
        1,
    );
    assert_eq!(
        refused(bytes.as_bytes()),
        [Diagnostic::error(
            DiagCode::ArrayFlattenCollision,
            "array element \"q_1\" collides with an existing sibling parameter of the same name",
        )
        .with_subject(q)]
    );
}

#[test]
fn a_sibling_is_compared_by_its_whole_local_name_label_included() {
    // `q_1[3]` beside `q[2]`: the sibling's local name is `q_1[3]`, not `q_1`, so `q`'s first
    // element is minted as before and sits beside the array named `q_1` (whose own elements are
    // `q_1_1` to `q_1_3`).
    let (labelled, q) = (format!("{CON}.q_1[3]"), format!("{CON}.q[2]"));
    let ids = [labelled.clone(), q.clone()];
    let nodes = [
        array(&labelled, &[3], "[ 1.0 ]"),
        array(&q, &[2], "[ 2.0 ]"),
    ];
    let table = constant_table(&document(&ids, &nodes));
    let names: Vec<&str> = table.iter().map(|(name, _)| name.as_ref()).collect();
    assert_eq!(names, ["k", "q_1_1", "q_1_2", "q_1_3", "q_1", "q_2"]);
}

#[test]
fn a_positional_array_listed_again_grounds_each_of_its_values_once() {
    // Two values, listed three times: two groundings, kept for the later listings, and three
    // copies of the two entries in the table, as before.
    let q = format!("{CON}.q");
    let bytes = document(
        &vec![q.clone(); 3],
        &[array(&q, &[2], "[ \"\\\"a\\\"\", \"\\\"b\\\"\" ]")],
    );
    let (_, grounded, table) = reads_counted_by(|| constant_table(&bytes));
    assert_eq!(grounded, MODEL_GROUNDINGS + 2);
    let names: Vec<&str> = table.iter().map(|(name, _)| name.as_ref()).collect();
    assert_eq!(names, ["k", "q_1", "q_2", "q_1", "q_2", "q_1", "q_2"]);
}

#[test]
fn a_broadcast_is_charged_its_evaluation_once_and_the_explicit_spelling_once_an_element() {
    // The per-document evaluation budget bounds the work done. A broadcast's one value is now
    // evaluated once, so forty elements of `sum(1:1048576)` load, each the same Integer, where
    // each element used to be charged again and the ninth ran out. Written out forty times, the
    // same values are forty evaluations, and the document is still refused for its cost.
    let q = format!("{CON}.q");
    let sum = "\"sum(1:1048576)\"";
    let broadcast = document(
        std::slice::from_ref(&q),
        &[array(&q, &[40], &format!("[ {sum} ]"))],
    );
    let table = constant_table(&broadcast);
    assert_eq!(table.len(), 41);
    assert!(
        table[1..]
            .iter()
            .all(|(_, value)| value.bit_eq(&Value::Integer(549_756_338_176)))
    );
    let explicit = vec![sum; 40].join(", ");
    let explicit = document(
        std::slice::from_ref(&q),
        &[array(&q, &[40], &format!("[ {explicit} ]"))],
    );
    let diagnostics = refused(&explicit);
    assert!(!diagnostics.is_empty());
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code == DiagCode::GroundingFailed
                && d.subject.as_deref() == Some(q.as_str())
                && d.message.contains("too expensive")),
        "{diagnostics:?}"
    );
}

#[test]
fn a_broadcast_value_is_grounded_once_and_every_element_shares_it() {
    // A string over 1,024 elements: one grounding, one allocation shared by every entry.
    let q = format!("{CON}.q");
    let bytes = document(
        std::slice::from_ref(&q),
        &[array(&q, &[1_024], "[ \"\\\"shared\\\"\" ]")],
    );
    let (_, grounded, table) = reads_counted_by(|| constant_table(&bytes));
    assert_eq!(grounded, MODEL_GROUNDINGS + 1);
    let elements: Vec<&Arc<str>> = table[1..]
        .iter()
        .map(|(_, value)| match value {
            Value::String(text) => text,
            other => panic!("a string, not {other:?}"),
        })
        .collect();
    assert_eq!(elements.len(), 1_024);
    assert!(elements.iter().all(|text| text.as_ref() == "shared"));
    assert!(elements.iter().all(|text| Arc::ptr_eq(text, elements[0])));
    // A positional list grounds each of its values once, keeping none.
    let bytes = document(
        std::slice::from_ref(&q),
        &[array(&q, &[3], "[ 1.0, \"\\\"b\\\"\", 3 ]")],
    );
    let (_, grounded, table) = reads_counted_by(|| constant_table(&bytes));
    assert_eq!((grounded, table.len()), (MODEL_GROUNDINGS + 3, 4));
}

#[test]
fn a_broadcast_value_that_fails_is_reported_once_an_element() {
    // One grounding, and the failure raised for each of the four elements, as before.
    let q = format!("{CON}.q");
    let value =
        "[ { \"@value\": \"nope\", \"@type\": \"http://www.w3.org/2001/XMLSchema#double\" } ]";
    let bytes = document(std::slice::from_ref(&q), &[array(&q, &[4], value)]);
    let (_, grounded, diagnostics) = reads_counted_by(|| refused(&bytes));
    assert_eq!(grounded, MODEL_GROUNDINGS + 1);
    let one = Diagnostic::error(
        DiagCode::GroundingFailed,
        "typed literal \"nope\" (datatype http://www.w3.org/2001/XMLSchema#double) is not a \
         valid Real",
    )
    .with_subject(q);
    assert_eq!(diagnostics, vec![one; 4]);
}

#[test]
fn a_parameter_listed_again_is_grounded_once_and_still_listed_each_time() {
    // Ten listings of one string node: one grounding and ten entries sharing one allocation. As
    // before, the table holds an entry per listing; the validator refuses them later as
    // `duplicate-parameter`.
    let q = format!("{CON}.q");
    let ids = vec![q.clone(); 10];
    let bytes = document(&ids, &[scalar(&q, "\"\\\"text\\\"\"")]);
    let (_, grounded, table) = reads_counted_by(|| constant_table(&bytes));
    assert_eq!(grounded, MODEL_GROUNDINGS + 1);
    assert_eq!(table.len(), 11);
    let Value::String(first) = &table[1].1 else {
        panic!("a string")
    };
    assert!(table[1..].iter().all(|(name, value)| name.as_ref() == "q"
        && matches!(value, Value::String(text) if Arc::ptr_eq(text, first))));
}

#[test]
fn string_values_that_fill_the_budget_exactly_load_and_one_byte_more_is_refused() {
    // A broadcast of 1 MiB over 7 elements and a scalar of the rest, counted once per entry.
    let each = 1024 * 1024;
    let rest = STRING_BUDGET - 7 * each;
    let (q, r) = (format!("{CON}.q"), format!("{CON}.r"));
    let edge = |last: usize| {
        document(
            &[q.clone(), r.clone()],
            &[
                array(&q, &[7], &format!("[ {} ]", quoted(each))),
                scalar(&r, &quoted(last)),
            ],
        )
    };
    assert_eq!(constant_table(&edge(rest)).len(), 9);
    assert_eq!(
        refused(&edge(rest + 1)),
        [Diagnostic::error(
            DiagCode::ResourceLimitExceeded,
            format!(
                "string parameter value of {} byte(s) on `{CON}` does not fit: the parameter \
                 tables of the document already hold {} of the 8388608 bytes of string values a \
                 document may hold, each table entry counted",
                rest + 1,
                7 * each
            ),
        )
        .with_subject(r)]
    );
    // An element that does not fit refuses its array there: one refusal, nothing after it.
    let over = document(
        std::slice::from_ref(&q),
        &[array(&q, &[9], &format!("[ {} ]", quoted(each)))],
    );
    let diagnostics = refused(&over);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, DiagCode::ResourceLimitExceeded);
    assert_eq!(diagnostics[0].subject.as_deref(), Some(q.as_str()));
    assert!(
        diagnostics[0].message.starts_with(&format!(
            "string parameter value of {each} byte(s) on `{CON}`"
        )) && diagnostics[0]
            .message
            .contains(&format!("already hold {} of", 8 * each)),
        "{}",
        diagnostics[0].message
    );
}

/// A seeded xorshift64* generator: the same documents on every run and every host.
struct Seeded(u64);

impl Seeded {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// One generated parameter: its local name, the dimensions of an array, and its values as JSON.
struct Generated {
    local: &'static str,
    dims: Option<Vec<usize>>,
    values: Vec<String>,
}

/// A value of one of the shapes the reader grounds: numbers, strings, a literal that does not
/// parse, a non-finite one, and expressions that read other parameters (an earlier one, an
/// element name, or nothing bound).
fn generated_value(rng: &mut Seeded) -> String {
    match rng.below(9) {
        0 => format!("{}.5", rng.below(100)),
        1 => format!("{}", rng.below(100)),
        2 => format!("\"\\\"s{}\\\"\"", rng.below(4)),
        3 => "{ \"@value\": \"nope\", \"@type\": \"http://www.w3.org/2001/XMLSchema#double\" }"
            .to_owned(),
        4 => "{ \"@value\": \"NaN\", \"@type\": \"http://www.w3.org/2001/XMLSchema#double\" }"
            .to_owned(),
        5 => "\"a + 1\"".to_owned(),
        6 => "\"q_1 * 2\"".to_owned(),
        7 => "\"unbound\"".to_owned(),
        _ => "true".to_owned(),
    }
}

/// One leaf's generated parameters: scalars named `a`, `b`, `q_1`, `q_2` or `r_1`, and arrays
/// `q` and `r`, each local name at most once, so no node is listed twice and no two arrays mint
/// one name (the cases where grounding once may differ from grounding each read).
fn generated_parameters(rng: &mut Seeded) -> Vec<Generated> {
    let mut names = vec!["a", "b", "q_1", "q_2", "r_1", "q", "r"];
    let mut parameters = Vec::new();
    for _ in 0..rng.below(names.len() + 1) {
        let local = names.remove(rng.below(names.len()));
        let generated = if local == "q" || local == "r" {
            let dims: Vec<usize> = (0..1 + rng.below(2)).map(|_| rng.below(4)).collect();
            let count: usize = dims.iter().product();
            let len = match rng.below(4) {
                0 => 1,
                1 => count,
                _ => rng.below(5),
            };
            Generated {
                local,
                dims: Some(dims),
                values: (0..len).map(|_| generated_value(rng)).collect(),
            }
        } else {
            Generated {
                local,
                dims: None,
                values: vec![generated_value(rng)],
            }
        };
        parameters.push(generated);
    }
    parameters
}

/// What the reader produced for the constant: its table when the document resolved, and every
/// diagnostic, sorted.
fn read(parameters: &[Generated]) -> (Option<Table>, Vec<Diagnostic>) {
    let ids: Vec<String> = parameters
        .iter()
        .map(|p| format!("{CON}.{}", p.local))
        .collect();
    let nodes: Vec<String> = parameters
        .iter()
        .zip(&ids)
        .map(|(p, id)| match &p.dims {
            Some(dims) => array(id, dims, &format!("[ {} ]", p.values.join(", "))),
            None => scalar(id, &p.values[0]),
        })
        .collect();
    match import_cxf(&document(&ids, &nodes), &ResolveOptions::default()) {
        Ok((model, report)) => (Some(table_of(&model)), sorted(report.diagnostics)),
        Err(CxfError::Validation(diagnostics)) => (None, sorted(diagnostics)),
        Err(other) => panic!("{other}"),
    }
}

/// The same leaf read by a reference that shares nothing: every element grounded on its own read,
/// and the sibling names of each array rebuilt from every other parameter, as the reader did
/// before. Rows are `k` and then the generated parameters in order.
fn reference(parameters: &[Generated]) -> (Table, Vec<Diagnostic>) {
    let enclosing = ScopeFrame::root();
    let mut siblings = IndexedEntries::default();
    let mut table: Vec<(Arc<str>, Value)> = vec![(Arc::from("k"), Value::Real(2.0))];
    siblings.push(Arc::from("k"), EvalResult::Scalar(Value::Real(2.0)));
    let mut diagnostics = Vec::new();
    let ids: Vec<String> = std::iter::once(format!("{CON}.k"))
        .chain(parameters.iter().map(|p| format!("{CON}.{}", p.local)))
        .collect();
    let parse = |json: &str| -> CxfValue { serde_json::from_str(json).expect("a value") };
    for (p, id) in parameters.iter().zip(&ids[1..]) {
        let mint = |name: &str,
                    value: &CxfValue,
                    siblings: &mut IndexedEntries,
                    table: &mut Vec<(Arc<str>, Value)>,
                    diagnostics: &mut Vec<Diagnostic>| {
            let scope = ParamScope::with_enclosing(&enclosing, siblings);
            match ground_param_value(value, &scope) {
                Ok(v) => {
                    siblings.push(Arc::from(name), EvalResult::Scalar(v.clone()));
                    table.push((Arc::from(name), v));
                }
                Err(e) => diagnostics.push(ground_failure(&e, id, CON)),
            }
        };
        let Some(dims) = &p.dims else {
            let value = parse(&p.values[0]);
            mint(p.local, &value, &mut siblings, &mut table, &mut diagnostics);
            continue;
        };
        let count: usize = dims.iter().product();
        let m = p.values.len();
        if !(m == count || (m == 1 && count >= 1)) {
            diagnostics.push(
                Diagnostic::error(
                    DiagCode::GroundingFailed,
                    format!(
                        "array value list has {m} element(s) but the declared dimensions imply \
                         {count}"
                    ),
                )
                .with_subject(id.clone()),
            );
            continue;
        }
        let others: Vec<&str> = ids
            .iter()
            .filter(|other| *other != id)
            .map(|other| local_name(other))
            .collect();
        let names: Vec<String> = match dims[..] {
            [n] => (1..=n).map(|i| format!("{}_{i}", p.local)).collect(),
            [n, o] => (1..=n)
                .flat_map(|i| (1..=o).map(move |j| (i, j)))
                .map(|(i, j)| format!("{}_{i}_{j}", p.local))
                .collect(),
            _ => panic!("one or two dimensions are generated"),
        };
        for (k, name) in names.iter().enumerate() {
            if others.contains(&name.as_str()) {
                diagnostics.push(
                    Diagnostic::error(
                        DiagCode::ArrayFlattenCollision,
                        format!(
                            "array element {name:?} collides with an existing sibling parameter \
                             of the same name"
                        ),
                    )
                    .with_subject(id.clone()),
                );
                continue;
            }
            let value = parse(&p.values[if m == 1 { 0 } else { k }]);
            mint(name, &value, &mut siblings, &mut table, &mut diagnostics);
        }
    }
    (table, sorted(diagnostics))
}

#[test]
fn random_leaves_read_as_a_reference_that_shares_nothing_reads_them() {
    // 2,000 seeded leaves of scalars and arrays (broadcast, positional, of the wrong length, of
    // one or two dimensions, empty), with values that parse, fail, are not finite, read an
    // earlier parameter or an element name: the reader never panics, and its table and
    // diagnostics are the reference's.
    let mut rng = Seeded(0x0736_0731_0711_5EED);
    let mut loaded = 0;
    for case in 0..2_000 {
        let parameters = generated_parameters(&mut rng);
        let (table, diagnostics) = read(&parameters);
        let (expected_table, expected_diagnostics) = reference(&parameters);
        assert_eq!(diagnostics, expected_diagnostics, "case {case}");
        let failed = expected_diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error);
        let Some(table) = table else {
            assert!(failed, "case {case}");
            continue;
        };
        assert!(!failed, "case {case}");
        loaded += 1;
        assert_eq!(table.len(), expected_table.len(), "case {case}");
        for ((name, value), (expected_name, expected_value)) in table.iter().zip(&expected_table) {
            assert_eq!(name, expected_name, "case {case}");
            assert!(value.bit_eq(expected_value), "case {case}: {name}");
        }
    }
    // Both outcomes are exercised.
    assert!((200..1_800).contains(&loaded), "{loaded} of 2,000 loaded");
}
