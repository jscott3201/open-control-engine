//! One output line, built byte by byte in a fixed buffer: text, decimal and hex numbers, and the
//! closing newline. It stands in for `core::fmt`, which a `no_std` image would otherwise link a few
//! kilobytes of to print the same lines.

/// The longest line, newline included; a longer one is cut off here.
pub const CAPACITY: usize = 128;

/// A line under construction.
#[derive(Clone, Debug)]
pub struct Line {
    bytes: [u8; CAPACITY],
    len: usize,
}

impl Default for Line {
    fn default() -> Self {
        Self::new()
    }
}

impl Line {
    /// An empty line.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            bytes: [0; CAPACITY],
            len: 0,
        }
    }

    fn push_byte(&mut self, byte: u8) {
        // One byte stays free for the newline `finish` appends.
        if let Some(slot) = self
            .bytes
            .get_mut(self.len)
            .filter(|_| self.len + 1 < CAPACITY)
        {
            *slot = byte;
            self.len += 1;
        }
    }

    /// Append `text`.
    pub fn push_str(&mut self, text: &str) -> &mut Self {
        for byte in text.bytes() {
            self.push_byte(byte);
        }
        self
    }

    /// Append `value` in decimal, without leading zeros.
    pub fn push_dec(&mut self, value: u64) -> &mut Self {
        let mut digits = [0_u8; 20];
        let mut start = digits.len();
        let mut rest = value;
        loop {
            start -= 1;
            digits[start] = b'0' + (rest % 10) as u8;
            rest /= 10;
            if rest == 0 {
                break;
            }
        }
        for byte in &digits[start..] {
            self.push_byte(*byte);
        }
        self
    }

    /// Append `value` as sixteen lowercase hex digits.
    pub fn push_hex(&mut self, value: u64) -> &mut Self {
        for shift in (0..16).rev() {
            let nibble = ((value >> (shift * 4)) & 0xf) as u8;
            self.push_byte(if nibble < 10 {
                b'0' + nibble
            } else {
                b'a' + nibble - 10
            });
        }
        self
    }

    /// The line with its newline appended.
    pub fn finish(&mut self) -> &[u8] {
        if let Some(slot) = self.bytes.get_mut(self.len) {
            *slot = b'\n';
            self.len += 1;
        }
        &self.bytes[..self.len]
    }
}
