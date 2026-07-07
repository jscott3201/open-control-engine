//! G36 Tier-A funnel-band routing: every sequence's Real outputs run through the oce-conformance L1
//! funnel band with the recorded near-ULP band (`_spec/07 §8`), sharing the sequence table with the
//! exact-oracle suite (`g36_funnel.rs`) via `g36_funnel_band/sequences.rs` so there is one source of
//! truth for every connector id. The exact oracle stays the bit-exact gate over all outputs; this
//! proves the funnel path is live on the same corpus. Boolean/Integer outputs are excluded from the
//! type-blind funnel (`_spec/07 §9.3`) by omission from the mapping — `multizone_vav_plant_requests`
//! is all-Integer and has no Real output, so it is skipped.

use oce_conformance::ValueKind;

#[path = "g36_funnel_band/policy.rs"]
mod funnel_band_policy;
#[path = "g36_funnel_band/sequences.rs"]
mod sequences;

use sequences::{PointSpec, SEQUENCES, kind_name, reference_path};

#[test]
fn funnel_band_routes_g36_sequence_real_outputs() {
    for spec in SEQUENCES {
        if !spec
            .exact_outputs
            .iter()
            .any(|point| matches!(point.kind, ValueKind::Real))
        {
            // No Real output (e.g. multizone_vav_plant_requests is all-Integer): nothing to band; the
            // exact oracle covers it. Skipping avoids a zero-comparison funnel run.
            continue;
        }
        let inputs = funnel_points(spec.inputs);
        let outputs = funnel_points(spec.exact_outputs);
        let instants: Vec<f64> = (0..=spec.t_stop)
            .map(|tick| f64::from(tick) * spec.sample_step)
            .collect();
        funnel_band_policy::route_real_outputs_through_funnel_band(
            &funnel_band_policy::FunnelRouting {
                sequence: spec.name,
                cxf: spec.cxf.as_bytes(),
                inputs: &inputs,
                outputs: &outputs,
                instants: &instants,
                sample_step: spec.sample_step,
                reference_csv: &reference_path(spec),
                kind_to_str: kind_name,
            },
        );
    }
}

fn funnel_points(specs: &[PointSpec]) -> Vec<funnel_band_policy::FunnelPoint> {
    specs
        .iter()
        .map(|point| funnel_band_policy::FunnelPoint {
            reference_name: point.reference_name,
            cdl_name: point.cdl_name,
            kind: point.kind,
        })
        .collect()
}
