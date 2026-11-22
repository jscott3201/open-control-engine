use super::*;

#[test]
fn a_message_is_cut_at_a_character_boundary_within_the_limit() {
    assert_eq!(cut("short", MAX_ENGINE_MESSAGE_BYTES), "short");
    let exact = "a".repeat(MAX_ENGINE_MESSAGE_BYTES);
    assert_eq!(cut(&exact, MAX_ENGINE_MESSAGE_BYTES), exact);
    let over = "a".repeat(MAX_ENGINE_MESSAGE_BYTES + 1);
    assert_eq!(
        cut(&over, MAX_ENGINE_MESSAGE_BYTES).len(),
        MAX_ENGINE_MESSAGE_BYTES
    );
    // A two-byte character straddling the limit is dropped whole.
    let straddling = format!("{}é", "a".repeat(MAX_ENGINE_MESSAGE_BYTES - 1));
    let shown = cut(&straddling, MAX_ENGINE_MESSAGE_BYTES);
    assert_eq!(shown.len(), MAX_ENGINE_MESSAGE_BYTES - 1);
    assert!(straddling.starts_with(&shown));
    let wide = "€".repeat(200);
    assert!(cut(&wide, MAX_ENGINE_MESSAGE_BYTES).len() <= MAX_ENGINE_MESSAGE_BYTES);
    assert_eq!(cut(&wide, MAX_ENGINE_MESSAGE_BYTES).len() % 3, 0);
}

#[test]
fn a_stretch_runs_from_its_refused_frame_to_before_the_next_executed_one() {
    let closed = Stretch {
        from: 10.0,
        until: Some(20.0),
    };
    assert!(!closed.holds(9.999));
    assert!(closed.holds(10.0));
    assert!(closed.holds(19.999));
    assert!(!closed.holds(20.0));
    let open = Stretch {
        from: 10.0,
        until: None,
    };
    assert!(open.holds(10.0));
    assert!(open.holds(f64::MAX));
    assert!(!open.holds(9.0));
}

/// Stretches opened by every odd second's frame and closed by the next even one: the shape of
/// alternating refused frames.
fn alternating(count: u32) -> Vec<Stretch> {
    (0..count)
        .map(|k| Stretch {
            from: f64::from(2 * k + 1),
            until: Some(f64::from(2 * k + 2)),
        })
        .collect()
}

#[test]
fn the_uncovered_instants_of_alternating_refused_frames_take_linear_work() {
    let stretches = alternating(50_000);
    let instants: Vec<f64> = (0..200_000).map(|k| f64::from(k) * 0.5).collect();
    let (flags, steps) = uncovered_at(&stretches, &instants);
    // At 2k + 1 and 2k + 1.5 inside a stretch; at 2k and 2k + 0.5 outside.
    for (k, flag) in flags.iter().enumerate().take(12) {
        assert_eq!(*flag, k % 4 == 2 || k % 4 == 3, "instant {k}");
    }
    assert_eq!(flags.iter().filter(|f| **f).count(), 100_000);
    // One step per instant and one per stretch passed: never instants times stretches.
    assert!(
        steps <= (instants.len() + stretches.len()) as u64,
        "{steps}"
    );
}

#[test]
fn a_stretch_to_stop_holds_every_later_instant_and_an_empty_list_none() {
    let stretches = [
        Stretch {
            from: 1.0,
            until: Some(2.0),
        },
        Stretch {
            from: 5.0,
            until: None,
        },
    ];
    let (flags, _) = uncovered_at(&stretches, &[0.0, 1.0, 2.0, 4.9, 5.0, 99.0]);
    assert_eq!(flags, [false, true, false, false, true, true]);
    let (flags, steps) = uncovered_at(&[], &[0.0, 1.0]);
    assert_eq!((flags, steps), (vec![false, false], 2));
}
