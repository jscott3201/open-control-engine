//! An enumeration or constant package is named by its canonical class path and nothing else
//! (issue #762, the owner's ruling of 2026-11-22; typing IRIs, issue #637).
//!
//! `enumeration_class_by_path` is the one place identity is decided: every lookup the engine
//! makes from document text (`enum_class_id`, `enumeration_member`, the typed member parsers, the
//! G36 Integer constants) answers exactly what it answers, and `enumeration_spelling_hint` only
//! ever words a refusal for a spelling the identity refused.

use oce_model::{
    EnumClassId, EnumerationKind, SimpleController, ZeroTime, enum_class_id,
    enum_descriptor_by_path, enumeration_class_by_iri, enumeration_class_by_path,
    enumeration_classes, enumeration_member, enumeration_spelling_hint, g36_enum_class_id,
    g36_integer_constant, is_g36_integer_constant_package,
};

/// Every non-canonical spelling of the class at `qualified` the ruling names, plus case,
/// whitespace and punctuation variants.
fn non_canonical(qualified: &str, type_name: &str) -> Vec<String> {
    let relative = qualified.strip_prefix("Buildings.Controls.OBC.").unwrap();
    let mut out = vec![
        type_name.to_owned(),
        relative.to_owned(),
        format!("Controls.OBC.{relative}"),
        format!("Foo.{type_name}"),
        format!("Foo.{qualified}"),
        format!("Modelica.Blocks.Types.{type_name}"),
        format!("Buildings.Controls.OBC.ASHRAE.G36.Types.{type_name}"),
        format!("Buildings.Controls.OBC.CDL.Types.{type_name}"),
        qualified.to_lowercase(),
        qualified.to_uppercase(),
        format!(" {qualified}"),
        format!("{qualified} "),
        format!("{qualified}\n"),
        format!("{qualified}."),
        format!(".{qualified}"),
        qualified.replacen('.', "..", 1),
        qualified.replacen('.', " .", 1),
        format!("{qualified}\u{200B}"),
    ];
    // The CDL or G36 spelling that is the canonical one is not a refusal case.
    out.retain(|spelling| spelling != qualified);
    out
}

#[test]
fn every_row_resolves_by_its_canonical_path_and_no_other_spelling() {
    for row in enumeration_classes() {
        assert_eq!(
            enumeration_class_by_path(row.qualified_path),
            Some(row),
            "{}",
            row.qualified_path
        );
        for spelling in non_canonical(row.qualified_path, row.type_name()) {
            assert_eq!(
                enumeration_class_by_path(&spelling),
                None,
                "{spelling:?} must not name {}",
                row.qualified_path
            );
            assert_eq!(enumeration_class_by_iri(&spelling), None, "{spelling:?}");
            assert_eq!(enum_class_id(&spelling), None, "{spelling:?}");
            assert!(!is_g36_integer_constant_package(&spelling), "{spelling:?}");
            let hint = enumeration_spelling_hint(&spelling);
            assert!(
                hint.is_none_or(|hinted| hinted.type_name() == row.type_name()),
                "{spelling:?} hinted {hint:?}"
            );
        }
        let id = row.id;
        assert_eq!(
            enum_class_id(row.qualified_path),
            id.filter(|_| row.kind == EnumerationKind::Enumeration)
        );
    }
}

