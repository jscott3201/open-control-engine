//! The unit spellings modelica-json and CXF-Core write reach the §7.10 unit gate, the host's point
//! list and the content id like the engine's own (issue #631; design note
//! `docs/design/unit-spellings.md`, owner rulings of 2026-11-12).
//!
//! The nine cases of the note's "What the corpus cannot show" table are here, on its own probe:
//! two gains in series, root input `TSup`, `gai1`, `gai2`, root output `ySup`. No tracked document
//! mixes the engine's spelling with an upstream one in one connected cluster, so these are the
//! only evidence that a `qudt:hasUnit` IRI and an `S231:unit` string are one reading. Expected
//! outcomes are the note's "C or R" column, stated by hand; every refusal is the complete
//! diagnostic list.
//!
//! Before, an upstream key was not read: cases 1 and 4 loaded, case 5 did not warn, and the
//! `unit:K` of case 3 was a unit that equals no unit string.
//!
//! Case 1 is also put with `qudt` unbound and bound to a near miss of a QUDT namespace (issue
//! #675): the key then named no property, so the load went through and the gate compared nothing.
//! Each is a refusal of the key itself.

use oce_api::{Engine, OcError, PointInfo};
use serde_json::{Value as Json, json};

const ROOT: &str = "http://example.org#Buildings.Controls.OBC.Examples.SeriesGain";
const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";

fn id(local: &str) -> String {
    format!("{ROOT}.{local}")
}

/// The probe: `TSup -> gai1.u`, `gai1.y -> gai2.u`, `gai2.y -> ySup`, in the dialect and under
/// the `@context` modelica-json writes, carrying no unit.
fn probe() -> Json {
    let reference = |local: &str| json!({ "@id": id(local) });
    let real = |local: &str, kind: &str, to: Option<&str>| {
        let mut node = json!({
            "@id": id(local),
            "@type": format!("S231:Real{kind}"),
            "S231:isOfDataType": { "@id": "S231:Real" }
        });
        if let Some(to) = to {
            node["S231:isConnectedTo"] = reference(to);
        }
        node
    };
    let gain = |name: &str| {
        [
            json!({
                "@id": id(name),
                "@type": "ex:Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
                "S231:hasParameter": reference(&format!("{name}.k")),
                "S231:hasInput": reference(&format!("{name}.u")),
                "S231:hasOutput": reference(&format!("{name}.y"))
            }),
            json!({
                "@id": id(&format!("{name}.k")),
                "@type": "S231:Parameter",
                "S231:value": { "@value": "0.5", "@type": XSD_DOUBLE }
            }),
        ]
    };
    let [gai1, gai1_k] = gain("gai1");
    let [gai2, gai2_k] = gain("gai2");
    json!({
        "@context": {
            "S231": "http://data.ashrae.org/S231#",
            "qudt": "http://qudt.org/schema/qudt#",
            "unit": "http://qudt.org/vocab/unit#",
            "q": "http://qudt.org/vocab/quantitykind#",
            "ex": "http://example.org#"
        },
        "@graph": [
            {
                "@id": ROOT,
                "@type": "S231:Block",
                "S231:containsBlock": [reference("gai1"), reference("gai2")],
                "S231:hasInput": reference("TSup"),
                "S231:hasOutput": reference("ySup")
            },
            real("TSup", "Input", Some("gai1.u")),
            gai1,
            gai1_k,
            real("gai1.u", "Input", None),
            real("gai1.y", "Output", Some("gai2.u")),
            gai2,
            gai2_k,
            real("gai2.u", "Input", None),
            real("gai2.y", "Output", Some("ySup")),
            real("ySup", "Output", None)
        ]
    })
}

/// `doc` with the property `key`, as written, set to `value` on the node `local`.
fn with(mut doc: Json, local: &str, key: &str, value: Json) -> Json {
    let node = doc["@graph"]
        .as_array_mut()
        .expect("@graph")
        .iter_mut()
        .find(|node| node["@id"] == id(local))
        .unwrap_or_else(|| panic!("no node `{local}`"));
    node[key] = value;
    doc
}

fn unit_iri(local: &str) -> Json {
    json!({ "@id": format!("unit:{local}") })
}

/// One diagnostic as `(code, subject, message)`.
type Reported = (String, Option<String>, String);

