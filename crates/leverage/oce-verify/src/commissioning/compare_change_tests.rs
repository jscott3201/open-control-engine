use super::*;

#[test]
fn the_ends_of_the_stretches_are_each_from_and_each_closing_until_in_order() {
    let stretches = [
        Stretch {
            from: 5.0,
            until: Some(7.0),
        },
        Stretch {
            from: 9.0,
            until: None,
        },
    ];
    assert_eq!(stretch_ends(&stretches), [5.0, 7.0, 9.0]);
}
