//! Each lookup answers what a lookup by text answers, reading the text once per allocation, at
//! one, two and four times as many inputs holding it.

use std::collections::HashMap;
use std::sync::Arc;

use super::{PositionsByView, distinct_texts};

#[test]
fn a_boundarys_position_is_looked_up_once_however_many_inputs_hold_it() {
    let shared: Arc<str> = Arc::from("urn:x#u");
    let separate: Arc<str> = Arc::from("urn:x#u");
    let other: Arc<str> = Arc::from("urn:x#v");
    let by_text = HashMap::from([("urn:x#u", 7), ("urn:x#v", 3)]);
    for inputs in [64, 128, 256] {
        let mut positions = PositionsByView::new(&by_text);
        for _ in 0..inputs {
            assert_eq!(positions.get(&shared), Some(7));
            assert_eq!(positions.get(&other), Some(3));
        }
        // A separate allocation of the same text, and a view of the start of the shared one.
        assert_eq!(positions.get(&separate), Some(7));
        assert_eq!(positions.get(&shared[..5]), None);
        assert_eq!(positions.text_lookups, 4, "{inputs} inputs");
    }
}

#[test]
fn distinct_texts_hash_each_allocation_once_and_keep_every_text() {
    let shared: Arc<str> = Arc::from("urn:x#u");
    let separate: Arc<str> = Arc::from("urn:x#u");
    let other: Arc<str> = Arc::from("urn:x#v");
    for inputs in [64, 128, 256] {
        let iris = (0..inputs)
            .flat_map(|_| [&*shared, &*other])
            .chain([&*separate, &shared[..5]]);
        let (texts, hashed) = distinct_texts(iris);
        let mut texts: Vec<&str> = texts.into_iter().collect();
        texts.sort_unstable();
        assert_eq!(texts, ["urn:x", "urn:x#u", "urn:x#v"], "{inputs} inputs");
        assert_eq!(hashed, 4, "{inputs} inputs");
    }
}