/// The `(unit, quantity, displayUnit)` columns of one point row.
type Columns = (Option<String>, Option<String>, Option<String>);

/// One load: the engine and every warning.
fn load(doc: &Json) -> Result<(Engine, Vec<Reported>), OcError> {
    let mut engine = Engine::new();
    let bytes = serde_json::to_vec(doc).expect("serialize");
    let (report, _) = engine.load_cxf(&bytes)?;
    let warnings = report
        .warnings
        .iter()
        .map(|d| {
            (
                d.code.as_str().to_owned(),
                d.subject.as_deref().map(str::to_owned),
                d.message.clone(),
            )
        })
        .collect();
    Ok((engine, warnings))
}

/// An engine loaded from `doc`, which must load without a warning.
fn loaded(doc: &Json) -> Engine {
    let (engine, warnings) = load(doc).unwrap_or_else(|error| panic!("expected a load: {error}"));
    assert_eq!(warnings, Vec::new());
    engine
}

/// Every error of the refusal of `doc`.
fn refusal(doc: &Json) -> Vec<Reported> {
    let error = match load(doc) {
        Err(error) => error,
        Ok(_) => panic!("expected a refusal, but the document loaded"),
    };
    error
        .all_diagnostics()
        .filter(|d| d.is_error())
        .map(|d| {
            (
                d.code.as_str().to_owned(),
                d.subject.as_deref().map(str::to_owned),
                d.message.clone(),
            )
        })
        .collect()
}

/// The host's row for the point `local`.
fn row(engine: &Engine, local: &str) -> PointInfo {
    engine
        .point_list(None)
        .expect("point inventory")
        .into_iter()
        .find(|point| point.path == id(local))
        .unwrap_or_else(|| panic!("no point `{local}`"))
}

/// The `(unit, quantity, displayUnit)` columns of the host's row for `local`.
fn columns(engine: &Engine, local: &str) -> Columns {
    let row = row(engine, local);
    (row.unit, row.quantity, row.display_unit)
}

fn unit(text: &str) -> Columns {
    (Some(text.to_owned()), None, None)
}

/// The refusal of the cluster driven by `source` whose members declare the units `values`.
fn unit_mismatch(source: &str, values: &[&str]) -> Vec<Reported> {
    vec![(
        "unit-quantity-mismatch".to_owned(),
        Some(id(source)),
        format!(
            "connected cluster members declare conflicting unit values {values:?}; §7.10 (R13.1) \
             requires agreement"
        ),
    )]
}

#[test]
fn the_probe_carries_no_unit_until_a_case_writes_one() {
    let engine = loaded(&probe());
    for point in engine.point_list(None).expect("point inventory") {
        assert_eq!(
            (point.unit, point.quantity, point.display_unit),
            (None, None, None),
            "{}",
            point.path
        );
    }
}

#[test]
fn case_1_a_string_unit_against_another_units_qudt_iri_is_a_mismatch() {
    // The issue's probe: before, `gai2.u` loaded and read `K`.
    let doc = with(
        with(probe(), "gai1.y", "S231:unit", json!("K")),
        "gai2.u",
        "qudt:hasUnit",
        unit_iri("DEG_C"),
    );
    assert_eq!(refusal(&doc), unit_mismatch("gai1.y", &["K", "degC"]));
}

#[test]
fn case_1_under_no_qudt_binding_is_refused_where_the_gate_never_saw_the_unit() {
    // Issue #675: with `qudt` left out of the `@context`, the key of case 1 named no property.
    // The document loaded, `gai2.u` read `K` by propagation, and the gate had nothing to compare.
    let mut doc = with(
        with(probe(), "gai1.y", "S231:unit", json!("K")),
        "gai2.u",
        "qudt:hasUnit",
        unit_iri("DEG_C"),
    );
    doc["@context"]
        .as_object_mut()
        .expect("context")
        .remove("qudt");
    assert_eq!(
        refusal(&doc),
        vec![(
            "malformed-document".to_owned(),
            Some(id("gai2.u")),
            format!(
                "property `qudt:hasUnit` on node `{}` reads as `qudt:hasUnit` under a `qudt` \
                 prefix the document's `@context` does not bind, so it names no property and the \
                 unit it carries would be left unread; bind `qudt` to \
                 `http://qudt.org/schema/qudt#` or `http://qudt.org/schema/qudt/`, or write the \
                 unit as `S231:unit`",
                id("gai2.u")
            )
        )]
    );
}

