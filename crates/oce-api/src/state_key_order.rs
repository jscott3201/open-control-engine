//! Canonical ordering for compound keys as their revision-1 wire bytes.

use std::cmp::Ordering;

use crate::state::{BlockKey, ConnectorKey, WireDir};

impl Ord for BlockKey {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Authored(left), Self::Authored(right)) => wire_string_cmp(left, right),
            (
                Self::PassThrough {
                    input_path: left_input,
                    output_path: left_output,
                },
                Self::PassThrough {
                    input_path: right_input,
                    output_path: right_output,
                },
            ) => wire_string_cmp(left_input, right_input)
                .then_with(|| wire_string_cmp(left_output, right_output)),
            (Self::Dense(left), Self::Dense(right)) => left.to_le_bytes().cmp(&right.to_le_bytes()),
            (left, right) => block_key_tag(left).cmp(&block_key_tag(right)),
        }
    }
}

impl PartialOrd for BlockKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ConnectorKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.owner
            .cmp(&other.owner)
            .then_with(|| wire_dir_tag(self.direction).cmp(&wire_dir_tag(other.direction)))
            .then_with(|| {
                self.port_index
                    .to_le_bytes()
                    .cmp(&other.port_index.to_le_bytes())
            })
    }
}

impl PartialOrd for ConnectorKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn block_key_tag(key: &BlockKey) -> u8 {
    match key {
        BlockKey::Authored(_) => 0,
        BlockKey::PassThrough { .. } => 1,
        BlockKey::Dense(_) => 2,
    }
}

fn wire_dir_tag(direction: WireDir) -> u8 {
    match direction {
        WireDir::In => 0,
        WireDir::Out => 1,
    }
}

fn wire_string_cmp(left: &str, right: &str) -> Ordering {
    let left_len = u32::try_from(left.len()).unwrap_or(u32::MAX).to_le_bytes();
    let right_len = u32::try_from(right.len()).unwrap_or(u32::MAX).to_le_bytes();
    left_len
        .cmp(&right_len)
        .then_with(|| left.as_bytes().cmp(right.as_bytes()))
}