#[test]
fn the_issue_spellings_of_simple_controller_pd_name_no_member() {
    let canonical = "Buildings.Controls.OBC.CDL.Types.SimpleController.PD";
    let (row, member) = enumeration_member(canonical).expect("canonical");
    assert_eq!(
        (row.id, member),
        (Some(EnumClassId::SIMPLE_CONTROLLER), "PD")
    );
    assert_eq!(
        SimpleController::from_qualified(canonical),
        Some(SimpleController::Pd)
    );
    for written in [
        "SimpleController.PD",
        "Foo.SimpleController.PD",
        "Buildings.Controls.OBC.ASHRAE.G36.Types.SimpleController.PD",
        "Modelica.Blocks.Types.SimpleController.PD",
        "CDL.Types.SimpleController.PD",
        "buildings.controls.obc.cdl.types.SimpleController.PD",
        "Buildings.Controls.OBC.CDL.Types.simplecontroller.PD",
        " Buildings.Controls.OBC.CDL.Types.SimpleController.PD",
        "http://example.org#Buildings.Controls.OBC.CDL.Types.SimpleController.PD",
    ] {
        assert_eq!(enumeration_member(written), None, "{written:?}");
        assert_eq!(
            SimpleController::from_qualified(written),
            None,
            "{written:?}"
        );
        let (class, _) = written.rsplit_once('.').unwrap();
        // As a typing IRI the namespaced form names the class; as a value it is refused above.
        if !class.contains('#') {
            assert_eq!(enum_class_id(class), None, "{written:?}");
        }
        // The refusal names the canonical class, never another.
        let hint = enumeration_spelling_hint(class).map(|row| row.qualified_path);
        assert!(
            hint.is_none_or(|path| path == "Buildings.Controls.OBC.CDL.Types.SimpleController"),
            "{written:?}: {hint:?}"
        );
    }
    // Canonical class, wrong member; wrong case of the member; empty member.
    for written in [
        "Buildings.Controls.OBC.CDL.Types.SimpleController.XX",
        "Buildings.Controls.OBC.CDL.Types.SimpleController.pd",
        "Buildings.Controls.OBC.CDL.Types.SimpleController.",
        "Buildings.Controls.OBC.CDL.Types.SimpleController.PD ",
    ] {
        assert_eq!(enumeration_member(written), None, "{written:?}");
    }
}

#[test]
fn the_zero_time_parser_reads_only_the_canonical_path() {
    assert_eq!(
        ZeroTime::from_qualified("Buildings.Controls.OBC.CDL.Types.ZeroTime.NY2017"),
        Some(ZeroTime::NewYear(2017))
    );
    for written in [
        "ZeroTime.NY2017",
        "CDL.Types.ZeroTime.NY2017",
        "Modelica.Blocks.Types.ZeroTime.NY2017",
        "Buildings.Controls.OBC.CDL.Typos.ZeroTime.NY2017",
        "Buildings.Controls.OBC.CDL.Types.Smoothness.NY2017",
    ] {
        assert_eq!(ZeroTime::from_qualified(written), None, "{written:?}");
    }
}

/// The G36 Integer constant packages (`OperationModes`, `ZoneStates`, `FreezeProtectionStages`,
/// `DemandLimitLevels`) are named by the same canonical rule as an enumeration: the full path of
/// the package, then the member, as written. They are never an enumeration class.
#[test]
fn a_g36_integer_constant_is_named_by_its_package_canonical_path_only() {
    let package = "Buildings.Controls.OBC.ASHRAE.G36.Types";
    for (constant, value) in [
        ("OperationModes.occupied", 1),
        ("OperationModes.unoccupied", 7),
        ("ZoneStates.cooling", 3),
        ("FreezeProtectionStages.stage2", 2),
        ("DemandLimitLevels.heating3", 3),
    ] {
        let canonical = format!("{package}.{constant}");
        assert_eq!(g36_integer_constant(&canonical), Some(value), "{canonical}");
        let (type_path, _) = canonical.rsplit_once('.').unwrap();
        assert_eq!(
            enum_class_id(type_path),
            None,
            "a package is never an enumeration"
        );
        assert!(is_g36_integer_constant_package(type_path));
        assert!(is_g36_integer_constant_package(&format!(
            "http://example.org#{type_path}"
        )));
        for written in [
            constant.to_owned(),
            format!("ASHRAE.G36.Types.{constant}"),
            format!("Foo.{constant}"),
            format!("Buildings.Controls.OBC.CDL.Types.{constant}"),
            format!("http://example.org#{canonical}"),
            format!("x/{canonical}"),
            canonical.to_lowercase(),
            format!(" {canonical}"),
        ] {
            assert_eq!(g36_integer_constant(&written), None, "{written:?}");
        }
    }
}

