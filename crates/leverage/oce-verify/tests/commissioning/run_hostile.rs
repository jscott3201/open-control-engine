//! Arbitrary spec, mapping and trend bytes never panic a run end to end (issues #662 and #663):
//! every outcome is a receipt or a refusal whose code is listed, and whose cause, when it has
//! one, is a code of the layer it names. Two generators, each seeded so every run draws the same
//! cases: byte edits of the run fixture's files, and well-formed fixtures with hostile numbers
//! (huge, tiny, negative and subnormal readings, windows and samplings, tolerances).

use oce_verify::{
    COMPARE_REFUSAL_CAUSES, CommissionOptions, CommissioningSpec, ENGINE_REFUSAL_CLASSES,
    PARAM_REFUSAL_CLASSES, REPLAY_FAILURE_CAUSES, SOURCE_ERROR_CODES, SPEC_ERROR_CODES, SpecLimits,
    UNIT_ERROR_CODES, VERIFY_ERROR_CODES, VerifyError, run_commissioning,
};

use super::support::Sequence;
use super::{run_file, run_inputs};

/// Check one outcome: a receipt that writes and identifies itself, or a listed refusal.
fn check(outcome: Result<oce_verify::CommissionRun, VerifyError>, case: &str) {
    match outcome {
        Ok(run) => {
            let json = run.receipt.to_canonical_json();
            assert!(json.is_ascii(), "{case}");
            assert_eq!(
                run.receipt.header.receipt_id,
                run.receipt.computed_receipt_id(),
                "{case}"
            );
            assert_eq!(
                run.curves.len(),
                match &run.receipt.body {
                    oce_verify::ReceiptBody::Commissioning(body) => body.outputs.len(),
                    _ => panic!("{case}: a commissioning body"),
                }
            );
        }
        Err(error) => {
            assert!(
                VERIFY_ERROR_CODES.contains(&error.code()),
                "{case}: {}",
                error.code()
            );
            if let Some(cause) = error.cause() {
                let tables: &[&[&str]] = &[
                    SOURCE_ERROR_CODES,
                    SPEC_ERROR_CODES,
                    UNIT_ERROR_CODES,
                    COMPARE_REFUSAL_CAUSES,
                    ENGINE_REFUSAL_CLASSES,
                    PARAM_REFUSAL_CLASSES,
                    REPLAY_FAILURE_CAUSES,
                ];
                assert!(
                    tables.iter().any(|table| table.contains(&cause)),
                    "{case}: {} cause {cause}",
                    error.code()
                );
            }
            assert!(!error.detail().is_empty(), "{case}");
        }
    }
}

/// `bytes` with a few random edits: a byte replaced by one of a hostile alphabet, a byte
/// inserted, a run deleted, or the tail cut.
fn mutated(bytes: &[u8], random: &mut Sequence) -> Vec<u8> {
    const ALPHABET: &[u8] = b"0123456789-+.eE,;:\"{}[]\n \t#*?/\\aZ\x00\xff";
    let mut out = bytes.to_vec();
    for _ in 0..=random.below(4) {
        if out.is_empty() {
            out.push(ALPHABET[random.below(ALPHABET.len())]);
            continue;
        }
        let at = random.below(out.len());
        match random.below(4) {
            0 => out[at] = ALPHABET[random.below(ALPHABET.len())],
            1 => out.insert(at, ALPHABET[random.below(ALPHABET.len())]),
            2 => {
                let end = (at + 1 + random.below(8)).min(out.len());
                out.drain(at..end);
            }
            _ => out.truncate(at),
        }
    }
    out
}

