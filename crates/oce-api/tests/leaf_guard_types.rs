//! A leaf's own conditional guard reads each member in the type its class's catalog gives it
//! (issue #759, a follow-up of issues #734 and #735).
//!
//! A guard on a leaf's connector is evaluated over the leaf's own declarations before the model
//! is lowered. Each member is now held in its catalog type there too, as the parameter table
//! holds it: `2` and `2.0` on a Real member are one Real to the guard, an Integer member keeps
//! its Integer (and `2.0` on it is still refused), and Boolean and enumeration members are read
//! as written. The guards here name a Boolean member whose expression reads the others (a guard
//! is a Boolean parameter, its negation, a conjunction or an enumeration comparison). Every case
//! is written in each spelling of its number and compared with the same guard written on a
//! composite over declared parameters, which issue #735 already held in their declared types.
//!
//! What the base (84db2d76) did is stated beside each case that it decided differently.

use oce_api::{Engine, OcError};

const NS: &str = "http://example.org#";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
const P: &str = "Buildings.Controls.OBC.CDL.Types.SimpleController.P";
const TWO_POW_53: i64 = 1 << 53;

/// What a guard decided, read off the load: the guarded connector kept or pruned, or a refusal
/// that is not about the guarded connector's presence.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Guard {
    Holds,
    Fails,
    Refused(Vec<String>),
}

/// The leaf `S.blk` of `class` whose output `output` (of `signal`) is conditional on `guard`,
/// with Real `inputs` driven from root inputs, its `members` written as given (JSON), and
/// root parameters `root` (name, declared type, JSON).
struct Leaf<'a> {
    class: &'a str,
    signal: &'a str,
    inputs: &'a [&'a str],
    output: &'a str,
    guard: &'a str,
}

const PID: Leaf<'static> = Leaf {
    class: "CDL.Reals.PID",
    signal: "Real",
    inputs: &["u_s", "u_m"],
    output: "y",
    guard: "reverseActing",
};

const VECTOR_FILTER: Leaf<'static> = Leaf {
    class: "CDL.Routing.RealVectorFilter",
    signal: "Real",
    inputs: &["u1", "u2"],
    output: "y1",
    guard: "msk_1",
};

fn leaf_document(
    leaf: &Leaf<'_>,
    members: &[(&str, String)],
    root: &[(&str, &str, String)],
) -> String {
    let mut graph = vec![format!(
        r#"{{"@id":"{NS}S","@type":"S231:Block","S231:containsBlock":{{"@id":"{NS}S.blk"}},"S231:hasParameter":[{}],"S231:hasInput":[{}],"S231:hasOutput":{{"@id":"{NS}S.out"}}}}"#,
        refs(root.iter().map(|(name, ..)| format!("S.{name}"))),
        refs(leaf.inputs.iter().map(|name| format!("S.in_{name}"))),
    )];
    for (name, datatype, literal) in root {
        graph.push(format!(
            r#"{{"@id":"{NS}S.{name}","@type":"S231:Parameter","S231:isOfDataType":{{"@id":"S231:{datatype}"}},"S231:value":{literal}}}"#
        ));
    }
    for name in leaf.inputs {
        graph.push(format!(
            r#"{{"@id":"{NS}S.in_{name}","@type":"S231:RealInput","S231:isOfDataType":{{"@id":"S231:Real"}},"S231:isConnectedTo":{{"@id":"{NS}S.blk.{name}"}}}}"#
        ));
        graph.push(format!(
            r#"{{"@id":"{NS}S.blk.{name}","@type":"S231:RealInput","S231:isOfDataType":{{"@id":"S231:Real"}}}}"#
        ));
    }
    graph.push(format!(
        r#"{{"@id":"{NS}S.blk","@type":"{NS}Buildings.Controls.OBC.{}","S231:hasParameter":[{}],"S231:hasInput":[{}],"S231:hasOutput":{{"@id":"{NS}S.blk.{}"}}}}"#,
        leaf.class,
        refs(members.iter().map(|(name, _)| format!("S.blk.{name}"))),
        refs(leaf.inputs.iter().map(|name| format!("S.blk.{name}"))),
        leaf.output,
    ));
    for (name, literal) in members {
        graph.push(format!(
            r#"{{"@id":"{NS}S.blk.{name}","S231:value":{literal}}}"#
        ));
    }
    let signal = leaf.signal;
    graph.push(format!(
        r#"{{"@id":"{NS}S.blk.{}","@type":"S231:{signal}Output","S231:isOfDataType":{{"@id":"S231:{signal}"}},"S231:isConditionalComponent":true,"S231:conditionalExpression":"{}","S231:isConnectedTo":{{"@id":"{NS}S.out"}}}}"#,
        leaf.output, leaf.guard
    ));
    graph.push(format!(
        r#"{{"@id":"{NS}S.out","@type":"S231:{signal}Output","S231:isOfDataType":{{"@id":"S231:{signal}"}}}}"#
    ));
    wrap(&graph)
}

