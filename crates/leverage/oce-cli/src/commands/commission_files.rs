//! The files one commissioning reference names, read for `oce verify --commission` (commissioning
//! note, "CLI" "Reads" and decision 25; issue #664). The library opens no file: this module
//! resolves every path and hands `oce_verify::run_commissioning` the bytes.
//!
//! **Which files.** The reference's `cxf`; its `pointNameMapping` when that is a path; then its
//! trend sources in the receipt's order: each `inputs` source, each `expected` source (or the
//! `controllerOutput` when the reference has no `expected` key at all), each `indicatorSources`
//! source. A path named twice is read once, at its first place, as the library counts it. These
//! are exactly the paths `TrendSources::read` asks for, so a list that drifted from the
//! library's would be refused there (`file-missing`, `file-unlisted`), never run on.
//!
//! **How.** Each path, in that order, is resolved inside the spec's directory and opened once
//! (`super::contained`; a path that resolves outside it is `file-outside-directory`), and its size
//! is judged from that handle's metadata **before** a byte is read:
//!
//! | File | Bound | Refusal |
//! | --- | --- | --- |
//! | the CXF | `oce_api::MAX_CXF_BYTES` (8 MiB) | `input-too-large` |
//! | the mapping | `SpecLimits::MAX.max_bytes` (4 MiB) | `input-too-large` |
//! | a trend source | `TrendLimits::MAX.replay.max_source_bytes` (1 GiB) | `input-too-large` |
//! | the mapping and the sources together, each path once, the CXF not counted | `TrendLimits::MAX.max_total_bytes` (1 GiB) | `source-refused`, cause `too-many-total-bytes` |
//!
//! The bound on the files together is the library's own, counted the library's way, and refused
//! under the library's own word before the bytes are held, so a run the CLI lets through is never
//! refused for it later. The read itself stops one byte past what is still allowed: a file that
//! grew after its metadata was read is read up to that room and one byte more, and then refused.
//! Paths resolve against the directory of the spec's path as given (`dir/spec.json` gives `dir`),
//! so a spec reached through a symbolic link reads the files beside the link, not beside its
//! target; that directory is the handle the spec itself was read through (issue #771), so a
//! folder exchanged during the run cannot pair the spec with another folder's files. Every refusal is at the spec
//! field that names the file (`references[0].inputs[1].path`) and names the path as the spec
//! writes it, never a host path.

use oce_verify::{
    PointNameMapping, SpecInputs, SpecLimits, SpecReference, TrendLimits, TrendSource,
};

use super::contained::Root;
use super::file_identity::Identity;
use crate::codes;
use crate::input::{read_bounded, too_large};

/// `oce-verify`'s code for a refused trend source, whose cause the bound on the files together
/// reuses.
pub(crate) const SOURCE_REFUSED: &str = "source-refused";

/// The source reader's word for the files of a reference past their bound together.
pub(crate) const TOO_MANY_TOTAL_BYTES: &str = "too-many-total-bytes";

/// What a named file is, which decides its bound and whether it counts toward the total.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// The sequence's CXF.
    Cxf,
    /// The point-mapping file.
    Mapping,
    /// A trend source, or the `controllerOutput` read in place of `expected`.
    Source,
}

impl Kind {
    /// The largest file of this kind that is read.
    fn bound(self) -> u64 {
        match self {
            Self::Cxf => u64::try_from(oce_api::MAX_CXF_BYTES).unwrap_or(u64::MAX),
            Self::Mapping => SpecLimits::MAX.max_bytes,
            Self::Source => TrendLimits::MAX.replay.max_source_bytes,
        }
    }

    /// How a refusal names a file of this kind.
    fn noun(self) -> &'static str {
        match self {
            Self::Cxf => "the CXF",
            Self::Mapping => "the mapping",
            Self::Source => "trend source",
        }
    }
}

/// One file the reference names: the spec field that names it, its path as written, its kind.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Named<'s> {
    /// The place in the spec (`references[0].inputs[1].path`), the refusal's `at`.
    pub(crate) at: String,
    /// The path as the spec writes it.
    pub(crate) path: &'s str,
    kind: Kind,
}

/// A refusal of one file.
#[derive(Debug)]
pub(crate) struct Refused {
    /// The spec field that names the file.
    pub(crate) at: String,
    /// The code.
    pub(crate) code: &'static str,
    /// The sentence, naming the path as written.
    pub(crate) detail: String,
    /// The cause, for a refusal under `source-refused`.
    pub(crate) cause: Option<&'static str>,
}

/// What the reads give.
#[derive(Debug)]
pub(crate) struct Files {
    /// The bytes the library is handed.
    pub(crate) inputs: SpecInputs,
    /// The identity of every file read, for the `--out` guard.
    pub(crate) read: Vec<Identity>,
}

