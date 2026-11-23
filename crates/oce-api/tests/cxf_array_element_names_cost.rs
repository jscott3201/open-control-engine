//! What an array parameter's element names cost a load, in requested bytes and never in elapsed
//! time (issue #710; `TESTING.md`: cost is asserted with deterministic measures).
//!
//! Every element of `k[n]` on an instance `c` is minted under a name of its own, `k_1` to `k_n`,
//! and the facade's parameter table keys each by its path, `c.k_1` to `c.k_n`, `c` being the
//! instance's full IRI. So an array's paths take `n` times the lengths of `c` and `k`, while the
//! document pays for them a few times and for `n` with a few digits. The reader built every name
//! of an array before it read the value and counted no byte of them, so a long name or a long
//! instance IRI over a large declared size asked for `n × (IRI + name)` bytes whatever the value
//! was: the issue's document, 136,486 bytes, peaked at 202 MB, a 1 KiB name over 262,144
//! elements, a document of 7,464 bytes, at 860 MB, and a 4 KiB instance name over 65,536
//! elements, 38,356 bytes, loaded at 854 MB. Now the paths of one document are held to 64 MiB,
//! counted before any name is built, and each name is built where its element is minted, once
//! the value's shape is known to fit.
//!
//! The budget is the reader's, and the arrays these documents add (`q`, a long name) are names
//! the constant's class does not declare, which the facade's load gate refuses (issue #748). So
//! a document at the budget's edge is read by the reader (`oce_cxf::import_cxf`, the facade's
//! first step) and measured there, and the facade is held to refusing it for the undeclared
//! names, never for the budget.
//!
//! Measured with `allocation-counter`, whose counters are local to the measuring thread. The
//! ceilings have headroom over what was measured (in brackets, with what the same document cost
//! before the change).

use allocation_counter::{AllocationInfo, measure};
use oce_api::{Engine, LoadedModel, OcError};
use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::ModelGraph;

const MODEL: &str = include_str!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");

/// The per-document budget of element-path bytes (`oce-cxf`'s `MAX_DOCUMENT_ARRAY_PATH_BYTES`),
/// restated so this test fails if the budget moves without review.
const PATH_BUDGET: usize = 64 * 1024 * 1024;

/// The local name of [`MODEL`]'s constant, the instance every document edits.
const CON: &str = "con";

/// The parameter node and reference of `k` on the instance `instance`.
fn k_node(instance: &str) -> String {
    format!(
        "{{ \"@id\": \"{}.k\",\n      \"S231:value\": {{ \"@value\": \"2.0\", \"@type\": \"http://www.w3.org/2001/XMLSchema#double\" }} }}",
        instance_iri(instance)
    )
}
fn k_reference(instance: &str) -> String {
    format!(
        "\"S231:hasParameter\": {{ \"@id\": \"{}.k\" }}",
        instance_iri(instance)
    )
}

/// The IRI of the constant when its local name is `instance`.
fn instance_iri(instance: &str) -> String {
    format!("http://example.org#MinLoop.{instance}")
}

/// The `@id` of a parameter `local` of the constant `instance`.
fn parameter(instance: &str, local: &str) -> String {
    format!("{}.{local}", instance_iri(instance))
}

/// [`MODEL`] with its constant named `instance`: its node, its references and its members'.
fn renamed(instance: &str) -> String {
    MODEL.replace(&instance_iri(CON), &instance_iri(instance))
}

/// A preserved one-dimensional array parameter node.
fn array_node(instance: &str, local: &str, size: &str, value: &str) -> String {
    format!(
        "{{ \"@id\": \"{}\", \"S231:isArray\": true, \"S231:numberDimensions\": 1, \
         \"S231:sizeOfDimensions\": \"{size}\", \"S231:value\": {value} }}",
        parameter(instance, local)
    )
}

/// The issue's document: [`MODEL`] with `MinLoop.con.k` renamed to `local` (node and reference)
/// and made an array of `size` with the value `value`.
fn renamed_k(local: &str, size: &str, value: &str) -> Vec<u8> {
    assert_eq!(MODEL.matches(&k_node(CON)).count(), 1);
    assert_eq!(MODEL.matches(&k_reference(CON)).count(), 1);
    MODEL
        .replacen(&k_node(CON), &array_node(CON, local, size, value), 1)
        .replacen(
            &k_reference(CON),
            &format!(
                "\"S231:hasParameter\": {{ \"@id\": \"{}\" }}",
                parameter(CON, local)
            ),
            1,
        )
        .into_bytes()
}

