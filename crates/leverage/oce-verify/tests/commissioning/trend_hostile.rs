//! Hostile bytes in every file a trend read takes: never a panic, always a listed code with a
//! bounded sentence, and memory within a stated bound of the bytes handed in.
//!
//! The bound, for one source: at most 48 bytes held per byte of the file, plus 64 KiB of fixed
//! overhead (replay's 8 KiB read chunk, a matrix's slot table of two bytes per declared column,
//! the mapping read). By construction a cell takes at least one byte of the file (its separator)
//! and is held as at most 16 bytes while its file is read (8, in a list whose capacity at most
//! doubles) and 9 once merged, so 25 bytes; the densest cases here hold 14.4 a byte past the
//! overhead. This is a bound per source: with many input sources the merged columns grow as
//! frames times columns, which `trend_cost.rs` pins against `max_source_bytes`.

use oce_verify::{
    MAX_SPEC_DETAIL_BYTES, SOURCE_ERROR_CODES, SpecInputs, TrendLimits, TrendSources,
};

use super::support::Sequence;
use super::{loaded, point, trend_file};

const PER_BYTE: u64 = 48;
const OVERHEAD: u64 = 64 * 1024;

/// A reference reading `f` as one source of `kind`, and `m.json` as its mapping.
fn spec(kind: &str) -> oce_verify::CommissioningSpec {
    let source = match kind {
        "combi" => {
            r#""inputs": [{"path": "f", "format": "combiTimeTable", "timeColumn": 1,
            "columns": {"2": "A", "3": "B"}, "hold": {"maxAge": 5}}]"#
        }
        "csv" => {
            r#""expected": [{"path": "f", "format": "csv", "timeColumn": "time",
            "columns": {"A": "A", "B": "B"}}]"#
        }
        "csv-inputs" => {
            r#""inputs": [{"path": "f", "format": "csv", "timeColumn": "time",
            "columns": {"A": "A", "B": "B"}}]"#
        }
        _ => r#""controllerOutput": "f""#,
    };
    loaded(&format!(
        r#"{{"time": {{"origin": "2018-06-07T00:00:00Z"}}, "references": [{{"model": "M",
            "sequence": "s", "pointNameMapping": "m.json", {source}}}]}}"#
    ))
}

fn mapping() -> Vec<u8> {
    format!(
        "[{}, {}]",
        point(("a", Some("K"), "float"), ("A", Some("degF"), "float")),
        point(("b", None, "bool"), ("B", None, "float")),
    )
    .into_bytes()
}

/// Read `bytes` as file `f` of `spec`, and check the bounds a hostile file is held to.
fn check(spec: &oce_verify::CommissioningSpec, bytes: &[u8], worst: &mut f64) {
    let inputs = SpecInputs {
        cxf: Vec::new(),
        files: vec![
            ("m.json".to_owned(), mapping()),
            ("f".to_owned(), bytes.to_vec()),
        ],
    };
    let mut result = None;
    let info = allocation_counter::measure(|| {
        result = Some(TrendSources::read(
            spec,
            0,
            &inputs,
            &TrendLimits::default(),
        ));
    });
    if let Err(error) = result.expect("ran") {
        assert!(
            SOURCE_ERROR_CODES.contains(&error.code()),
            "{}",
            error.code()
        );
        for text in [
            error.detail(),
            error.file().unwrap_or(""),
            error.column().unwrap_or(""),
        ] {
            assert!(text.len() <= MAX_SPEC_DETAIL_BYTES);
            assert!(text.bytes().all(|b| (b' '..=b'~').contains(&b)), "{text:?}");
        }
    }
    let length = bytes.len() as u64;
    let held = info.bytes_max.saturating_sub(OVERHEAD);
    *worst = worst.max(held as f64 / length.max(1) as f64);
    assert!(
        info.bytes_max <= PER_BYTE * length + OVERHEAD,
        "{info:?} for {length} bytes: {:?}",
        String::from_utf8_lossy(&bytes[..bytes.len().min(200)])
    );
}