/// The files `reference`, at `index` in its spec, names, in the order they are read (module
/// header).
pub(crate) fn named(index: usize, reference: &SpecReference) -> Vec<Named<'_>> {
    /// Add one file, unless the mapping or a source already names its path: the CXF is never
    /// one of the files the library counts, so a source may share its path.
    fn push<'r>(all: &mut Vec<Named<'r>>, at: String, path: &'r str, kind: Kind) {
        if !all
            .iter()
            .any(|named| named.path == path && named.kind != Kind::Cxf)
        {
            all.push(Named { at, path, kind });
        }
    }
    fn sources<'r>(
        all: &mut Vec<Named<'r>>,
        place: &str,
        list: &str,
        sources: Option<&'r [TrendSource]>,
    ) {
        for (j, source) in sources.unwrap_or_default().iter().enumerate() {
            push(
                all,
                format!("{place}.{list}[{j}].path"),
                source.path(),
                Kind::Source,
            );
        }
    }
    let place = format!("references[{index}]");
    let mut all = Vec::new();
    if let Some(cxf) = reference.cxf() {
        push(&mut all, format!("{place}.cxf"), cxf, Kind::Cxf);
    }
    if let Some(PointNameMapping::Path(path)) = reference.point_name_mapping() {
        push(
            &mut all,
            format!("{place}.pointNameMapping"),
            path,
            Kind::Mapping,
        );
    }
    sources(&mut all, &place, "inputs", reference.inputs());
    match (reference.expected(), reference.controller_output()) {
        (None, Some(path)) => push(
            &mut all,
            format!("{place}.controllerOutput"),
            path,
            Kind::Source,
        ),
        (expected, _) => sources(&mut all, &place, "expected", expected),
    }
    sources(
        &mut all,
        &place,
        "indicatorSources",
        reference.indicator_sources(),
    );
    all
}