/// [`MODEL`] with its constant named `instance`, keeping `k` and declaring each of `arrays`
/// (local name, size, value) after it.
fn with_arrays(instance: &str, arrays: &[(&str, &str, &str)]) -> Vec<u8> {
    let model = renamed(instance);
    let (node, reference) = (k_node(instance), k_reference(instance));
    assert_eq!(model.matches(&node).count(), 1);
    assert_eq!(model.matches(&reference).count(), 1);
    let references: String = arrays
        .iter()
        .map(|(local, _, _)| format!(", {{ \"@id\": \"{}\" }}", parameter(instance, local)))
        .collect();
    let nodes: String = arrays
        .iter()
        .map(|(local, size, value)| format!(",\n    {}", array_node(instance, local, size, value)))
        .collect();
    model
        .replacen(
            &reference,
            &format!(
                "\"S231:hasParameter\": [ {{ \"@id\": \"{}\" }}{references} ]",
                parameter(instance, "k")
            ),
            1,
        )
        .replacen(&node, &format!("{node}{nodes}"), 1)
        .into_bytes()
}

/// One load of `bytes`, measured. The unedited model is loaded once first, so what a process
/// sets up on its first load is not counted.
fn load(bytes: &[u8]) -> (Result<LoadedModel, OcError>, AllocationInfo) {
    Engine::new()
        .load_cxf(MODEL.as_bytes())
        .expect("the model loads");
    let mut engine = Engine::new();
    let mut result = None;
    let info = measure(|| result = Some(engine.load_cxf(bytes)));
    (result.expect("the load ran").map(|(_, model)| model), info)
}

/// One read of `bytes` by the CXF reader alone (the facade's first step, with its options),
/// measured. The unedited model is read once first.
fn read(bytes: &[u8]) -> (Result<ModelGraph, CxfError>, AllocationInfo) {
    import_cxf(MODEL.as_bytes(), &ResolveOptions::default()).expect("the model reads");
    let mut result = None;
    let info = measure(|| {
        result = Some(import_cxf(bytes, &ResolveOptions::default()).map(|(model, _)| model));
    });
    (result.expect("the read ran"), info)
}

/// Hold the facade to refusing `bytes` at its load gate for parameter names the class does not
/// declare, and for nothing the reader's budget says.
fn assert_refused_for_undeclared_names(bytes: &[u8]) {
    let error = Engine::new()
        .load_cxf(bytes)
        .expect_err("the undeclared names are refused");
    let diagnostics: Vec<&Diagnostic> = error.all_diagnostics().collect();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == DiagCode::UnknownParameter),
        "{:?}",
        diagnostics.first()
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| !d.message.starts_with("array parameter")),
        "the reader's budget refused it"
    );
}

/// The diagnostics of a refused load of `bytes`, and what it cost.
fn refused(bytes: &[u8]) -> (Vec<Diagnostic>, AllocationInfo) {
    let (result, info) = load(bytes);
    let error = result.expect_err("the document is refused");
    (error.all_diagnostics().cloned().collect(), info)
}

/// The path budget's refusal of the array `local` on `instance`, stated by hand.
fn over_paths(instance: &str, local: &str, count: usize, bytes: usize, used: usize) -> Diagnostic {
    Diagnostic::error(
        DiagCode::ResourceLimitExceeded,
        format!(
            "array parameter's {count} element path(s) (instance IRI, `.`, element name) need \
             {bytes} byte(s) but the element paths of the document's array parameters already \
             use {used} of the 67108864-byte per-document budget"
        ),
    )
    .with_subject(parameter(instance, local))
}

/// The bytes of the paths `<instance IRI>.<base>_<i>` for `i` from 1 to `len` together, added up
/// path by path: what the facade's parameter table keys each element by.
fn path_bytes(instance: &str, base: &str, len: usize) -> usize {
    let prefix = instance_iri(instance).len() + 1 + base.len() + 1;
    (1..=len).map(|i| prefix + i.to_string().len()).sum()
}

fn bytes(count: usize) -> u64 {
    u64::try_from(count).expect("fits")
}