#[test]
fn case_1_under_a_near_miss_of_a_qudt_namespace_is_refused_naming_the_namespace() {
    // The same document with `qudt` bound one edit from a schema namespace, or to the unit
    // vocabulary: it loaded the same way, and is refused naming the namespace it wrote.
    for binding in [
        "https://qudt.org/schema/qudt#",
        "http://QUDT.org/schema/qudt/",
        "http://qudt.org:80/schema/qudt#",
        "http://qudt.org/schema/qudt",
        "http://qudt.org/vocab/unit#",
    ] {
        let mut doc = with(
            with(probe(), "gai1.y", "S231:unit", json!("K")),
            "gai2.u",
            "qudt:hasUnit",
            unit_iri("DEG_C"),
        );
        doc["@context"]["qudt"] = json!(binding);
        assert_eq!(
            refusal(&doc),
            vec![(
                "vocabulary-namespace-mismatch".to_owned(),
                Some(id("gai2.u")),
                format!(
                    "property `qudt:hasUnit` on node `{}` names `hasUnit` in `{binding}`; the \
                     QUDT schema property `hasUnit` is read only in \
                     `http://qudt.org/schema/qudt#` and `http://qudt.org/schema/qudt/`; write it \
                     in one of them, or write the unit as `S231:unit`",
                    id("gai2.u")
                )
            )],
            "{binding}"
        );
    }
}

#[test]
fn case_2_a_string_unit_against_the_same_units_qudt_iri_loads() {
    let doc = with(
        with(probe(), "gai1.y", "S231:unit", json!("K")),
        "gai2.u",
        "qudt:hasUnit",
        unit_iri("K"),
    );
    let engine = loaded(&doc);
    assert_eq!(columns(&engine, "gai1.y"), unit("K"));
    assert_eq!(columns(&engine, "gai2.u"), unit("K"));
}

#[test]
fn case_3_the_engines_own_key_holding_an_iri_node_unifies_with_the_unit_string() {
    // The reversed rule: before, `{"@id": "unit:K"}` under `S231:unit` was the unit `unit:K`,
    // and this document was refused as `K` against `unit:K`.
    let doc = with(
        with(probe(), "gai1.y", "S231:unit", unit_iri("K")),
        "gai2.u",
        "S231:unit",
        json!("K"),
    );
    let engine = loaded(&doc);
    assert_eq!(columns(&engine, "gai1.y"), unit("K"));
    assert_eq!(columns(&engine, "gai2.u"), unit("K"));
}

#[test]
fn case_4_a_root_outputs_qudt_unit_against_its_drivers_unit_is_a_mismatch() {
    let doc = with(
        with(probe(), "ySup", "qudt:hasUnit", unit_iri("DEG_C")),
        "gai2.y",
        "S231:unit",
        json!("K"),
    );
    assert_eq!(refusal(&doc), unit_mismatch("gai2.y", &["K", "degC"]));
}

#[test]
fn case_5_an_upstream_display_unit_against_another_warns_of_divergence() {
    let doc = with(
        with(probe(), "gai1.y", "S231:hasDisplayUnit", unit_iri("DEG_C")),
        "gai2.u",
        "S231:displayUnit",
        json!("degF"),
    );
    let (engine, warnings) = load(&doc).expect("a divergent display unit loads");
    assert_eq!(
        warnings,
        vec![(
            "display-unit-divergence".to_owned(),
            Some(id("gai1.y")),
            "connected cluster members declare divergent displayUnit values [\"degC\", \
             \"degF\"]; non-computational, advisory only (§7.17, R13.3)"
                .to_owned()
        )]
    );
    assert_eq!(
        columns(&engine, "gai1.y"),
        (None, None, Some("degC".to_owned()))
    );
    assert_eq!(
        columns(&engine, "gai2.u"),
        (None, None, Some("degF".to_owned()))
    );
    // The same display unit in both spellings does not warn.
    let agreeing = with(
        with(probe(), "gai1.y", "S231:hasDisplayUnit", unit_iri("DEG_C")),
        "gai2.u",
        "S231:displayUnit",
        json!("degC"),
    );
    loaded(&agreeing);
}