/// Read every file of `named`, resolved inside `root`, the spec's directory as it was opened to
/// read the spec, in order; the first refusal stops the reading (module header).
pub(crate) fn read(root: &Root, named: &[Named<'_>]) -> Result<Files, Refused> {
    read_within(root, named, TrendLimits::MAX.max_total_bytes)
}

/// [`read`] with `total_bound` as the bound on the files together (a unit test lowers it).
fn read_within(root: &Root, named: &[Named<'_>], total_bound: u64) -> Result<Files, Refused> {
    let mut total: u64 = 0;
    let mut files = Files {
        inputs: SpecInputs::default(),
        read: Vec::with_capacity(named.len()),
    };
    for file in named {
        let refuse = |(code, why): (&'static str, String)| Refused {
            at: file.at.clone(),
            code,
            detail: format!("{} `{}`: {why}", file.kind.noun(), file.path),
            cause: None,
        };
        let opened = root
            .open(file.path, codes::FILE_OUTSIDE_DIRECTORY, "the spec's")
            .map_err(refuse)?;
        let size = opened.metadata.len();
        let per_file = file.kind.bound();
        if size > per_file {
            return Err(refuse(too_large(
                size,
                usize::try_from(per_file).unwrap_or(usize::MAX),
            )));
        }
        let counted = file.kind != Kind::Cxf;
        let room = if counted {
            if total.saturating_add(size) > total_bound {
                return Err(Refused {
                    cause: Some(TOO_MANY_TOTAL_BYTES),
                    ..refuse((
                        SOURCE_REFUSED,
                        format!(
                            "the mapping and the trend sources up to this one are {} bytes; at \
                             most {total_bound} are read together",
                            total.saturating_add(size)
                        ),
                    ))
                });
            }
            per_file.min(total_bound - total)
        } else {
            per_file
        };
        // The read stops one byte past what is still allowed: a file that grew since its
        // metadata was read is read that far and refused.
        let bytes = read_bounded(opened.file, usize::try_from(room).unwrap_or(usize::MAX))
            .map_err(refuse)?;
        if counted {
            total = total.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
            files.inputs.files.push((file.path.to_owned(), bytes));
        } else {
            files.inputs.cxf = bytes;
        }
        files.read.push(opened.identity);
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oce_verify::CommissioningSpec;

    fn spec(reference: &str) -> CommissioningSpec {
        let text = format!(r#"{{"references": [{{"model": "M", "sequence": "s", {reference}}}]}}"#);
        CommissioningSpec::from_json_str(&text, &SpecLimits::default()).expect("the spec loads")
    }

    fn source(path: &str) -> String {
        format!(
            r#"{{"path": "{path}", "format": "csv", "timeColumn": "t", "columns": {{"a": "a"}}}}"#
        )
    }

    fn listed(reference: &str) -> Vec<(String, String, Kind)> {
        let spec = spec(reference);
        named(0, &spec.references()[0])
            .into_iter()
            .map(|named| (named.at, named.path.to_owned(), named.kind))
            .collect()
    }

    #[test]
    fn the_files_are_the_cxf_the_mapping_then_the_sources_in_receipt_order() {
        let reference = format!(
            r#""cxf": "s.cxf.json", "pointNameMapping": "points.json",
               "inputs": [{}, {}], "expected": [{}], "indicatorSources": [{}],
               "controllerOutput": "ignored.csv""#,
            source("in0.csv"),
            source("in1.csv"),
            source("out.csv"),
            source("fan.csv")
        );
        let at = |field: &str| format!("references[0].{field}");
        assert_eq!(
            listed(&reference),
            [
                (at("cxf"), "s.cxf.json".to_owned(), Kind::Cxf),
                (
                    at("pointNameMapping"),
                    "points.json".to_owned(),
                    Kind::Mapping
                ),
                (at("inputs[0].path"), "in0.csv".to_owned(), Kind::Source),
                (at("inputs[1].path"), "in1.csv".to_owned(), Kind::Source),
                (at("expected[0].path"), "out.csv".to_owned(), Kind::Source),
                (
                    at("indicatorSources[0].path"),
                    "fan.csv".to_owned(),
                    Kind::Source
                ),
            ]
        );
    }

    #[test]
    fn the_controller_output_is_read_only_when_there_is_no_expected_key() {
        let without = r#""cxf": "s.cxf.json", "controllerOutput": "out.csv""#;
        assert_eq!(
            listed(without)[1],
            (
                "references[0].controllerOutput".to_owned(),
                "out.csv".to_owned(),
                Kind::Source
            )
        );
        // An empty `expected` list is still `expected`: the controllerOutput is a finding.
        let empty = r#""cxf": "s.cxf.json", "expected": [], "controllerOutput": "out.csv""#;
        assert_eq!(listed(empty).len(), 1);
    }

    #[test]
    fn a_path_named_twice_is_read_once_at_its_first_place_and_the_cxf_apart() {
        let reference = format!(
            r#""cxf": "a.csv", "pointNameMapping": "a.csv", "inputs": [{}, {}],
               "expected": [{}]"#,
            source("a.csv"),
            source("b.csv"),
            source("b.csv")
        );
        let listed = listed(&reference);
        let paths: Vec<(&str, Kind)> = listed
            .iter()
            .map(|(_, path, kind)| (path.as_str(), *kind))
            .collect();
        assert_eq!(
            paths,
            [
                ("a.csv", Kind::Cxf),
                ("a.csv", Kind::Mapping),
                ("b.csv", Kind::Source)
            ]
        );
        assert_eq!(listed[2].0, "references[0].inputs[1].path");
    }

    #[test]
    fn an_inline_mapping_and_no_sources_name_the_cxf_alone() {
        let reference = r#""cxf": "s.cxf.json", "pointNameMapping": []"#;
        assert_eq!(listed(reference).len(), 1);
    }

    /// A fresh directory for one test, holding `files`, and the path of a spec inside it.
    fn directory(name: &str, files: &[(&str, usize)]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "oce-cli-commission-files-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create the directory");
        for (file, len) in files {
            std::fs::write(dir.join(file), vec![b'x'; *len]).expect("write");
        }
        dir
    }

    fn files_of(reference: &str) -> (CommissioningSpec, String) {
        (spec(reference), reference.to_owned())
    }

    #[test]
    fn the_total_counts_the_mapping_and_the_sources_once_each_and_never_the_cxf() {
        let reference = format!(
            r#""cxf": "s.cxf.json", "pointNameMapping": "m.json", "inputs": [{}, {}]"#,
            source("a.csv"),
            source("a.csv")
        );
        let (spec, _) = files_of(&reference);
        let named = named(0, &spec.references()[0]);
        let dir = directory("total", &[("s.cxf.json", 100), ("m.json", 3), ("a.csv", 4)]);
        std::fs::write(dir.join("spec.json"), b"{}").expect("spec");
        let root = Root::read_document(&dir.join("spec.json"), 64)
            .map_err(|(code, _)| code)
            .expect("the spec's directory")
            .root;
        // 3 + 4 = 7 counted bytes: read at a bound of 7, though the CXF alone is 100.
        let files = read_within(&root, &named, 7).expect("read");
        assert_eq!(files.inputs.cxf.len(), 100);
        assert_eq!(files.inputs.files.len(), 2, "a.csv once");
        assert_eq!(files.read.len(), 3);
        // One byte less, and the source that passes it is refused before its read.
        let refused = read_within(&root, &named, 6).expect_err("past");
        assert_eq!(
            (refused.code, refused.cause, refused.at.as_str()),
            (
                SOURCE_REFUSED,
                Some(TOO_MANY_TOTAL_BYTES),
                "references[0].inputs[0].path"
            )
        );
        assert!(
            refused.detail.starts_with("trend source `a.csv`: "),
            "{}",
            refused.detail
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn each_kind_has_its_bound() {
        assert_eq!(Kind::Cxf.bound(), 8 * 1024 * 1024);
        assert_eq!(Kind::Mapping.bound(), 4 * 1024 * 1024);
        assert_eq!(Kind::Source.bound(), 1 << 30);
        assert_eq!(TrendLimits::MAX.max_total_bytes, 1 << 30);
    }
}
