//! Every value of a JSON text with its byte span and its path, for sweeps that replace one value
//! and keep every other byte as written (GitHub #544).
//!
//! `serde_json::Value` cannot serve: without `preserve_order` it sorts an object's keys, so the
//! list of an object's values it would build is in alphabetical order, not the order the document
//! (and a struct's fields) gives them, and it respells numbers. This walker is written for the
//! well-formed fixtures it reads: it panics on anything else, and it reads a key as its raw text
//! between the quotes (no fixture key has an escape).
//!
//! A path is spelled as the wire reader spells it, independently of it: keys joined by dots,
//! list indexes in brackets counted from zero, `document` for the root.

use std::ops::Range;

/// One step from a value into a member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// An object member, by key.
    Key(String),
    /// A list item, by index.
    Index(usize),
}

/// The JSON type of a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Object,
    List,
    String,
    Number,
    Boolean,
    Null,
}

/// One value of the text.
#[derive(Clone, Debug)]
pub struct Site {
    /// The steps from the root to the value.
    pub steps: Vec<Step>,
    /// The value's bytes in the text.
    pub span: Range<usize>,
    /// Its JSON type.
    pub kind: Kind,
    /// For an object, the spans of its members' values in written order; for a list, its items'.
    pub members: Vec<Range<usize>>,
}

impl Site {
    /// The path as the wire reader spells it.
    pub fn path(&self) -> String {
        spell(&self.steps)
    }

    /// The text with this value replaced by `with`.
    pub fn replaced(&self, text: &str, with: &str) -> String {
        format!(
            "{}{with}{}",
            &text[..self.span.start],
            &text[self.span.end..]
        )
    }

    /// This object's values as a list, in written order (`[v1, v2, …]`).
    pub fn list_of_values(&self, text: &str) -> String {
        assert_eq!(self.kind, Kind::Object, "{}", self.path());
        let values: Vec<&str> = self.members.iter().map(|m| &text[m.clone()]).collect();
        format!("[{}]", values.join(", "))
    }

    /// The text with this object replaced by the list of its values.
    pub fn as_list_of_values(&self, text: &str) -> String {
        self.replaced(text, &self.list_of_values(text))
    }

    /// The 1-based line the value starts on.
    pub fn line(&self, text: &str) -> u32 {
        u32::try_from(1 + text[..self.span.start].matches('\n').count())
            .expect("a fixture's lines fit")
    }
}

/// `steps` spelled as a path.
pub fn spell(steps: &[Step]) -> String {
    if steps.is_empty() {
        return "document".to_owned();
    }
    let mut path = String::new();
    for step in steps {
        match step {
            Step::Key(key) => {
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(key);
            }
            Step::Index(index) => path.push_str(&format!("[{index}]")),
        }
    }
    path
}

/// Every value of `text`, in document order (a value before its members).
pub fn sites(text: &str) -> Vec<Site> {
    let mut walker = Walker {
        bytes: text.as_bytes(),
        at: 0,
        sites: Vec::new(),
    };
    walker.whitespace();
    walker.value(&mut Vec::new());
    walker.whitespace();
    assert_eq!(walker.at, text.len(), "one value fills the text");
    walker.sites
}

struct Walker<'a> {
    bytes: &'a [u8],
    at: usize,
    sites: Vec<Site>,
}

impl Walker<'_> {
    fn peek(&self) -> u8 {
        *self.bytes.get(self.at).expect("the text ends early")
    }

    fn whitespace(&mut self) {
        while self.at < self.bytes.len()
            && matches!(self.bytes[self.at], b' ' | b'\n' | b'\r' | b'\t')
        {
            self.at += 1;
        }
    }

    fn eat(&mut self, byte: u8) {
        assert_eq!(self.peek(), byte, "at byte {}", self.at);
        self.at += 1;
    }

    /// One value at `steps`; returns its span.
    fn value(&mut self, steps: &mut Vec<Step>) -> Range<usize> {
        let start = self.at;
        let slot = self.sites.len();
        self.sites.push(Site {
            steps: steps.clone(),
            span: start..start,
            kind: Kind::Null,
            members: Vec::new(),
        });
        let (kind, members) = match self.peek() {
            b'{' => (Kind::Object, self.object(steps)),
            b'[' => (Kind::List, self.list(steps)),
            b'"' => {
                self.string();
                (Kind::String, Vec::new())
            }
            b't' => {
                self.word("true");
                (Kind::Boolean, Vec::new())
            }
            b'f' => {
                self.word("false");
                (Kind::Boolean, Vec::new())
            }
            b'n' => {
                self.word("null");
                (Kind::Null, Vec::new())
            }
            _ => {
                while self.at < self.bytes.len()
                    && matches!(
                        self.bytes[self.at],
                        b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'
                    )
                {
                    self.at += 1;
                }
                assert!(self.at > start, "a value at byte {start}");
                (Kind::Number, Vec::new())
            }
        };
        let site = &mut self.sites[slot];
        site.span = start..self.at;
        site.kind = kind;
        site.members = members;
        start..self.at
    }

    fn word(&mut self, word: &str) {
        assert!(self.bytes[self.at..].starts_with(word.as_bytes()));
        self.at += word.len();
    }

    /// A string token; returns its raw content.
    fn string(&mut self) -> String {
        self.eat(b'"');
        let start = self.at;
        while self.peek() != b'"' {
            self.at += if self.peek() == b'\\' { 2 } else { 1 };
        }
        let raw = String::from_utf8(self.bytes[start..self.at].to_vec()).expect("UTF-8");
        self.eat(b'"');
        raw
    }

    fn object(&mut self, steps: &mut Vec<Step>) -> Vec<Range<usize>> {
        self.eat(b'{');
        let mut members = Vec::new();
        self.whitespace();
        if self.peek() == b'}' {
            self.at += 1;
            return members;
        }
        loop {
            self.whitespace();
            let key = self.string();
            self.whitespace();
            self.eat(b':');
            self.whitespace();
            steps.push(Step::Key(key));
            members.push(self.value(steps));
            steps.pop();
            self.whitespace();
            if self.peek() == b',' {
                self.at += 1;
            } else {
                self.eat(b'}');
                return members;
            }
        }
    }

    fn list(&mut self, steps: &mut Vec<Step>) -> Vec<Range<usize>> {
        self.eat(b'[');
        let mut items = Vec::new();
        self.whitespace();
        if self.peek() == b']' {
            self.at += 1;
            return items;
        }
        loop {
            self.whitespace();
            steps.push(Step::Index(items.len()));
            items.push(self.value(steps));
            steps.pop();
            self.whitespace();
            if self.peek() == b',' {
                self.at += 1;
            } else {
                self.eat(b']');
                return items;
            }
        }
    }
}