#[test]
fn case_6_a_qudt_quantity_kind_is_not_read_so_it_conflicts_with_nothing() {
    // modelica-json writes `PressureDifference` as `q:Pressure`. Read, it would refuse this
    // cluster, or report `Pressure` to the host for a pressure difference.
    let doc = with(
        with(
            probe(),
            "gai1.y",
            "S231:quantity",
            json!("PressureDifference"),
        ),
        "gai2.u",
        "qudt:hasQuantityKind",
        json!({ "@id": "q:Pressure" }),
    );
    let engine = loaded(&doc);
    let quantity = (None, Some("PressureDifference".to_owned()), None);
    assert_eq!(columns(&engine, "gai1.y"), quantity);
    // The one declared quantity propagates to the port that declares none.
    assert_eq!(columns(&engine, "gai2.u"), quantity);
    // Alone, the kind sets no quantity anywhere.
    let alone = with(
        probe(),
        "gai2.u",
        "qudt:hasQuantityKind",
        json!({ "@id": "q:Pressure" }),
    );
    assert_eq!(columns(&loaded(&alone), "gai2.u"), (None, None, None));
}

#[test]
fn case_7_the_s231_fallback_iri_unifies_with_its_unit_string() {
    for fallback in ["http://data.ashrae.org/S231#s/K", "S231:s/K"] {
        let doc = with(
            with(
                probe(),
                "gai1.y",
                "qudt:hasUnit",
                json!({ "@id": fallback }),
            ),
            "gai2.u",
            "S231:unit",
            json!("s/K"),
        );
        let engine = loaded(&doc);
        assert_eq!(columns(&engine, "gai1.y"), unit("s/K"), "{fallback}");
        assert_eq!(columns(&engine, "gai2.u"), unit("s/K"), "{fallback}");
    }
    // Against another unit string it is a mismatch like any other.
    let doc = with(
        with(
            probe(),
            "gai1.y",
            "qudt:hasUnit",
            json!({ "@id": "http://data.ashrae.org/S231#s/K" }),
        ),
        "gai2.u",
        "S231:unit",
        json!("1/K"),
    );
    assert_eq!(refusal(&doc), unit_mismatch("gai1.y", &["1/K", "s/K"]));
}

#[test]
fn case_8_a_root_inputs_unit_still_does_not_meet_the_port_it_drives() {
    // The documented limit, which this change does not move (its own issue closes it): a root
    // input's declaration is outside the gate, and the host's row for it shows the unit of the
    // port it drives. Both spellings of the declaration behave alike.
    for (key, value) in [
        ("S231:unit", json!("K")),
        ("qudt:hasUnit", unit_iri("K")),
        ("S231:hasUnit", unit_iri("K")),
    ] {
        let doc = with(
            with(probe(), "TSup", key, value),
            "gai1.u",
            "S231:unit",
            json!("degC"),
        );
        assert_eq!(columns(&loaded(&doc), "TSup"), unit("degC"), "{key}");
    }
}

#[test]
fn case_9_a_root_inputs_unit_still_reaches_no_point_row() {
    for (key, value) in [
        ("S231:unit", json!("K")),
        ("qudt:hasUnit", unit_iri("K")),
        ("S231:hasUnit", unit_iri("K")),
    ] {
        let doc = with(probe(), "TSup", key, value);
        assert_eq!(columns(&loaded(&doc), "TSup"), (None, None, None), "{key}");
    }
}

#[test]
fn a_root_inputs_unit_reaches_the_content_id_the_same_in_every_spelling() {
    // Cases 8 and 9 say where the declared unit does not go. It does go to the content id and
    // the export, and there every spelling is the one reading.
    let without = loaded(&probe()).content_id().expect("content id");
    let own = loaded(&with(probe(), "TSup", "S231:unit", json!("K")));
    let own_id = own.content_id().expect("content id");
    assert_ne!(own_id, without, "a declared unit is part of the content");
    let own_export = own.export_cxf().expect("exports");
    for (key, value) in [
        ("qudt:hasUnit", unit_iri("K")),
        ("S231:hasUnit", unit_iri("K")),
        ("S231:unit", unit_iri("K")),
        ("http://qudt.org/schema/qudt/hasUnit", json!("K")),
        (
            "qudt:hasUnit",
            json!({ "@id": "http://qudt.org/vocab/unit/K" }),
        ),
    ] {
        let engine = loaded(&with(probe(), "TSup", key, value.clone()));
        assert_eq!(
            engine.content_id().expect("content id"),
            own_id,
            "{key} {value}"
        );
        assert_eq!(
            engine.export_cxf().expect("exports").bytes,
            own_export.bytes,
            "{key} {value}"
        );
    }
    let other = loaded(&with(probe(), "TSup", "qudt:hasUnit", unit_iri("DEG_C")));
    assert_ne!(other.content_id().expect("content id"), own_id);
}

