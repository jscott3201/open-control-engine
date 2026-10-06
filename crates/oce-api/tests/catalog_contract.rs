//! Complete metadata parity and independent content-tag evidence. No numeric engine oracle claim.

use oce_api::{
    CATALOG_JSON, CatalogDefault, CatalogPortNaming, CatalogRule, catalog, catalog_content_id,
    catalog_to_json,
};
use serde_json::{Value, json};

#[path = "support/catalog_schema.rs"]
mod catalog_schema;

#[path = "support/catalog_rule_payloads.rs"]
mod catalog_rule_payloads;

#[test]
fn canonical_catalog_matches_packaged_bytes_and_repeats_exactly() {
    let first = catalog_to_json(catalog());
    let second = catalog_to_json(catalog());
    assert_eq!(first, second);
    assert_eq!(first, CATALOG_JSON);
    assert_eq!(
        catalog_content_id(catalog()),
        include_str!("fixtures/catalog.content-id.txt").trim()
    );
}

#[test]
fn every_legacy_manifest_field_agrees_with_the_independent_facade_projection() {
    // The pre-existing owner manifest is a separate serializer, kept byte-unchanged. Its
    // Required rule historically omitted kind; that additional payload is checked below.
    let manifest: Value = serde_json::from_str(include_str!(
        "../../../tools/reference-catalog/oce-blocks.registry-manifest.json"
    ))
    .unwrap();
    let canonical: Value = serde_json::from_str(&catalog_to_json(catalog())).unwrap();
    let entries = canonical["entries"].as_array().unwrap();
    assert_eq!(manifest.as_array().unwrap().len(), entries.len());
    for ((old, new), typed) in manifest
        .as_array()
        .unwrap()
        .iter()
        .zip(entries)
        .zip(catalog())
    {
        for field in ["class_path", "stateful", "reserved", "width_driven"] {
            assert_eq!(old[field], new[field], "{}: {field}", typed.class_path);
        }
        assert_eq!(old["port_naming"], new["naming"]);
        for (direction, names) in [("inputs", "input_names"), ("outputs", "output_names")] {
            let ports = new[direction].as_array().unwrap();
            assert_eq!(old[direction].as_array().unwrap().len(), ports.len());
            for (index, port) in ports.iter().enumerate() {
                assert_eq!(old[direction][index], port["kind"]);
                if typed.naming == CatalogPortNaming::Named {
                    assert_eq!(old[names][index], port["name"]);
                } else {
                    assert_eq!(port["name"], Value::Null);
                }
            }
        }
        assert_eq!(
            old["param_rules"].as_array().unwrap().len(),
            typed.param_rules.len()
        );
        for (rule, projected) in old["param_rules"]
            .as_array()
            .unwrap()
            .iter()
            .zip(new["param_rules"].as_array().unwrap())
        {
            for (field, value) in rule.as_object().unwrap() {
                let expected = if projected[field].is_string() && value.is_number() {
                    json!(format!("{:016x}", value.as_f64().unwrap().to_bits()))
                } else {
                    value.clone()
                };
                assert_eq!(
                    expected, projected[field],
                    "{}: {rule}: {field}",
                    typed.class_path
                );
            }
            assert_eq!(
                projected.as_object().unwrap().len(),
                rule.as_object().unwrap().len() + usize::from(rule["rule"] == "Required")
            );
        }
        assert_eq!(
            old["param_defaults"].as_array().unwrap().len(),
            typed.param_defaults.len()
        );
        for (old, new) in old["param_defaults"]
            .as_array()
            .unwrap()
            .iter()
            .zip(new["param_defaults"].as_array().unwrap())
        {
            assert_eq!(old["name"], new["name"]);
            let projected = &new["default"];
            match old["kind"].as_str().unwrap() {
                "literal" => {
                    let field = ["real", "integer", "boolean", "string", "enum"]
                        .into_iter()
                        .find(|field| old.get(field).is_some())
                        .unwrap();
                    assert_eq!(projected["kind"], field);
                    if field == "real" {
                        assert_eq!(
                            projected["bits"],
                            format!("{:016x}", old[field].as_f64().unwrap().to_bits())
                        );
                    } else {
                        assert_eq!(projected["value"], old[field]);
                    }
                }
                "derived" => {
                    assert_eq!(projected["kind"], "derived");
                    assert_eq!(projected["formula"], old["formula"]);
                }
                "required" => assert_eq!(projected, &json!({"kind":"required"})),
                other => panic!("unrecognized manifest default {other}"),
            }
        }
    }
    for (facade, owner) in catalog().iter().zip(oce_blocks::catalog()) {
        for (facade, owner) in facade.param_rules.iter().zip(owner.param_rules) {
            if let oce_blocks::ParamRule::Required { kind, .. } = owner {
                let CatalogRule::Required { kind: actual, .. } = facade else {
                    panic!("lost required rule")
                };
                match (kind, actual) {
                    (oce_api::ValueType::Real, oce_api::CatalogValueKind::Real)
                    | (oce_api::ValueType::Integer, oce_api::CatalogValueKind::Integer)
                    | (oce_api::ValueType::Boolean, oce_api::CatalogValueKind::Boolean)
                    | (oce_api::ValueType::String, oce_api::CatalogValueKind::String) => {}
                    (
                        oce_api::ValueType::Enum(id),
                        oce_api::CatalogValueKind::Enum {
                            class_path,
                            members,
                        },
                    ) => {
                        let descriptor = oce_model::enum_descriptor(*id).unwrap();
                        assert_eq!(descriptor.class_path, *class_path);
                        assert_eq!(descriptor.members, *members);
                    }
                    _ => panic!("required kind projection differs"),
                }
            }
        }
    }
}

