//! Public-API surface snapshot for `oce-store` — defense-in-depth for the
//! *surface-enumeration* layer (`08` R-PUB-3).
//!
//! `oce-api` re-exports this port crate as `pub use oce_store;`, and the facade's own surface
//! baseline sees that re-export as one opaque line. This test pins `oce_store`'s *exact text*: any
//! unintended addition, removal, or signature change to the store traits, DTOs, or typed errors fails
//! the release gate against a reviewer-blessed baseline (`tests/public-api.txt`).
//!
//! GATE-ONLY. `cargo public-api` needs rustdoc JSON, which needs a nightly toolchain — but the
//! workspace is pinned to stable 1.97.1. So this test is **armed** by the `OCE_PUBLIC_API_NIGHTLY`
//! env var, which only `release-gate.yml` sets (after installing the pinned nightly). When the var
//! is unset — the fast per-PR `ci.yml` gate and local `cargo test` — the test **skips**. When armed,
//! it **must run to completion**: every tooling step is fail-hard, so a broken toolchain turns the
//! gate RED, never a silent green.
//!
//! COMPAT TRIPLE: `public-api` / `rustdoc-json` / nightly are pinned together in `Cargo.toml`.
//! A rustdoc-JSON `format_version` skew is caught because the `rustdoc-types` `Crate` shape changes
//! between versions, so a mismatched JSON fails to deserialize (a serde parse error) — the exact
//! pins, not a version assertion, keep the triple aligned. Re-bless after an *intentional* surface
//! change by running with the nightly and `UPDATE_EXPECT=1` (use `--locked`, matching the gate, so
//! the bless and verify resolve against the same `Cargo.lock`), then review the diff:
//!
//! ```text
//! OCE_PUBLIC_API_NIGHTLY=nightly-2026-05-01 UPDATE_EXPECT=1 \
//!   cargo test -p oce-store --test public_api --locked
//! ```

// The skip notice is written to stderr on purpose; the workspace denies `print_stderr`.
#![allow(clippy::print_stderr)]

/// Env var that arms the snapshot. Its value is the nightly toolchain used to build rustdoc JSON.
const ARM_ENV: &str = "OCE_PUBLIC_API_NIGHTLY";

/// Step-scoped sentinel set ONLY by the dedicated release-gate surface-gate step. When it is set the
/// gate MUST run: an unset/empty `ARM_ENV` becomes a hard panic instead of a silent skip. This turns
/// "the release gate ran but the surface check was silently disarmed" — e.g. a future edit drops
/// `ARM_ENV` from that step — from a false-green into a RED. It is deliberately NOT set on the
/// workspace-wide nextest step, so that unarmed double-run of this same test still skips normally.
const REQUIRE_ENV: &str = "OCE_REQUIRE_SURFACE_CHECK";

const BASELINE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/public-api.txt");

fn difference_window<'a>(line: &'a str, other: &str) -> &'a str {
    let different_at = line
        .bytes()
        .zip(other.bytes())
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| line.len().min(other.len()));
    let mut start = different_at.saturating_sub(100);
    let mut end = (different_at + 100).min(line.len());
    while start != 0 && !line.is_char_boundary(start) {
        start -= 1;
    }
    while end != line.len() && !line.is_char_boundary(end) {
        end += 1;
    }
    line.get(start..end).unwrap_or(line)
}