#[test]
fn a_long_name_over_a_large_size_is_refused_before_any_name_is_built() {
    // The issue's document (a 64 KiB name over 1,024 elements) and the review's (1 KiB over
    // 262,144). Each is refused with the one diagnostic of the path budget, and costs what reading
    // the document costs. [measured: peaks of 702,330 and 124,303 bytes; before, 202,112,449 and
    // 860,475,847, refused for the missing `k` once every name was built]
    for (local, size) in [("k".repeat(64 * 1024), 1_024), ("k".repeat(1_024), 262_144)] {
        let document = renamed_k(&local, &format!("({size})"), "[ 1.0 ]");
        let (diagnostics, info) = refused(&document);
        let need = path_bytes(CON, &local, size);
        assert_eq!(diagnostics, [over_paths(CON, &local, size, need, 0)]);
        assert!(
            info.bytes_max <= 48 * bytes(document.len()),
            "{} bytes held for a document of {}",
            info.bytes_max,
            document.len()
        );
    }
}

#[test]
fn a_long_instance_iri_over_a_large_size_is_refused_before_any_name_is_built() {
    // The constant renamed to 4 KiB with `q[65536]`, and to 64 KiB with `q[4096]`: the name `q`
    // is one byte, and the instance IRI is what every element's path repeats. [measured: peaks of
    // 260,225 and 2,410,622 bytes for documents of 38,356 and 529,875; before, both loaded, at
    // 853,768,967 and 808,935,086]
    for (instance, size) in [
        ("c".repeat(4 * 1024), 65_536),
        ("c".repeat(64 * 1024), 4_096),
    ] {
        let document = with_arrays(&instance, &[("q", &format!("({size})"), "[ 1.0 ]")]);
        let (diagnostics, info) = refused(&document);
        let need = path_bytes(&instance, "q", size);
        assert_eq!(diagnostics, [over_paths(&instance, "q", size, need, 0)]);
        assert!(
            info.bytes_max <= 48 * bytes(document.len()),
            "{} bytes held for a document of {}",
            info.bytes_max,
            document.len()
        );
    }
}

#[test]
fn the_declared_size_of_a_refused_array_costs_nothing() {
    // The same 64 KiB name over 1,024 and over 1,048,576 elements: the two refusals request the
    // same bytes but for the digits of the size. [measured: 1,000,902 bytes in total at 1,024
    // elements, 15 more at 1,048,576]
    let local = "k".repeat(64 * 1024);
    let (_, small) = refused(&renamed_k(&local, "(1024)", "[ 1.0 ]"));
    let (diagnostics, large) = refused(&renamed_k(&local, "(1048576)", "[ 1.0 ]"));
    let need = path_bytes(CON, &local, 1 << 20);
    assert_eq!(diagnostics, [over_paths(CON, &local, 1 << 20, need, 0)]);
    assert!(
        large.bytes_total.abs_diff(small.bytes_total) <= 64,
        "{} and {} bytes requested",
        small.bytes_total,
        large.bytes_total
    );
}

#[test]
fn a_value_refused_for_its_shape_builds_no_name() {
    // A 32 KiB name over 1,024 elements fits the budget; each value below is refused for its
    // shape, before any name is built. [measured: peaks of 410,561 to 411,036 bytes; before,
    // 67.5 MB each] An array expression mints its elements, whose paths under that name (33.5 MB)
    // the minted path budget refuses before the expression is read (issue #756), so its shape is
    // checked under a 7 KiB name, whose 1,024 paths fit.
    let long = "k".repeat(32 * 1024);
    let shorter = "k".repeat(7 * 1024);
    for (local, value, message) in [
        (
            &long,
            "[ 1.0, 2.0 ]",
            "array value list has 2 element(s) but the declared dimensions imply 1024",
        ),
        (
            &long,
            "1.0",
            "array parameter value must be a JSON list of element literals or an array \
             expression string",
        ),
        (
            &shorter,
            "\"{1, 2}\"",
            "array expression evaluated to 2 element(s) but the declared dimensions imply 1024",
        ),
    ] {
        let document = with_arrays(CON, &[(local, "(1024)", value)]);
        let (diagnostics, info) = refused(&document);
        assert_eq!(
            diagnostics,
            [Diagnostic::error(DiagCode::GroundingFailed, message)
                .with_subject(parameter(CON, local))],
            "{value}"
        );
        assert!(
            info.bytes_max <= 48 * bytes(document.len()),
            "{value}: {} bytes held for a document of {}",
            info.bytes_max,
            document.len()
        );
    }
    // The same expression under the 32 KiB name is refused by the minted path budget, before the
    // expression is read or any name is built.
    let document = with_arrays(CON, &[(&long, "(1024)", "\"{1, 2}\"")]);
    let (diagnostics, info) = refused(&document);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, DiagCode::ResourceLimitExceeded);
    assert!(
        diagnostics[0]
            .message
            .starts_with("array parameter's 1024 minted element path(s)"),
        "{diagnostics:?}"
    );
    assert!(
        info.bytes_max <= 48 * bytes(document.len()),
        "{}",
        info.bytes_max
    );
}