/// Seeds: well-formed files of each kind, and shapes that stress one bound each.
fn seeds() -> Vec<(&'static str, Vec<u8>)> {
    let mut seeds = vec![
        (
            "combi",
            b"#1\ndouble t(3,3)\n0 50 1\n5 51 0\n5 52 1\n".to_vec(),
        ),
        (
            "csv",
            b"time,A,B\n0,50,1\n5,,0\n2018-06-07T00:00:10Z,52,1\n".to_vec(),
        ),
        ("controller", b"time,A,B\n0,50,1\n5,51,0\n".to_vec()),
        ("csv", trend_file("status.csv")),
        ("combi", trend_file("ahu.mos")),
        ("controller", trend_file("controller_output.csv")),
        ("combi", b"#1\ndouble t(16777216,4096)\n".to_vec()),
        (
            "combi",
            format!("#1\n{}", "double t(1,1)\n0\n".repeat(2_000)).into_bytes(),
        ),
        (
            "combi",
            format!("#1\ndouble t(1,3)\n{}", " ".repeat(100_000)).into_bytes(),
        ),
        (
            "csv",
            format!("time,A,B\n{}", ",".repeat(100_000)).into_bytes(),
        ),
        (
            "csv",
            format!("time,A,B\n\"{}", "x".repeat(100_000)).into_bytes(),
        ),
        (
            "controller",
            format!("\"{}", "\"\"".repeat(50_000)).into_bytes(),
        ),
        (
            "controller",
            format!("time,{}\n", vec!["A"; 5_000].join(",")).into_bytes(),
        ),
    ];
    let mut rows = String::from("#1\ndouble t(20000,3)\n");
    for row in 0..20_000 {
        rows.push_str(&format!("{row} 1 0\n"));
    }
    seeds.push(("combi", rows.into_bytes()));
    // The densest shapes: the fewest bytes per value kept.
    let dense = format!("#1\ndouble t(50000,3)\n{}", "0 0 0\n".repeat(50_000));
    seeds.push(("combi", dense.into_bytes()));
    let dense = format!("time,A,B\n{}", "0,,\n".repeat(50_000));
    seeds.push(("csv-inputs", dense.into_bytes()));
    let dense = format!("time,A,B\n{}", "0,0,0\n".repeat(50_000));
    seeds.push(("csv", dense.into_bytes()));
    seeds
}

#[test]
fn arbitrary_bytes_in_any_trend_file_never_panic_and_stay_within_the_memory_bound() {
    let specs = [
        ("combi", spec("combi")),
        ("csv", spec("csv")),
        ("csv-inputs", spec("csv-inputs")),
        ("controller", spec("controller")),
    ];
    let mut worst = 0.0_f64;
    let mut sequence = Sequence(0x0660);
    // Random bytes, from an alphabet that hits the grammars' own characters often.
    let alphabet: &[u8] = b"#1double t(,);\n\r\t 0123456789.eE+-\"xinfaT:Z\xff\xef\xbb\xbf";
    for round in 0..600 {
        let length = sequence.below(512);
        let bytes: Vec<u8> = (0..length)
            .map(|_| {
                if round % 3 == 0 {
                    sequence.next() as u8
                } else {
                    alphabet[sequence.below(alphabet.len())]
                }
            })
            .collect();
        for (_, spec) in &specs {
            check(spec, &bytes, &mut worst);
        }
    }
    // Each seed as it is, then mutated: bytes flipped, inserted, removed and cut.
    for (kind, seed) in seeds() {
        let spec = &specs
            .iter()
            .find(|(name, _)| *name == kind)
            .expect("a spec")
            .1;
        check(spec, &seed, &mut worst);
        for _ in 0..40 {
            let mut bytes = seed.clone();
            for _ in 0..=sequence.below(4) {
                if bytes.is_empty() {
                    break;
                }
                let at = sequence.below(bytes.len());
                match sequence.below(4) {
                    0 => bytes[at] = sequence.next() as u8,
                    1 => bytes.insert(at, alphabet[sequence.below(alphabet.len())]),
                    2 => {
                        bytes.remove(at);
                    }
                    _ => bytes.truncate(at),
                }
            }
            check(spec, &bytes, &mut worst);
        }
    }
    // [worst 14.4 bytes held per byte past the overhead, on a csv input of empty cells]
    assert!(worst < 25.0, "{worst}");
}