#[test]
fn every_spelling_of_a_cluster_gives_one_point_list_content_id_and_export() {
    // A whole document in the upstream spellings against the same document in the engine's.
    let upstream = with(
        with(
            with(probe(), "gai1.y", "qudt:hasUnit", unit_iri("K")),
            "gai1.y",
            "S231:hasDisplayUnit",
            unit_iri("DEG_C"),
        ),
        "ySup",
        "qudt:hasUnit",
        unit_iri("K"),
    );
    let own = with(
        with(
            with(probe(), "gai1.y", "S231:unit", json!("K")),
            "gai1.y",
            "S231:displayUnit",
            json!("degC"),
        ),
        "ySup",
        "S231:unit",
        json!("K"),
    );
    let (upstream, own) = (loaded(&upstream), loaded(&own));
    assert_eq!(
        upstream.point_list(None).expect("points"),
        own.point_list(None).expect("points")
    );
    assert_eq!(
        columns(&upstream, "gai1.y"),
        (Some("K".to_owned()), None, Some("degC".to_owned()))
    );
    // The declared unit propagated along both clusters.
    assert_eq!(columns(&upstream, "gai2.u"), unit("K"));
    assert_eq!(columns(&upstream, "gai2.y"), unit("K"));
    assert_eq!(
        upstream.content_id().expect("content id"),
        own.content_id().expect("content id")
    );
    assert_eq!(
        upstream.export_cxf().expect("exports").bytes,
        own.export_cxf().expect("exports").bytes
    );
}

#[test]
fn an_unmapped_unit_iri_refuses_the_load_with_one_typed_diagnostic() {
    let doc = with(probe(), "gai1.y", "qudt:hasUnit", unit_iri("M3-PER-MIN"));
    assert_eq!(
        refusal(&doc),
        vec![(
            "non-subset-construct".to_owned(),
            Some(id("gai1.y")),
            "S231:unit is the IRI node `http://qudt.org/vocab/unit#M3-PER-MIN`, which the closed \
             unit table does not map; a unit written as an IRI node is read only as a QUDT unit \
             the table lists (`http://qudt.org/vocab/unit#K`) or as an S231-namespace unit \
             string, so write one of those or the unit as a string (`\"K\"`)"
                .to_owned()
        )]
    );
}

#[test]
fn two_spellings_on_one_node_refuse_the_load_with_one_typed_diagnostic() {
    let doc = with(
        with(probe(), "gai1.y", "S231:unit", json!("K")),
        "gai1.y",
        "qudt:hasUnit",
        unit_iri("K"),
    );
    assert_eq!(
        refusal(&doc),
        vec![(
            "malformed-document".to_owned(),
            Some(id("gai1.y")),
            format!(
                "node `{}` writes `S231:unit` twice, as `S231:unit` and as `qudt:hasUnit`; a \
                 property must have exactly one spelling",
                id("gai1.y")
            )
        )]
    );
}

#[test]
fn a_load_in_the_upstream_spellings_is_byte_identical_every_time() {
    let doc = with(
        with(probe(), "gai1.y", "qudt:hasUnit", unit_iri("K")),
        "ySup",
        "S231:hasDisplayUnit",
        unit_iri("DEG_C"),
    );
    let first = loaded(&doc);
    let (id_first, export_first) = (
        first.content_id().expect("content id"),
        first.export_cxf().expect("exports"),
    );
    for _ in 0..8 {
        let again = loaded(&doc);
        assert_eq!(again.content_id().expect("content id"), id_first);
        assert_eq!(
            again.export_cxf().expect("exports").bytes,
            export_first.bytes
        );
        assert_eq!(
            again.point_list(None).expect("points"),
            first.point_list(None).expect("points")
        );
    }
}
