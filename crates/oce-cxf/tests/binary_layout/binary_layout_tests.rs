//! Controls for the layout check: each defect it reports, and each declaration form it reads,
//! on a synthetic manifest and source tree.

use std::collections::BTreeMap;

use super::layout_defects;

/// A manifest with `roots` as `[[test]]` targets, each at `tests/<root>.rs`, discovery off.
fn manifest_of(roots: &[&str]) -> String {
    let mut manifest = String::from("[package]\nautotests = false\n");
    for root in roots {
        manifest.push_str(&format!(
            "\n[[test]]\nname = \"{root}\"\npath = \"tests/{root}.rs\"\n"
        ));
    }
    manifest
}

/// Sources for `files` (each `tests/<name>.rs`, or `tests/<name>` when it ends in `.rs`),
/// overridden by `texts` as `(name, text)`.
fn sources_of(files: &[&str], texts: &[(&str, &str)]) -> BTreeMap<String, String> {
    let key = |name: &str| {
        if name.ends_with(".rs") {
            format!("tests/{name}")
        } else {
            format!("tests/{name}.rs")
        }
    };
    let mut sources: BTreeMap<String, String> = files
        .iter()
        .map(|name| (key(name), String::new()))
        .collect();
    for (name, text) in texts {
        sources.insert(key(name), (*text).to_owned());
    }
    sources
}

#[test]
fn the_layout_check_accepts_a_complete_layout() {
    let sources = sources_of(
        &["root", "member", "alone", "helper/mod.rs"],
        &[(
            "root",
            "#[path = \"support/helper.rs\"]\nmod renamed;\nmod member;\nmod helper;\n",
        )],
    );
    let manifest = format!(
        "{}\n[[test]]\nname = \"alone\"\n",
        manifest_of(&["root"]).replace("autotests = false", "autotests = false # off")
    );
    assert_eq!(
        layout_defects(&manifest, &sources),
        Vec::<String>::new(),
        "a target with no path is tests/<name>.rs, and a comment may follow `false`"
    );
}

#[test]
fn the_layout_check_refuses_a_file_compiled_into_no_binary() {
    let mut sources = sources_of(&["root", "member", "orphan", "helper"], &[]);
    sources.insert("tests/nested/main.rs".into(), String::new());
    sources.insert(
        "tests/root.rs".into(),
        // A helper at another path does not compile the top-level file of the same name.
        "#[path = \"support/helper.rs\"]\nmod helper;\nmod member;\n".into(),
    );
    assert_eq!(
        layout_defects(&manifest_of(&["root"]), &sources),
        [
            "tests/helper.rs is never compiled",
            "tests/nested/main.rs is never compiled",
            "tests/orphan.rs is never compiled",
        ]
    );
}

#[test]
fn the_layout_check_refuses_a_file_compiled_twice() {
    // A target that another target also declares as a module.
    let sources = sources_of(&["root", "alone"], &[("root", "mod alone;\n")]);
    assert_eq!(
        layout_defects(&manifest_of(&["root", "alone"]), &sources),
        ["tests/alone.rs is compiled 2 times: a [[test]] target, a module of root"]
    );

    // A file compiled through `#[path]` in one root and as a plain module in another.
    let sources = sources_of(
        &["first", "second", "shared"],
        &[
            ("first", "#[path = \"shared.rs\"]\nmod renamed;\n"),
            ("second", "mod shared;\n"),
        ],
    );
    assert_eq!(
        layout_defects(&manifest_of(&["first", "second"]), &sources),
        ["tests/shared.rs is compiled 2 times: a module of first, a module of second"]
    );

    // The same file twice in one root, and by a path that climbs out of `tests/` and back.
    let sources = sources_of(
        &["root", "twice"],
        &[(
            "root",
            "mod twice;\n#[path = \"../tests/twice.rs\"]\nmod again;\n",
        )],
    );
    assert_eq!(
        layout_defects(&manifest_of(&["root"]), &sources),
        ["tests/twice.rs is compiled 2 times: a module of root, a module of root"]
    );

    // A target named twice, and a module reached through another module's `#[path]`.
    let sources = sources_of(
        &["root", "inner", "leaf"],
        &[
            ("root", "mod inner;\n"),
            ("inner", "#[path = \"leaf.rs\"]\nmod leaf;\n"),
        ],
    );
    let manifest = format!(
        "{}\n[[test]]\nname = \"again\"\npath = \"tests/leaf.rs\"\n",
        manifest_of(&["root"])
    );
    assert_eq!(
        layout_defects(&manifest, &sources),
        ["tests/leaf.rs is compiled 2 times: a [[test]] target, a module of root"]
    );

    // A test file reached a second time through a helper directory's `mod.rs`.
    let sources = sources_of(
        &["root", "member", "helper/mod.rs"],
        &[
            ("root", "mod helper;\nmod member;\n"),
            ("helper/mod.rs", "#[path = \"../member.rs\"]\nmod member;\n"),
        ],
    );
    assert_eq!(
        layout_defects(&manifest_of(&["root"]), &sources),
        ["tests/member.rs is compiled 2 times: a module of root, a module of root"]
    );
}

