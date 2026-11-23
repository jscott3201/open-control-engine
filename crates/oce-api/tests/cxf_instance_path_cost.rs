//! What a block's instance IRI costs a load for each parameter and boundary-driven input, in
//! requested and held bytes and never in elapsed time (issue #807; `TESTING.md`: cost is asserted
//! with deterministic measures).
//!
//! The export writes each scalar parameter of a block under the block's IRI, `<IRI>.<name>`, and
//! each input a boundary drives as `<IRI>.in<position>`, whatever the document wrote. Two places
//! on the load spelled those identities in full:
//!
//! - **the reader**, checking that no boundary input is named like one of them, built every
//!   identity of every block into a set: about twice the IRI's length for each parameter, declared
//!   or not, and before the facade's load gate refuses an undeclared name. One leaf under an 8 KiB
//!   IRI listing 500 short parameter nodes, a 63 KB document, read at a 10.3 MB peak, and the
//!   facade's 8 MiB admission projected to about 80 GB. It now splits each boundary input once,
//!   at its last `.`, and looks its parts up.
//! - **the facade's parameter table** held every path spelled twice, as the row's path and as the
//!   index's key, three times the IRI's length a parameter once the first had grown. It now holds
//!   the IRI shared and each row's name, and spells a path only when `ParamTable::iter` hands it
//!   out.
//!
//! So a parameter costs the reader and the loaded engine a fixed size whatever its block's IRI.
//! What a block's IRI still costs is a fixed number of copies of it for the block and each of its
//! ports, which the document's own spellings bound (each IRI is written at least twice, against
//! the 8 MiB document or the 8 MiB `@context` expansion).
//!
//! Measured with `allocation-counter`, whose counters are local to the measuring thread. The
//! ceilings have headroom over what was measured (in brackets, with what the same document cost
//! before the change).

use allocation_counter::{AllocationInfo, measure};
use oce_api::Engine;
use oce_cxf::{ResolveOptions, import_cxf};

use crate::cxf_export_element_cost::pid_instances;
use crate::cxf_minted_array_cost::MODEL;

