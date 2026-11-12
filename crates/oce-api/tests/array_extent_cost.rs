//! The cost of the load gate's vector rules (GitHub #626), in allocations, never in elapsed
//! time. Each rule reads a block's elements in one pass over its parameters and looks each judged
//! index up in an ordered map, building no string per index up to the length; the rules it
//! replaced formatted the name of every index from 1 to the length and scanned the parameters
//! for it, so a 2^20 width cost a million allocations before any element was judged.
//!
//! Measured through `class_ports`, which runs exactly the load gate's parameter rules for one
//! block and, on a refusal, returns before it names any port, so the count is the rules'. Each
//! table here is refused by the old rules too (a wrong-kind element inside the length), which is
//! what makes the old counts in the brackets comparable.

use allocation_counter::{AllocationInfo, measure};
use oce_api::{ClassPortsError, Value, class_ports};

/// `oce_blocks::MAX_RESOLVED_PORT_WIDTH`, the widest length the load gate accepts.
const WIDTH: i64 = 1 << 20;

fn refused(class: &str, params: &[(&str, Value)]) -> AllocationInfo {
    let mut result = None;
    let info = measure(|| result = Some(class_ports(class, params)));
    assert!(
        matches!(result, Some(Err(ClassPortsError::InvalidParameters { .. }))),
        "{class}: {result:?}"
    );
    info
}

#[test]
fn judging_a_vector_at_the_widest_length_allocates_nothing_per_index() {
    // A wrong-kind element inside the length [measured: 17 and 19 allocations; the per-index
    // rules made 1,097,169 and 2,097,170], then an element one past it [16 and 18].
    let past_gain = format!("k_{}", WIDTH + 1);
    let past_selector = format!("extract_{}", WIDTH + 1);
    let cases: [(&str, Vec<(&str, Value)>); 4] = [
        (
            "CDL.Reals.MultiSum",
            vec![
                ("nin", Value::Integer(WIDTH)),
                ("k_1", Value::Boolean(true)),
            ],
        ),
        (
            "CDL.Routing.RealExtractSignal",
            vec![
                ("nin", Value::Integer(WIDTH)),
                ("nout", Value::Integer(WIDTH)),
                ("extract_1", Value::Real(1.5)),
            ],
        ),
        (
            "CDL.Reals.MultiSum",
            vec![
                ("nin", Value::Integer(WIDTH)),
                (past_gain.as_str(), Value::Real(1.0)),
            ],
        ),
        (
            "CDL.Routing.RealExtractSignal",
            vec![
                ("nin", Value::Integer(WIDTH)),
                ("nout", Value::Integer(WIDTH)),
                (past_selector.as_str(), Value::Integer(1)),
            ],
        ),
    ];
    for (class, params) in &cases {
        let info = refused(class, params);
        assert!(info.count_total <= 64, "{class}: {info:?}");
    }
}

#[test]
fn refusing_many_elements_past_the_length_costs_linearly_in_the_elements() {
    // `n` gains past `nin = 2`, each refused once: at most six allocations and 400 bytes of
    // peak per element [measured: 20,502 and 327,706 allocations, 1,393,232 and 22,282,832
    // bytes; 5.0 and 340 per element at both sizes].
    for n in [4_096_u64, 65_536] {
        let names: Vec<String> = (3..n + 3).map(|i| format!("k_{i}")).collect();
        let mut params = vec![("nin", Value::Integer(2))];
        params.extend(names.iter().map(|name| (name.as_str(), Value::Real(1.0))));
        let info = refused("CDL.Reals.MultiSum", &params);
        assert!(info.count_total <= 6 * n + 64, "{n}: {info:?}");
        assert!(info.bytes_max <= 400 * n, "{n}: {info:?}");
    }
}