#[test]
fn a_typing_iri_names_a_class_by_the_canonical_path_of_its_fragment() {
    for (iri, expected) in [
        (
            "http://example.org#Buildings.Controls.OBC.CDL.Types.ZeroTime",
            Some(EnumClassId::ZERO_TIME),
        ),
        (
            "http://data.ashrae.org/S231#Buildings.Controls.OBC.CDL.Types.ZeroTime",
            Some(EnumClassId::ZERO_TIME),
        ),
        (
            "https://example.org/Buildings.Controls.OBC.ASHRAE.G36.Types.CoolingCoil",
            Some(EnumClassId::G36_COOLING_COIL),
        ),
        // Issue #637's reproduction: a typo in the package.
        (
            "http://example.org#Buildings.Controls.OBC.CDL.Typos.ZeroTime",
            None,
        ),
        ("http://example.org#ZeroTime", None),
        ("http://example.org#Foo.ZeroTime", None),
        (
            "http://example.org#Modelica.Blocks.Types.SimpleController",
            None,
        ),
        (
            "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.Types.SimpleController",
            None,
        ),
        (
            "http://example.org#Buildings.Controls.OBC.CDL.Types.CoolingCoil",
            None,
        ),
    ] {
        assert_eq!(enum_class_id(iri), expected, "{iri}");
        assert_eq!(
            g36_enum_class_id(iri),
            expected.filter(|id| id.0 > 100),
            "{iri}"
        );
    }
}

#[test]
fn a_durable_class_path_is_compared_exactly() {
    // The durable form a blob or snapshot records is relative for CDL and full for G36; it is a
    // recorded identity, not document text, and only its exact spelling decodes.
    assert!(enum_descriptor_by_path("CDL.Types.SimpleController").is_some());
    assert!(
        enum_descriptor_by_path("Buildings.Controls.OBC.ASHRAE.G36.Types.CoolingCoil").is_some()
    );
    for path in [
        "SimpleController",
        "Buildings.Controls.OBC.CDL.Types.SimpleController",
        "Foo.CDL.Types.SimpleController",
        "ASHRAE.G36.Types.CoolingCoil",
        "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.Types.CoolingCoil",
        "Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes",
        "",
    ] {
        assert_eq!(enum_descriptor_by_path(path), None, "{path:?}");
    }
}

/// xorshift64*: deterministic, no dependency.
struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

/// Random edits of canonical paths and members: every lookup equals an independent oracle (a
/// string compared with each row's full path), a refused spelling never resolves, and every
/// lookup is total.
#[test]
fn every_random_edit_of_a_canonical_spelling_resolves_exactly_when_it_is_canonical() {
    const ALPHABET: &[u8] = b"._#/ aAbBzZ0SimpleControllerTypesCDLG36";
    let rows = enumeration_classes();
    let mut rng = XorShift(0x0762_0637_2026_1122);
    let mut resolved = 0usize;
    for _ in 0..20_000 {
        let row = &rows[(rng.next() % rows.len() as u64) as usize];
        let member = row.members[(rng.next() % row.members.len() as u64) as usize];
        let mut text = format!("{}.{member}", row.qualified_path).into_bytes();
        for _ in 0..(rng.next() % 3) {
            let at = (rng.next() % (text.len() as u64 + 1)) as usize;
            let byte = ALPHABET[(rng.next() % ALPHABET.len() as u64) as usize];
            match rng.next() % 3 {
                0 if at < text.len() => text[at] = byte,
                1 if at < text.len() => {
                    text.remove(at);
                }
                _ => text.insert(at, byte),
            }
        }
        let text = String::from_utf8(text).expect("ASCII edits");
        let oracle = text.rsplit_once('.').and_then(|(class, member)| {
            rows.iter()
                .find(|row| row.qualified_path == class && row.members.contains(&member))
                .map(|row| (row.qualified_path, member.to_owned()))
        });
        let actual = enumeration_member(&text).map(|(row, m)| (row.qualified_path, m.to_owned()));
        assert_eq!(actual, oracle, "{text:?}");
        resolved += usize::from(actual.is_some());
    }
    assert!(resolved > 1_000, "{resolved} canonical spellings survived");
}