/// The minimal loop's root around one `Constant` leaf named by an IRI of `iri_bytes` bytes, which
/// lists its `k` and `count` parameter nodes its class does not declare, `urn:n.p<i>`, each a
/// short node of its own: the document grows by about 60 bytes a parameter, never by the IRI.
fn undeclared_parameters(iri_bytes: usize, count: usize) -> Vec<u8> {
    let leaf = format!("urn:{}", "c".repeat(iri_bytes - 4));
    let references: String = (0..count)
        .map(|i| format!(r#",{{"@id":"urn:n.p{i}"}}"#))
        .collect();
    let nodes: String = (0..count)
        .map(|i| format!(r#",{{"@id":"urn:n.p{i}","S231:value":1.0}}"#))
        .collect();
    format!(
        r#"{{"@context":{{"S231":"http://data.ashrae.org/S231#"}},"@graph":[
{{"@id":"urn:M","@type":"S231:Block","S231:containsBlock":{{"@id":"{leaf}"}},"S231:hasOutput":{{"@id":"urn:M.y"}}}},
{{"@id":"urn:M.y","@type":"S231:RealOutput","S231:isOfDataType":{{"@id":"S231:Real"}}}},
{{"@id":"{leaf}","@type":"http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant","S231:hasParameter":[{{"@id":"urn:k.k"}}{references}],"S231:hasOutput":{{"@id":"{leaf}.y"}}}},
{{"@id":"urn:k.k","S231:value":1.0}}{nodes},
{{"@id":"{leaf}.y","@type":"S231:RealOutput","S231:isOfDataType":{{"@id":"S231:Real"}},"S231:isConnectedTo":{{"@id":"urn:M.y"}}}}
]}}"#
    )
    .into_bytes()
}

/// The reader's cost of `document`, which it reads.
fn read(document: &[u8]) -> AllocationInfo {
    let mut result = None;
    let info = measure(|| result = Some(import_cxf(document, &ResolveOptions::default())));
    result.expect("ran").expect("the reader admits the names");
    info
}

/// Warm the reader's one-time allocations up, so the first measured read is not charged them.
fn warm_up() {
    import_cxf(MODEL.as_bytes(), &ResolveOptions::default()).expect("reads");
}

#[test]
fn undeclared_parameters_cost_the_reader_nothing_by_their_blocks_iri() {
    // One leaf listing 256 undeclared parameter nodes under IRIs of 2, 4 and 8 KiB: the peak
    // grows by the leaf's own copies of its IRI, not by a copy a parameter. [measured: 1,395,107 /
    // 1,425,827 / 1,487,267, 15 bytes an IRI byte; before, 2,431,150 at 2 KiB and 3.1 MB more at
    // 8 KiB]
    warm_up();
    let peaks: Vec<u64> = [2_048, 4_096, 8_192]
        .into_iter()
        .map(|iri| read(&undeclared_parameters(iri, 256)).bytes_max)
        .collect();
    assert!(
        peaks[2] - peaks[0] <= 24 * 6_144,
        "{peaks:?}: the peak grows by a copy of the IRI a parameter"
    );
    // 128, 256 and 512 parameters under the 8 KiB IRI: each costs a fixed amount, the reader's
    // nodes for it, and the facade, whose load gate refuses every name, holds no more than the
    // reader did. [measured: 814,907 / 1,487,267 / 2,836,899, 5,272 bytes a parameter, the load's
    // peak the reader's; before, 21.5 KB a parameter, 2.0 bytes an IRI byte of it]
    let mut peaks = Vec::new();
    for count in [128, 256, 512] {
        let document = undeclared_parameters(8_192, count);
        let reader = read(&document);
        let mut engine = Engine::new();
        let mut result = None;
        let load = measure(|| result = Some(engine.load_cxf(&document)));
        assert!(
            result.expect("ran").is_err(),
            "the load gate refuses the names"
        );
        assert!(
            load.bytes_max <= reader.bytes_max + 64 * 1_024,
            "{count}: the load held {} bytes, the reader {}",
            load.bytes_max,
            reader.bytes_max
        );
        peaks.push(reader.bytes_max);
    }
    let per_parameter = (peaks[2] - peaks[0]) / 384;
    assert!(
        per_parameter <= 8 * 1_024,
        "{per_parameter} bytes a parameter under an 8 KiB IRI: {peaks:?}"
    );
}

#[test]
fn a_loaded_model_holds_its_parameters_at_a_fixed_size_whatever_its_blocks_iri() {
    // 4 and 8 `CDL.Reals.PID` instances, each binding 11 parameters and named under a `@context`
    // prefix by an IRI of 4, 8 or 16 KiB, loaded through the facade: what the engine keeps for a
    // further instance is a few copies of its IRI for the block and its three ports, not one for
    // each parameter. [measured: 21,420 / 37,804 / 70,572 bytes an instance, 4.3 bytes an IRI
    // byte at 16 KiB; before, about 37 bytes an IRI byte, 33 of them the parameter table's]
    Engine::new().load_cxf(MODEL.as_bytes()).expect("loads");
    for iri in [4_096_u64, 8_192, 16_384] {
        let held: Vec<i64> = [4, 8]
            .into_iter()
            .map(|count| {
                let document = pid_instances(count, iri as usize, true);
                let mut engine = Engine::new();
                let info = measure(|| {
                    engine.load_cxf(&document).expect("the instances load");
                });
                drop(engine);
                info.bytes_current
            })
            .collect();
        let per_instance = u64::try_from((held[1] - held[0]) / 4).expect("grows");
        assert!(
            per_instance <= 6 * iri + 8 * 1_024,
            "{iri}-byte IRIs: {per_instance} bytes held an instance, {held:?}"
        );
    }
}

/// One leaf named by a 64 KiB IRI written once under a `@context` prefix, listing `count` shared
/// parameter nodes its class does not declare: about 30 bytes of document a listing, and 64 KiB
/// of identity under the leaf's IRI.
fn prefixed_leaf_listing(count: usize) -> Vec<u8> {
    let stem = format!("urn:{}", "i".repeat(64 * 1024 - 4));
    let references: String = (0..count)
        .map(|i| format!(r#",{{"@id":"urn:n.p{i}"}}"#))
        .collect();
    let nodes: String = (0..count)
        .map(|i| format!(r#",{{"@id":"urn:n.p{i}","S231:value":1.0}}"#))
        .collect();
    format!(
        r#"{{"@context":{{"S231":"http://data.ashrae.org/S231#","x":"{stem}"}},"@graph":[
{{"@id":"urn:M","@type":"S231:Block","S231:containsBlock":{{"@id":"x:"}},"S231:hasOutput":{{"@id":"urn:M.y"}}}},
{{"@id":"urn:M.y","@type":"S231:RealOutput","S231:isOfDataType":{{"@id":"S231:Real"}}}},
{{"@id":"x:","@type":"http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant","S231:hasParameter":[{{"@id":"urn:k.k"}}{references}],"S231:hasOutput":{{"@id":"x:.y"}}}},
{{"@id":"urn:k.k","S231:value":1.0}}{nodes},
{{"@id":"x:.y","@type":"S231:RealOutput","S231:isOfDataType":{{"@id":"S231:Real"}},"S231:isConnectedTo":{{"@id":"urn:M.y"}}}}
]}}"#
    )
    .into_bytes()
}

#[test]
fn listings_past_the_instance_path_budget_are_refused_at_a_fixed_cost_each() {
    // 256, 512 and 1,024 listings under a 64 KiB IRI, documents of 81 to 126 KB: their paths
    // take 16, 32 and 64 MiB, and every listing past the first 126 is refused by the 8 MiB
    // budget, by `oce_cxf::import_cxf` alone as by the facade, before anything is built for it.
    // The peak grows by a fixed amount a listing (the reader's nodes and a refusal each), not by
    // the IRI. [measured: peaks of 2,629,702 / 3,867,206 / 6,342,382, 4,834 bytes a listing;
    // before, the reader admitted every listing, and exporting the graph `import_cxf` returned
    // would write 2 × 64 KiB for each, about 134 MB at 1,024 listings (computed)]
    warm_up();
    let mut peaks = Vec::new();
    for count in [256_usize, 512, 1_024] {
        let document = prefixed_leaf_listing(count);
        let mut result = None;
        let info = measure(|| result = Some(import_cxf(&document, &ResolveOptions::default())));
        let Err(oce_cxf::CxfError::Validation(diagnostics)) = result.expect("ran") else {
            panic!("{count}: the listings past the budget are refused");
        };
        let refused = diagnostics
            .iter()
            .filter(|d| d.code == oce_diag::DiagCode::ResourceLimitExceeded)
            .count();
        // The leaf's `k` and `p0` to `p125` fit, 8,323,594 bytes; `p126` would take 8,389,135.
        assert_eq!(refused, count - 126, "{count}: {:?}", diagnostics.first());
        let mut engine = Engine::new();
        let mut loaded = None;
        let load = measure(|| loaded = Some(engine.load_cxf(&document)));
        assert!(loaded.expect("ran").is_err());
        assert!(
            load.bytes_max <= info.bytes_max + 64 * 1_024,
            "{count}: the facade held {} bytes, the reader {}",
            load.bytes_max,
            info.bytes_max
        );
        peaks.push(info.bytes_max);
    }
    let per_listing = (peaks[2] - peaks[0]) / 768;
    assert!(
        per_listing <= 8 * 1_024,
        "{per_listing} bytes a listing: {peaks:?}"
    );
}