#[test]
fn the_layout_check_resolves_nested_declarations_as_the_compiler_does() {
    // A file reached through `#[path]` owns its directory, as a `mod.rs` does; any other module
    // file looks in a directory named for it. Each chain ends in a `#[path]` to a test file, which
    // is compiled only when every step resolved.
    let sources = sources_of(
        &[
            "root",
            "via_path",
            "via_mod_rs",
            "via_file",
            "support/carrier.rs",
            "support/inner.rs",
            "holder/mod.rs",
            "holder/next.rs",
            "member.rs",
            "member/child/mod.rs",
        ],
        &[
            (
                "root",
                "#[path = \"support/carrier.rs\"]\nmod carrier;\nmod holder;\nmod member;\n",
            ),
            ("support/carrier.rs", "mod inner;\n"),
            ("support/inner.rs", "#[path = \"../via_path.rs\"]\nmod x;\n"),
            ("holder/mod.rs", "mod next;\n"),
            ("holder/next.rs", "#[path = \"../via_mod_rs.rs\"]\nmod x;\n"),
            ("member.rs", "mod child;\n"),
            (
                "member/child/mod.rs",
                "#[path = \"../../via_file.rs\"]\nmod x;\n",
            ),
        ],
    );
    assert_eq!(
        layout_defects(&manifest_of(&["root"]), &sources),
        Vec::<String>::new()
    );
}

#[test]
fn the_layout_check_reads_declarations_as_the_compiler_does() {
    // Each of these is a declaration the compiler honours, so each file counts as compiled. A
    // `cfg` gate counts as compiled on every target (see `declarations`).
    let sources = sources_of(
        &[
            "root",
            "public",
            "restricted",
            "trailing",
            "same_line_a",
            "same_line_b",
            "gated",
            "raw_name",
            "inner_gate",
        ],
        &[
            (
                "root",
                r#"#![doc = "a root may begin with inner attributes"]
pub mod public;
pub(crate) mod restricted;
mod trailing; // a note after the semicolon
mod same_line_a; mod same_line_b;
#[cfg(windows)]
mod gated;
mod r#raw_name;
mod inner_gate;
"#,
            ),
            ("inner_gate", "#![cfg(windows)]\n"),
        ],
    );
    assert_eq!(
        layout_defects(&manifest_of(&["root"]), &sources),
        Vec::<String>::new()
    );

    // Each of these is not a declaration, so each file is still in no binary.
    let sources = sources_of(
        &["root", "line", "block", "nested", "string", "raw"],
        &[(
            "root",
            r##"// mod line;
/* mod block; */
/* outer /* inner */ mod nested; */
const TEXT: &str = "mod string;";
const RAW: &str = r#"mod raw; "quoted""#;
const QUOTE: char = '"'; const TICK: char = '\'';
fn lifetime<'a>(text: &'a str) -> &'a str { text }
"##,
        )],
    );
    assert_eq!(
        layout_defects(&manifest_of(&["root"]), &sources),
        [
            "tests/block.rs is never compiled",
            "tests/line.rs is never compiled",
            "tests/nested.rs is never compiled",
            "tests/raw.rs is never compiled",
            "tests/string.rs is never compiled",
        ]
    );
}

#[test]
fn the_layout_check_refuses_declarations_it_cannot_place() {
    // A target-dependent path, a module inside an inline module and one inside a function body:
    // the check refuses each instead of guessing, even where its guess would pass.
    let sources = sources_of(
        &["root", "member"],
        &[(
            "root",
            "#[cfg_attr(unix, path = \"member.rs\")]\nmod member;\n\
             mod inline { mod hidden; }\n\
             fn body() { #[path = \"member.rs\"] mod again; }\n",
        )],
    );
    assert_eq!(
        layout_defects(&manifest_of(&["root"]), &sources),
        [
            "tests/root.rs: `mod again;` cannot be placed by the layout check: it is declared \
             inside braces",
            "tests/root.rs: `mod hidden;` cannot be placed by the layout check: it is declared \
             inside braces",
            "tests/root.rs: `mod member;` cannot be placed by the layout check: its path is under \
             `cfg_attr`",
            "tests/member.rs is never compiled",
        ]
    );
}

#[test]
fn the_layout_check_refuses_discovery_left_on_a_missing_target_and_no_target() {
    let sources = sources_of(&["root"], &[]);
    let refusal = ["Cargo.toml: autotests is not false, so Cargo builds every file alone"];
    let discovered = manifest_of(&["root"]).replace("autotests = false\n", "");
    assert_eq!(layout_defects(&discovered, &sources), refusal);
    for spelling in ["falsey", "\"false\"", "true", "false_", "nofalse"] {
        let manifest =
            manifest_of(&["root"]).replace("autotests = false", &format!("autotests = {spelling}"));
        assert_eq!(layout_defects(&manifest, &sources), refusal, "{spelling}");
    }
    let other_table = "[package]\nname = \"x\"\n[lints]\nautotests = false\n";
    assert_eq!(layout_defects(other_table, &sources).len(), 3);
    let missing = format!("{}\n[[test]]\nname = \"gone\"\n", manifest_of(&["root"]));
    assert_eq!(
        layout_defects(&missing, &sources),
        ["[[test]] gone: tests/gone.rs is not a file under tests/"]
    );
}