/// The same guard on a composite: root `S` declares `declared` (name, type, JSON) and holds a
/// constant `blk` and a constant `cond` that is conditional on `flag`.
fn composite_document(declared: &[(&str, &str, String)]) -> String {
    let mut graph = vec![format!(
        r#"{{"@id":"{NS}S","@type":"S231:Block","S231:hasParameter":[{}],"S231:containsBlock":[{{"@id":"{NS}S.blk"}},{{"@id":"{NS}S.cond"}}],"S231:hasOutput":{{"@id":"{NS}S.out"}}}}"#,
        refs(declared.iter().map(|(name, ..)| format!("S.{name}"))),
    )];
    for (name, datatype, literal) in declared {
        graph.push(format!(
            r#"{{"@id":"{NS}S.{name}","@type":"S231:Parameter","S231:isOfDataType":{{"@id":"S231:{datatype}"}},"S231:value":{literal}}}"#
        ));
    }
    for (name, extra) in [
        (
            "blk",
            r#","S231:isConnectedTo":{"@id":"http://example.org#S.out"}"#,
        ),
        ("cond", ""),
    ] {
        let guard = if name == "cond" {
            r#""S231:isConditionalComponent":true,"S231:conditionalExpression":"flag","#
        } else {
            ""
        };
        graph.push(format!(
            r#"{{"@id":"{NS}S.{name}",{guard}"@type":"{NS}Buildings.Controls.OBC.CDL.Reals.Sources.Constant","S231:hasParameter":{{"@id":"{NS}S.{name}.k"}},"S231:hasOutput":{{"@id":"{NS}S.{name}.y"}}}}"#
        ));
        graph.push(format!(r#"{{"@id":"{NS}S.{name}.k","S231:value":1.0}}"#));
        graph.push(format!(
            r#"{{"@id":"{NS}S.{name}.y","@type":"S231:RealOutput","S231:isOfDataType":{{"@id":"S231:Real"}}{extra}}}"#
        ));
    }
    graph.push(format!(
        r#"{{"@id":"{NS}S.out","@type":"S231:RealOutput","S231:isOfDataType":{{"@id":"S231:Real"}}}}"#
    ));
    wrap(&graph)
}

fn refs(ids: impl Iterator<Item = String>) -> String {
    ids.map(|id| format!(r#"{{"@id":"{NS}{id}"}}"#))
        .collect::<Vec<_>>()
        .join(",")
}

fn wrap(graph: &[String]) -> String {
    format!(
        r#"{{"@context":{{"S231":"http://data.ashrae.org/S231#"}},"@graph":[{}]}}"#,
        graph.join(",\n")
    )
}

fn load(doc: &str) -> Result<Engine, OcError> {
    let mut engine = Engine::new();
    engine.load_cxf(doc.as_bytes()).map(|_| engine)
}

fn codes(error: &OcError) -> Vec<String> {
    let mut codes: Vec<String> = error
        .all_diagnostics()
        .filter(|d| d.is_error())
        .map(|d| d.code.as_str().to_owned())
        .collect();
    codes.sort();
    codes.dedup();
    codes
}

/// The leaf's guard: its connector kept (the leaf loads), or pruned (the class's interface is
/// one output short, the only refusal).
fn leaf_guard(doc: &str) -> Guard {
    match load(doc) {
        Ok(_) => Guard::Holds,
        Err(error) => {
            let codes = codes(&error);
            let pruned = error.all_diagnostics().filter(|d| d.is_error()).all(|d| {
                d.message.starts_with("block interface mismatch")
                    || d.code.as_str() == "inactive-conditional-node"
            });
            if pruned && codes.iter().any(|code| code == "malformed-document") {
                Guard::Fails
            } else {
                Guard::Refused(codes)
            }
        }
    }
}

/// The composite's guard: `cond` kept or pruned, or the refusal.
fn composite_guard(doc: &str) -> Guard {
    let mut engine = Engine::new();
    match engine.load_cxf(doc.as_bytes()) {
        Ok((report, _)) if report.block_count == 2 => Guard::Holds,
        Ok(_) => Guard::Fails,
        Err(error) => Guard::Refused(codes(&error)),
    }
}

/// Every spelling of the integral number `n`: bare JSON integer and float, typed `xsd:integer`
/// and `xsd:double`, and an Integer expression when `n - 1` is exact.
fn spellings(n: i64) -> Vec<String> {
    let mut all = vec![
        n.to_string(),
        format!("{n}.0"),
        format!(r#"{{"@value":"{n}","@type":"{XSD}integer"}}"#),
        format!(r#"{{"@value":"{n}.0","@type":"{XSD}double"}}"#),
    ];
    if n.abs() < TWO_POW_53 {
        all.push(format!(r#""{} + 1""#, n - 1));
    }
    all
}

fn quoted(expression: &str) -> String {
    format!("\"{expression}\"")
}

/// A PID leaf with `yMax` written `literal`, `yMin` far below, and `reverseActing = flag` where
/// `flag` reads `yMax` as `x`.
fn pid_over(literal: &str, flag: &str) -> String {
    leaf_document(
        &PID,
        &[
            ("controllerType", quoted(P)),
            ("yMin", "-1.0e300".to_owned()),
            ("yMax", literal.to_owned()),
            ("reverseActing", quoted(&flag.replace('x', "yMax"))),
        ],
        &[],
    )
}

/// The same flag on a composite whose `x` is declared Real and written `literal`.
fn composite_over(literal: &str, flag: &str) -> String {
    composite_document(&[
        ("x", "Real", literal.to_owned()),
        ("flag", "Boolean", quoted(flag)),
    ])
}

/// The leaf's guard in every spelling of `literals`, required to be one; and the composite's,
/// required to be the same.
fn agreed(literals: &[String], flag: &str) -> Guard {
    let leaf = leaf_guard(&pid_over(&literals[0], flag));
    for literal in literals {
        assert_eq!(
            leaf_guard(&pid_over(literal, flag)),
            leaf,
            "leaf {flag} at {literal}"
        );
        assert_eq!(
            composite_guard(&composite_over(literal, flag)),
            leaf,
            "composite {flag} at {literal}"
        );
    }
    leaf
}

#[test]
fn the_issue_reproduction_a_sum_at_two_to_the_fifty_third_fails_in_every_spelling() {
    // `yMax + 1 > yMax` at 2^53: the Real sum rounds back to 2^53. The base read the Integer
    // spellings as Integers (the guard held, the leaf loaded) and the Real ones as Reals (the
    // output was pruned, the leaf refused).
    let refusal = |literal: &str| {
        load(&pid_over(literal, "x + 1 > x"))
            .err()
            .map(|error| {
                error
                    .all_diagnostics()
                    .map(|d| format!("{}|{:?}|{}", d.code.as_str(), d.subject, d.message))
                    .collect::<Vec<_>>()
            })
            .expect("the pruned output leaves the PID an output short")
    };
    let literals = spellings(TWO_POW_53);
    let expected = refusal(&literals[0]);
    for literal in &literals {
        assert_eq!(refusal(literal), expected, "{literal}");
    }
    assert_eq!(agreed(&literals, "x + 1 > x"), Guard::Fails);
}

#[test]
fn the_issue_reproduction_an_integer_overflow_in_a_member_expression_loads_in_every_spelling() {
    // `k = yMax * 2^62`: over the Integer `2` the product overflowed, `k` did not ground and the
    // guard named an unknown parameter (the base refused the Integer spellings); over the Real it
    // is 2^63. Every spelling now loads, as one model.
    let doc = |literal: &str| {
        leaf_document(
            &PID,
            &[
                ("controllerType", quoted(P)),
                ("yMax", literal.to_owned()),
                ("k", quoted("yMax * 4611686018427387904")),
                ("reverseActing", quoted("k > 0")),
            ],
            &[],
        )
    };
    let literals = spellings(2);
    let identity = |literal: &str| {
        let engine = load(&doc(literal)).unwrap_or_else(|e| panic!("{literal}: {e:?}"));
        (
            engine.content_id().expect("an id").as_str().to_owned(),
            engine.compile_blob().expect("a blob").bytes,
        )
    };
    let expected = identity(&literals[0]);
    for literal in &literals {
        assert_eq!(identity(literal), expected, "{literal}");
    }
    // The composite form agrees: its `m = x * 2^62` is a Real for a declared Real `x`.
    for literal in &literals {
        let composite = composite_document(&[
            ("x", "Real", literal.clone()),
            ("m", "Real", quoted("x * 4611686018427387904")),
            ("flag", "Boolean", quoted("m > 0")),
        ]);
        assert_eq!(composite_guard(&composite), Guard::Holds, "{literal}");
    }
}

#[test]
fn comparisons_and_arithmetic_over_a_real_member_agree_in_every_spelling_and_on_a_composite() {
    // Each row: the flag, and what it decides over 2 (no row differed between spellings on the
    // base: small Integers are exact either way, and `/` is a Real quotient for both).
    let small = [
        ("x == 2", Guard::Holds),
        ("x == 2.0", Guard::Holds),
        ("x <> 2", Guard::Fails),
        ("x < 3", Guard::Holds),
        ("x <= 2", Guard::Holds),
        ("x > 2", Guard::Fails),
        ("x / 4 > 0.4", Guard::Holds),
        ("div(x, 4) == 0", Guard::Holds),
        ("x * x == 4", Guard::Holds),
        ("x - 3 < 0", Guard::Holds),
        ("x + 0.5 == 2.5", Guard::Holds),
    ];
    for (flag, expected) in small {
        assert_eq!(agreed(&spellings(2), flag), expected, "{flag}");
    }
    // At the 2^53 boundaries. On the base the Integer spellings differed from the Real ones in
    // the rows marked: `+ 1` at 2^53 and `- 1` at -2^53 (Integer exact, Real rounded), and
    // `x * x` (an Integer overflow, refused as an unknown guard parameter).
    let boundaries = [
        (TWO_POW_53 - 1, "x + 1 > x", Guard::Holds),
        (TWO_POW_53, "x + 1 > x", Guard::Fails),
        (TWO_POW_53, "x - 1 < x", Guard::Holds),
        (-TWO_POW_53, "x - 1 < x", Guard::Fails),
        (-TWO_POW_53, "x + 1 > x", Guard::Holds),
        (TWO_POW_53, "x * 2 > x", Guard::Holds),
        (TWO_POW_53, "x * x > 0", Guard::Holds),
        (TWO_POW_53 - 1, "x * x > 0", Guard::Holds),
    ];
    for (n, flag, expected) in boundaries {
        assert_eq!(agreed(&spellings(n), flag), expected, "{flag} at {n}");
    }
}

#[test]
fn an_integer_past_two_to_the_fifty_third_on_a_real_member_is_refused_and_its_real_spelling_is_another_number()
 {
    // 2^53 + 1 is no Real: the Integer spellings are refused on the member, as on the composite's
    // declaration; the Real spellings read the Real 2^53 that JSON rounds them to.
    let n = TWO_POW_53 + 1;
    let integers = [
        n.to_string(),
        format!(r#"{{"@value":"{n}","@type":"{XSD}integer"}}"#),
    ];
    for literal in &integers {
        let refused = vec!["parameter-out-of-range".to_owned()];
        assert_eq!(
            leaf_guard(&pid_over(literal, "x - 1 < x")),
            Guard::Refused(refused.clone())
        );
        assert_eq!(
            composite_guard(&composite_over(literal, "x - 1 < x")),
            Guard::Refused(refused)
        );
    }
    let reals = [
        format!("{n}.0"),
        format!(r#"{{"@value":"{n}.0","@type":"{XSD}double"}}"#),
    ];
    assert_eq!(agreed(&reals, "x - 1 < x"), Guard::Holds);
    assert_eq!(agreed(&reals, "x + 1 > x"), Guard::Fails);
}

#[test]
fn signed_zeros_extremes_and_non_finite_values_agree_on_a_leaf_and_a_composite() {
    // Unchanged from the base: none of these is an Integer that converts differently.
    let zeros = [
        "0".to_owned(),
        "0.0".to_owned(),
        "-0.0".to_owned(),
        format!(r#"{{"@value":"-0","@type":"{XSD}integer"}}"#),
    ];
    assert_eq!(agreed(&zeros, "x == 0"), Guard::Holds);
    assert_eq!(agreed(&zeros, "x < 0"), Guard::Fails);
    assert_eq!(agreed(&zeros, "x <> x"), Guard::Fails);
    let non_finite = Guard::Refused(vec![
        "conditional-guard-unknown-parameter".to_owned(),
        "non-finite-parameter".to_owned(),
    ]);
    // `1 / 0` and `1e308 * 10` do not evaluate to a finite value: the flag does not ground.
    assert_eq!(agreed(&zeros, "1 / x < 0"), non_finite);
    assert_eq!(agreed(&["1e308".to_owned()], "x * 10 > x"), non_finite);
    assert_eq!(agreed(&["1e308".to_owned()], "x <> x"), Guard::Fails);
    assert_eq!(agreed(&["5e-324".to_owned()], "x * 10 > x"), Guard::Holds);
    // A member that is not finite never grounds; the guard cannot read it.
    for expression in ["0.0 / 0.0", "1e308 * 10"] {
        assert_eq!(
            agreed(&[quoted(expression)], "x == 0"),
            Guard::Refused(vec![
                "conditional-guard-unknown-parameter".to_owned(),
                "grounding-failed".to_owned(),
                "non-finite-parameter".to_owned(),
            ]),
            "{expression}"
        );
    }
}

#[test]
fn an_integer_member_keeps_its_integer_and_a_real_on_it_is_still_refused() {
    // `RealVectorFilter.nin` is an Integer; `msk_1`, an element of its Boolean mask, is the
    // guard. The Integer spellings decide as Integers, alike on a composite declaring `nin`
    // Integer; `2.0` on the Integer is refused, as before (no Real is narrowed).
    let doc = |nin: &str, flag: &str| {
        leaf_document(
            &VECTOR_FILTER,
            &[
                ("nin", nin.to_owned()),
                ("nout", "1".to_owned()),
                ("msk_1", quoted(flag)),
                ("msk_2", "false".to_owned()),
            ],
            &[],
        )
    };
    let integers = [
        "2".to_owned(),
        format!(r#"{{"@value":"2","@type":"{XSD}integer"}}"#),
        quoted("1 + 1"),
    ];
    for (flag, expected) in [
        ("nin > 1", Guard::Holds),
        ("nin * 4 > 7", Guard::Holds),
        ("div(nin, 4) == 0", Guard::Holds),
        ("nin / 4 > 0.4", Guard::Holds),
        ("nin == 3", Guard::Fails),
    ] {
        for nin in &integers {
            let leaf = leaf_guard(&doc(nin, flag));
            assert_eq!(leaf, expected, "{flag} at {nin}");
            let composite = composite_document(&[
                ("nin", "Integer", nin.clone()),
                ("flag", "Boolean", quoted(flag)),
            ]);
            assert_eq!(
                composite_guard(&composite),
                expected,
                "composite {flag} at {nin}"
            );
        }
    }
    // An Integer overflow in an Integer member's expression does not ground on either.
    let overflow = "nin * 4611686018427387904 > 0";
    assert!(matches!(leaf_guard(&doc("2", overflow)), Guard::Refused(_)));
    for real in [
        "2.0".to_owned(),
        format!(r#"{{"@value":"2.0","@type":"{XSD}double"}}"#),
    ] {
        assert!(load(&doc(&real, "nin > 1")).is_err(), "{real}");
        let composite = composite_document(&[
            ("nin", "Integer", real.clone()),
            ("flag", "Boolean", quoted("nin > 1")),
        ]);
        assert_eq!(
            composite_guard(&composite),
            Guard::Refused(vec!["parameter-kind-mismatch".to_owned()]),
            "{real}"
        );
    }
}

#[test]
fn boolean_and_enumeration_members_are_read_as_written() {
    let pid = |members: &[(&str, String)], guard: &'static str| {
        leaf_document(&Leaf { guard, ..PID }, members, &[])
    };
    for (value, holds) in [("true", true), ("false", false)] {
        let members = [
            ("controllerType", quoted(P)),
            ("reverseActing", value.to_owned()),
        ];
        let expected = |holds: bool| if holds { Guard::Holds } else { Guard::Fails };
        assert_eq!(leaf_guard(&pid(&members, "reverseActing")), expected(holds));
        assert_eq!(
            leaf_guard(&pid(&members, "not reverseActing")),
            expected(!holds)
        );
    }
    for (member, equal) in [("P", true), ("PI", false)] {
        let members = [
            (
                "controllerType",
                quoted(&format!(
                    "Buildings.Controls.OBC.CDL.Types.SimpleController.{member}"
                )),
            ),
            ("Ti", "0.5".to_owned()),
        ];
        let expected = |holds: bool| if holds { Guard::Holds } else { Guard::Fails };
        assert_eq!(
            leaf_guard(&pid(
                &members,
                "controllerType == Buildings.Controls.OBC.CDL.Types.SimpleController.P"
            )),
            expected(equal),
            "{member}"
        );
        assert_eq!(
            leaf_guard(&pid(
                &members,
                "controllerType <> Buildings.Controls.OBC.CDL.Types.SimpleController.P"
            )),
            expected(!equal),
            "{member}"
        );
    }
}

#[test]
fn a_member_left_to_its_default_is_not_in_the_guard_scope_on_a_leaf_or_a_composite() {
    // Unchanged from the base: a guard reads declarations, and a default is none.
    let leaf = leaf_document(
        &PID,
        &[
            ("controllerType", quoted(P)),
            ("reverseActing", quoted("yMax > 0")),
        ],
        &[],
    );
    let composite = composite_document(&[("flag", "Boolean", quoted("x > 0"))]);
    let unknown = Guard::Refused(vec![
        "conditional-guard-unknown-parameter".to_owned(),
        "grounding-failed".to_owned(),
    ]);
    assert_eq!(leaf_guard(&leaf), unknown);
    assert_eq!(composite_guard(&composite), unknown);
}

#[test]
fn a_leaf_guard_does_not_see_an_enclosing_parameter_in_any_spelling() {
    // Unchanged from the base, and a known gap: the guard scope of a leaf holds its own
    // declarations only, so a member bound to an enclosing parameter does not ground there and
    // the guard names an unknown parameter, in every spelling alike. The same flag over the same
    // declarations on a composite is decided.
    for literal in spellings(TWO_POW_53) {
        let leaf = leaf_document(
            &PID,
            &[
                ("controllerType", quoted(P)),
                ("yMin", "-1.0e300".to_owned()),
                ("yMax", quoted("p")),
                ("reverseActing", quoted("yMax + 1 > yMax")),
            ],
            &[("p", "Real", literal.clone())],
        );
        assert_eq!(
            leaf_guard(&leaf),
            Guard::Refused(vec!["conditional-guard-unknown-parameter".to_owned()]),
            "{literal}"
        );
        let composite = composite_document(&[
            ("p", "Real", literal.clone()),
            ("x", "Real", quoted("p")),
            ("flag", "Boolean", quoted("x + 1 > x")),
        ]);
        assert_eq!(composite_guard(&composite), Guard::Fails, "{literal}");
    }
}

#[test]
fn a_guard_decision_is_the_same_on_every_load() {
    let doc = pid_over(&spellings(TWO_POW_53)[0], "x + 1 > x");
    let first = leaf_guard(&doc);
    for _ in 0..3 {
        assert_eq!(leaf_guard(&doc), first);
    }
}