#[test]
fn all_catalog_metadata_changes_affect_content_identity() {
    let entry = catalog()
        .iter()
        .find(|entry| entry.class_path == "CDL.Reals.PID")
        .unwrap()
        .clone();
    let baseline = vec![entry];
    let id = catalog_content_id(&baseline);
    type CatalogMutation = Box<dyn Fn(&mut oce_api::CatalogEntry)>;
    let mutations: Vec<CatalogMutation> = vec![
        Box::new(|e| e.class_path = "changed"),
        Box::new(|e| e.inputs.clear()),
        Box::new(|e| e.inputs.swap(0, 1)),
        Box::new(|e| e.inputs[0].name = Some("changed")),
        Box::new(|e| e.inputs[0].kind = oce_api::CatalogPortKind::Boolean),
        Box::new(|e| e.outputs[0].name = None),
        Box::new(|e| e.outputs.clear()),
        Box::new(|e| e.naming = CatalogPortNaming::Positional),
        Box::new(|e| e.param_rules.pop().map(|_| ()).unwrap()),
        Box::new(|e| e.param_rules.swap(0, 1)),
        Box::new(|e| e.param_defaults.clear()),
        Box::new(|e| e.param_defaults.swap(0, 1)),
        Box::new(|e| e.param_defaults[0].name = "changed"),
        Box::new(|e| e.param_defaults[0].default = CatalogDefault::Required),
        Box::new(|e| e.width_driven = !e.width_driven),
        Box::new(|e| e.stateful = !e.stateful),
        Box::new(|e| e.reserved = !e.reserved),
    ];
    for mutation in mutations {
        let mut changed = baseline.clone();
        mutation(&mut changed[0]);
        assert_ne!(catalog_content_id(&changed), id);
    }
    let mut reordered = catalog().to_vec();
    reordered.swap(0, 1);
    assert_ne!(
        catalog_content_id(&reordered),
        catalog_content_id(catalog())
    );
}