#[test]
fn public_api_surface_matches_blessed_baseline() {
    let armed = std::env::var(ARM_ENV)
        .ok()
        .filter(|tc| !tc.trim().is_empty());
    let required = std::env::var(REQUIRE_ENV).is_ok_and(|v| !v.trim().is_empty());

    let toolchain = match armed {
        Some(tc) => tc,
        None if required => panic!(
            "[public_api] {REQUIRE_ENV} is set but {ARM_ENV} is unset/empty — the release-gate \
             surface-gate step is DISARMED. The public API would go UNCHECKED while the gate stays \
             green. Restore `{ARM_ENV}=nightly-YYYY-MM-DD` on that step."
        ),
        None => {
            eprintln!(
                "[public_api] SKIP: {ARM_ENV} unset — the cargo public-api gate runs only under \
                 release-gate.yml with the pinned nightly. Arm it with \
                 `{ARM_ENV}=nightly-YYYY-MM-DD` (add `UPDATE_EXPECT=1` to re-bless the baseline)."
            );
            return;
        }
    };

    // ARMED. Every step is fail-hard: a tooling break must turn the gate RED, never green.
    //
    // `.all_features(true)` snapshots the UNION of all cargo features, not just `default` (`mem`).
    // Today `mem = []` adds no public items, so the baseline is identical either way — but it means a
    // future non-default feature (e.g. the spec-mandated `metrics`, _spec/08 §8.1 R-OBS-1) cannot
    // smuggle a feature-gated public item past the gate: its surface lands in the diff and must be
    // re-blessed and reviewed.
    let json = rustdoc_json::Builder::default()
        .toolchain(toolchain.clone())
        .all_features(true)
        .manifest_path(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .build()
        .unwrap_or_else(|e| {
            panic!("[public_api] rustdoc JSON build failed under toolchain {toolchain}: {e}")
        });

    // Defaults INCLUDE blanket impls (Into/TryFrom/Borrow/…) and auto-trait impls (Send/Sync/Unpin/
    // UnwindSafe/…). That is deliberate, not laziness: for a safety-critical frozen surface the
    // auto-trait rows are exactly where a Send/Sync/UnwindSafe regression would surface (Engine is
    // Send + Sync for all S), and bit-exact full capture is the strongest leak detector. Do NOT add
    // `.omit_blanket_impls(true)` / `.omit_auto_trait_impls(true)` to "tidy" the baseline — that
    // would silently weaken the gate.
    let api = public_api::Builder::from_rustdoc_json(&json)
        .build()
        .unwrap_or_else(|e| {
            panic!(
                "[public_api] failed to parse rustdoc JSON — a format_version skew against the \
                 pinned COMPAT TRIPLE (see crates/oce-store/Cargo.toml [dev-dependencies])? {e}"
            )
        });

    // Render one public item per line, sorted, so the baseline is stable regardless of the tool's
    // internal ordering. The trailing `\n` makes the baseline a POSIX-conventional text file, so an
    // editor / pre-commit EOF-fixer that appends a final newline cannot turn the gate RED.
    let mut items: Vec<String> = api.items().map(|item| item.to_string()).collect();
    items.sort_unstable();
    let rendered = format!("{}\n", items.join("\n"));

    if oce_bless::enabled("UPDATE_EXPECT") {
        std::fs::write(BASELINE_PATH, &rendered).expect("write public-api baseline");
        return;
    }

    let checked_in = std::fs::read_to_string(BASELINE_PATH)
        .expect("read public-api baseline")
        .replace("\r\n", "\n");
    if checked_in != rendered {
        let expected: Vec<&str> = checked_in.lines().collect();
        let actual: Vec<&str> = rendered.lines().collect();
        if let Some(at) =
            (0..expected.len().max(actual.len())).find(|&i| expected.get(i) != actual.get(i))
        {
            let blessed = expected
                .get(at)
                .map(|line| difference_window(line, actual.get(at).copied().unwrap_or_default()));
            let generated = actual
                .get(at)
                .map(|line| difference_window(line, expected.get(at).copied().unwrap_or_default()));
            panic!(
                "crates/oce-store/tests/public-api.txt is stale (generated {} lines, blessed {} \
                 lines, first difference at line {}):\n  blessed: {blessed:?}\n  actual:  \
                 {generated:?}\nRe-bless deliberately with `OCE_PUBLIC_API_NIGHTLY=<nightly> \
                 UPDATE_EXPECT=1 cargo nextest run -p oce-store -E \
                 'test(public_api_surface_matches_blessed_baseline)' --profile public-api \
                 --locked` and review the diff.",
                actual.len(),
                expected.len(),
                at + 1,
            );
        }
        panic!(
            "crates/oce-store/tests/public-api.txt is stale due to a trailing-newline or \
             line-terminator-only difference (generated {} bytes, blessed {} bytes).\n\
             Re-bless deliberately with `OCE_PUBLIC_API_NIGHTLY=<nightly> UPDATE_EXPECT=1 cargo \
             nextest run -p oce-store -E 'test(public_api_surface_matches_blessed_baseline)' \
             --profile public-api --locked` and review the diff.",
            rendered.len(),
            checked_in.len(),
        );
    }
}