/// On the constant `instance`, an array `base[len]` written out (its long paths are not minted, so
/// the 8 MiB of minted paths does not bind first; issue #756) and a one-element array whose path
/// takes the rest of the budget to the byte: the reader reads the document with every name in order, and
/// holds at most `copies` times the budget at its peak (the facade then refuses the names, which
/// the constant's class does not declare). One byte more on the second name refuses it, at
/// no more than three times the paths of the first array, whose names were minted before the
/// refusal.
fn assert_budget_edge(instance: &str, base: &str, len: usize, copies: u64) {
    let used = path_bytes(instance, base, len);
    // The last path is the IRI, `.`, its name and `_1`.
    let rest = PATH_BUDGET - used - (instance_iri(instance).len() + 1 + 2);
    let (filler, over) = ("f".repeat(rest), "f".repeat(rest + 1));
    let size = format!("({len})");
    let written = format!("[ {} ]", vec!["1.0"; len].join(", "));

    let document = with_arrays(
        instance,
        &[(base, &size, &written), (&filler, "(1)", "[ 2.0 ]")],
    );
    let (result, loaded) = read(&document);
    let model = result.expect("paths that fill the budget exactly are read");
    let block = model
        .blocks
        .iter()
        .find(|block| block.instance_iri.as_deref() == Some(instance_iri(instance).as_str()))
        .expect("the constant");
    let names: Vec<&str> = block
        .params
        .values
        .iter()
        .map(|(name, _)| name.as_ref())
        .collect();
    let mut expected = vec!["k".to_owned()];
    expected.extend((1..=len).map(|i| format!("{base}_{i}")));
    expected.push(format!("{filler}_1"));
    assert_eq!(names, expected);
    let paths: usize = names[1..]
        .iter()
        .map(|name| instance_iri(instance).len() + 1 + name.len())
        .sum();
    assert_eq!(paths, PATH_BUDGET);
    assert!(
        loaded.bytes_max <= copies * bytes(PATH_BUDGET),
        "{} bytes held for {PATH_BUDGET} bytes of paths",
        loaded.bytes_max
    );
    drop(model);
    assert_refused_for_undeclared_names(&document);

    let document = with_arrays(
        instance,
        &[(base, &size, &written), (&over, "(1)", "[ 2.0 ]")],
    );
    let (diagnostics, refusal) = refused(&document);
    let need = PATH_BUDGET - used + 1;
    assert_eq!(diagnostics, [over_paths(instance, &over, 1, need, used)]);
    assert!(
        refusal.bytes_max <= 3 * bytes(used),
        "{} bytes held for {used} bytes of paths minted before the refusal",
        refusal.bytes_max
    );
}

#[test]
fn paths_of_long_names_that_fill_the_budget_load_whole_and_one_byte_more_is_refused() {
    // A 65,000-byte name over 1,024 elements, under the constant's 30-byte IRI: the names are
    // nearly all of every path. The name is 32,500 two-byte characters, so a count of characters
    // instead of bytes would admit one byte more. [measured: a peak of 138,726,946 bytes loaded,
    // 2.07 times the budget, and 138,214,278 refused, the first array written out (issue #756);
    // recorded with it broadcast, 268,967,071 and 137,993,874; before issue #710, both loaded at
    // about 269 MB]
    assert_budget_edge(CON, &"π".repeat(32_500), 1_024, 5);
}

#[test]
fn paths_under_a_long_instance_iri_that_fill_the_budget_load_whole_and_one_byte_more_is_refused() {
    // The one-byte name `q` over 16,000 elements of a constant whose IRI is 4,123 bytes, 2,048 of
    // its characters two bytes each: the IRI is nearly all of every path, and a count of its
    // characters instead of its bytes would admit one byte more. The reader spells each
    // element's path once to check minted aliases, so the refusal holds the first array's paths
    // too. [measured: a peak of 147,661,442 bytes loaded, 2.20 times the budget, and 146,637,885
    // refused, the first array written out (issue #756); recorded with it broadcast, 208,207,573
    // and 142,237,593; before issue #710, both loaded]
    assert_budget_edge(&"é".repeat(2 * 1024), "q", 16_000, 5);
}
