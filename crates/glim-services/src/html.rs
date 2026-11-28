//! Escaping for HTML text and double-quoted attributes.
use std::fmt::{self, Display, Formatter};

pub(crate) fn escaped(text: &str) -> impl Display + '_ {
    Escaped(text)
}

struct Escaped<'a>(&'a str);

impl Display for Escaped<'_> {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        let mut next = 0;
        for (index, ch) in self.0.match_indices(['&', '<', '>', '"']) {
            out.write_str(&self.0[next..index])?;
            let entity = match ch {
                "&" => "&amp;",
                "<" => "&lt;",
                ">" => "&gt;",
                "\"" => "&quot;",
                _ => unreachable!("matched only HTML special characters"),
            };
            out.write_str(entity)?;
            next = index + ch.len();
        }
        out.write_str(&self.0[next..])
    }
}