#[test]
fn byte_edits_of_any_file_of_a_run_never_panic() {
    let mut random = Sequence(0x0662_0663);
    let names = [
        "spec.json",
        "points.json",
        "inputs.csv",
        "outputs.csv",
        "fan.csv",
        "cxf",
    ];
    let mut ran = [0_usize; 2];
    for case in 0..600 {
        let target = names[random.below(names.len())];
        let mut spec_bytes = run_file("spec.json");
        let mut inputs = run_inputs(&[]);
        match target {
            "spec.json" => spec_bytes = mutated(&spec_bytes, &mut random),
            "cxf" => inputs.cxf = mutated(&inputs.cxf, &mut random),
            file => {
                for (path, bytes) in &mut inputs.files {
                    if path == file {
                        *bytes = mutated(bytes, &mut random);
                    }
                }
            }
        }
        let Ok(spec) = CommissioningSpec::from_json_bytes(&spec_bytes, &SpecLimits::default())
        else {
            continue;
        };
        // Now and then a reference past the end.
        let reference = usize::from(random.below(10) == 0);
        let outcome = run_commissioning(&spec, reference, &inputs, &CommissionOptions::default());
        ran[usize::from(outcome.is_ok())] += 1;
        check(outcome, &format!("case {case}, {target}"));
    }
    assert!(
        ran[0] > 100 && ran[1] > 10,
        "both outcomes are drawn: {ran:?}"
    );
}

/// A hostile real: huge, tiny, negative, subnormal or ordinary.
fn hostile_number(random: &mut Sequence) -> String {
    match random.below(9) {
        0 => "1e308".to_owned(),
        1 => "-1e308".to_owned(),
        2 => "5e-324".to_owned(),
        3 => "0".to_owned(),
        4 => "-0".to_owned(),
        5 => "1e15".to_owned(),
        6 => format!("{}", random.below(1000)),
        7 => format!("-{}.{}", random.below(100), random.below(100)),
        _ => format!("{}e{}", random.below(9) + 1, random.below(40)),
    }
}

#[test]
fn hostile_numbers_in_well_formed_files_never_panic() {
    let mut random = Sequence(0x7e70_0000);
    for case in 0..300 {
        let mut inputs_csv = String::from("time,Supply Temp,Offset,Enable,Stage,Limit\n");
        let mut outputs_csv = String::from("time,Setpoint,Enabled,Staged,Limited\n");
        let mut t = 0_u32;
        for _ in 0..=random.below(6) {
            inputs_csv.push_str(&format!(
                "{t},{},{},{},{},{}\n",
                hostile_number(&mut random),
                hostile_number(&mut random),
                random.below(2) == 0,
                random.below(5),
                hostile_number(&mut random),
            ));
            outputs_csv.push_str(&format!(
                "{t},{},{},{},{}\n",
                hostile_number(&mut random),
                random.below(2) == 0,
                random.below(5),
                hostile_number(&mut random),
            ));
            t += u32::try_from(random.below(20)).expect("small");
        }
        let start = hostile_number(&mut random);
        let stop = hostile_number(&mut random);
        let sampling = match random.below(3) {
            0 => String::new(),
            _ => format!(
                r#""sampling": {},"#,
                hostile_number(&mut random).trim_start_matches('-')
            ),
        };
        let envelope = ["funnel", "aligned-grid"][random.below(2)];
        let role = ["engine", "trend"][random.below(2)];
        let tolerance =
            |random: &mut Sequence| hostile_number(random).trim_start_matches('-').to_owned();
        let text = String::from_utf8(run_file("spec.json"))
            .expect("UTF-8")
            .replace(
                r#""window": { "start": 0, "stop": 30 },"#,
                &format!(
                    r#""window": {{ "start": {start}, "stop": {stop} }}, {sampling} "envelope": "{envelope}", "funnelReference": "{role}","#
                ),
            )
            .replace(
                r#""tolerances": { "atolx": 1, "atoly": 0.01 },"#,
                &format!(
                    r#""tolerances": {{ "atolx": {}, "atoly": {}, "rtoly": {}, "ltolx": {} }},"#,
                    tolerance(&mut random),
                    tolerance(&mut random),
                    tolerance(&mut random),
                    ["0", "0.5", "2"][random.below(3)],
                ),
            );
        // A window whose start is after its stop, a sampling of zero and the like are the
        // reader's to refuse.
        let Ok(spec) = CommissioningSpec::from_json_str(&text, &SpecLimits::default()) else {
            continue;
        };
        let inputs = run_inputs(&[
            ("inputs.csv", inputs_csv.as_bytes()),
            ("outputs.csv", outputs_csv.as_bytes()),
        ]);
        let outcome = run_commissioning(&spec, 0, &inputs, &CommissionOptions::default());
        check(
            outcome,
            &format!("case {case}: {text}\n{inputs_csv}\n{outputs_csv}"),
        );
    }
}
