//! The `core::fmt`-free line writer: decimal and hex at their extremes, the newline, and the cut
//! at capacity (issue #402).

use oce_firmware_link::line::{CAPACITY, Line};

fn text(line: &mut Line) -> String {
    String::from_utf8(line.finish().to_vec()).unwrap()
}

#[test]
fn decimal_covers_zero_and_the_extremes() {
    for value in [
        0,
        1,
        9,
        10,
        99,
        100,
        160,
        u64::from(u32::MAX),
        u64::MAX - 1,
        u64::MAX,
    ] {
        assert_eq!(text(Line::new().push_dec(value)), format!("{value}\n"));
    }
}

#[test]
fn hex_is_always_sixteen_lowercase_digits() {
    for value in [0, 1, 0xf, 0xdace_fc48_58f1_60f3, u64::MAX, 1 << 63] {
        assert_eq!(text(Line::new().push_hex(value)), format!("{value:016x}\n"));
    }
}

#[test]
fn pieces_concatenate_in_order() {
    let mut line = Line::new();
    line.push_str("frame ")
        .push_dec(7)
        .push_str(" digest ")
        .push_hex(0xab);
    assert_eq!(text(&mut line), "frame 7 digest 00000000000000ab\n");
    assert_eq!(text(&mut Line::default()), "\n");
}

#[test]
fn an_overlong_line_is_cut_and_still_ends_in_a_newline() {
    let long = "x".repeat(CAPACITY * 2);
    let mut line = Line::new();
    line.push_str(&long).push_dec(12345).push_hex(1);
    let out = text(&mut line);
    assert_eq!(out.len(), CAPACITY);
    assert_eq!(out, format!("{}\n", "x".repeat(CAPACITY - 1)));
}