#[test]
fn default_payloads_names_absence_and_escaping_remain_distinct() {
    let mut entry = catalog()[0].clone();
    let defaults = [
        CatalogDefault::Real(0.0),
        CatalogDefault::Real(-0.0),
        CatalogDefault::Integer(i64::MIN),
        CatalogDefault::Integer(i64::MAX),
        CatalogDefault::Boolean(false),
        CatalogDefault::Boolean(true),
        CatalogDefault::String("quote\"\nλ"),
        CatalogDefault::String("quote\\\"\\nλ"),
        CatalogDefault::EnumMember("CDL.Types.SimpleController.P"),
        CatalogDefault::EnumMember("CDL.Types.SimpleController.PI"),
        CatalogDefault::Derived("2 * x"),
        CatalogDefault::Derived("3 * x"),
        CatalogDefault::Required,
    ];
    let mut ids = Vec::new();
    for default in defaults {
        entry.param_defaults = vec![oce_api::CatalogParamDefault {
            name: "quoted\"\nname",
            default,
        }];
        let bytes = catalog_to_json(&[entry.clone()]);
        let value: Value = serde_json::from_str(&bytes).unwrap();
        assert_eq!(
            value["entries"][0]["param_defaults"][0]["name"],
            "quoted\"\nname"
        );
        let id = catalog_content_id(&[entry.clone()]);
        assert!(!ids.contains(&id));
        ids.push(id);
    }
    assert_eq!(
        catalog_to_json(&[]),
        "{\"entries\":[],\"schema_revision\":1}\n"
    );
    assert_ne!(catalog_content_id(&[]), catalog_content_id(&[entry]));
}

#[test]
fn real_literals_and_bounds_keep_signed_zero_nonfinite_and_nan_payload_bits() {
    let mut entry = catalog()[0].clone();
    entry.param_defaults.clear();
    entry.param_rules.clear();
    let mut previous = Vec::new();
    for bits in [
        0,
        1 << 63,
        1,
        0x7ff0000000000000,
        0xfff0000000000000,
        0x7ff8000000000001,
        0x7ff8000000000002,
        0xfff8000000000001,
    ] {
        entry.param_defaults = vec![oce_api::CatalogParamDefault {
            name: "literal",
            default: CatalogDefault::Real(f64::from_bits(bits)),
        }];
        entry.param_rules = vec![CatalogRule::RealGreaterThan {
            name: "bound",
            min: f64::from_bits(bits),
        }];
        let json: Value = serde_json::from_str(&catalog_to_json(&[entry.clone()])).unwrap();
        let hex = format!("{bits:016x}");
        assert_eq!(
            json["entries"][0]["param_defaults"][0]["default"]["bits"],
            hex
        );
        assert_eq!(json["entries"][0]["param_rules"][0]["min"], hex);
        let id = catalog_content_id(&[entry.clone()]);
        assert!(!previous.contains(&id));
        previous.push(id);
    }
}

/// Independent schoolbook byte multiplication modulo 2^128 (no shared StableHash or u128
/// multiplication). Offset/prime are the FNV-1a-128 constants documented by the facade.
fn independent_hash(bytes: &[u8]) -> String {
    let mut hash = [
        0x8d, 0xc5, 0x95, 0x62, 0x75, 0x21, 0xb8, 0x62, 0x42, 0x01, 0xbb, 0x07, 0x2e, 0x27, 0x62,
        0x6c,
    ];
    let mut prime = [0_u32; 16];
    prime[0] = 0x3b;
    prime[1] = 1;
    prime[11] = 1;
    for byte in bytes {
        hash[0] ^= *byte;
        let mut product = [0_u32; 16];
        for i in 0..16 {
            for j in 0..16 - i {
                product[i + j] += u32::from(hash[i]) * prime[j];
            }
        }
        for i in 0..15 {
            product[i + 1] += product[i] >> 8;
            hash[i] = product[i] as u8;
        }
        hash[15] = product[15] as u8;
    }
    hash.iter()
        .rev()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn catalog_content_tag_agrees_with_independent_byte_arithmetic_and_domain_golden() {
    assert_eq!(independent_hash(b"abc"), "a68d622cec8b5822836dbc7977af7f3b");
    let bytes = [b"oce:catalog:1\0".as_slice(), CATALOG_JSON.as_bytes()].concat();
    assert_eq!(
        catalog_content_id(catalog()),
        format!("catalog:1:fnv1a128:{}", independent_hash(&bytes))
    );
    assert_ne!(
        independent_hash(CATALOG_JSON.as_bytes()),
        independent_hash(&bytes)
    );
}
