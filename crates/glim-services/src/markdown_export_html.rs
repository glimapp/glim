//! Preserve reading markup, without copying active elements or attributes.
use super::{Assets, escaped, link_target};
use html5ever::tokenizer::{
    BufferQueue, Tag, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, states::RawKind,
};
use std::fmt::Write;
use url::Url;

pub(super) fn render(fragment: &str, assets: &mut Assets<'_>, base: &Url) -> String {
    let mut input = BufferQueue::default();
    input.push_back(fragment.into());
    let mut tokenizer = Tokenizer::new(
        Html {
            assets,
            base,
            output: String::new(),
            skipping_raw_text: false,
        },
        Default::default(),
    );
    let _ = tokenizer.feed(&mut input);
    tokenizer.end();
    tokenizer.sink.output
}

struct Html<'a, 'b> {
    assets: &'a mut Assets<'b>,
    base: &'a Url,
    output: String,
    skipping_raw_text: bool,
}

impl TokenSink for Html<'_, '_> {
    type Handle = ();

    fn process_token(&mut self, token: Token, _: u64) -> TokenSinkResult<()> {
        match token {
            Token::TagToken(tag) if self.skipping_raw_text => {
                if tag.kind == TagKind::EndTag {
                    self.skipping_raw_text = false;
                }
            }
            Token::TagToken(tag) => {
                if tag.kind == TagKind::StartTag
                    && matches!(
                        tag.name.as_ref(),
                        "script" | "style" | "iframe" | "xmp" | "textarea" | "title"
                    )
                {
                    self.skipping_raw_text = true;
                    return TokenSinkResult::RawData(RawKind::Rawtext);
                }
                self.tag(tag);
            }
            Token::CharacterTokens(text) if !self.skipping_raw_text => {
                self.output.push_str(&escaped(&text));
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}

impl Html<'_, '_> {
    fn tag(&mut self, tag: Tag) {
        let name = tag.name.as_ref();
        let void = matches!(name, "br" | "hr" | "img" | "col" | "wbr");
        if !void
            && !matches!(
                name,
                "a" | "abbr"
                    | "b"
                    | "blockquote"
                    | "caption"
                    | "center"
                    | "cite"
                    | "code"
                    | "colgroup"
                    | "dd"
                    | "del"
                    | "details"
                    | "div"
                    | "dl"
                    | "dt"
                    | "em"
                    | "figcaption"
                    | "figure"
                    | "h1"
                    | "h2"
                    | "h3"
                    | "h4"
                    | "h5"
                    | "h6"
                    | "i"
                    | "kbd"
                    | "li"
                    | "mark"
                    | "ol"
                    | "p"
                    | "pre"
                    | "q"
                    | "s"
                    | "samp"
                    | "small"
                    | "span"
                    | "strong"
                    | "sub"
                    | "summary"
                    | "sup"
                    | "table"
                    | "tbody"
                    | "td"
                    | "tfoot"
                    | "th"
                    | "thead"
                    | "tr"
                    | "u"
                    | "ul"
                    | "var"
            )
        {
            return;
        }
        if tag.kind == TagKind::EndTag {
            if !void {
                let _ = write!(self.output, "</{name}>");
            }
            return;
        }
        let _ = write!(self.output, "<{name}");
        for attr in &tag.attrs {
            let key = attr.name.local.as_ref();
            let value = attr.value.as_ref();
            match (name, key) {
                ("a", "href") => {
                    if let Some(target) = link_target(self.base, value) {
                        self.attribute(key, &target);
                    }
                }
                ("img", "src") if !value.trim().is_empty() => {
                    let staged = self.assets.image(self.base, value);
                    self.attribute(key, &staged);
                }
                ("details", "open") => {}
                (_, "title" | "id" | "lang") | ("img", "alt") | ("a", "name") => {
                    self.attribute(key, value)
                }
                (_, "align") if matches!(value, "left" | "center" | "right" | "justify") => {
                    self.attribute(key, value);
                }
                (_, "dir") if matches!(value, "ltr" | "rtl" | "auto") => self.attribute(key, value),
                ("img" | "table" | "col" | "td" | "th", "width" | "height")
                    if value
                        .strip_suffix('%')
                        .unwrap_or(value)
                        .parse::<u32>()
                        .is_ok() =>
                {
                    self.attribute(key, value);
                }
                ("td" | "th", "rowspan" | "colspan") if value.parse::<u32>().is_ok() => {
                    self.attribute(key, value);
                }
                ("ol", "start") | ("li", "value") if value.parse::<i32>().is_ok() => {
                    self.attribute(key, value);
                }
                _ => {}
            }
        }
        // Printed material cannot expand disclosures after export.
        if name == "details"
            && (!self.assets.embedded
                || tag
                    .attrs
                    .iter()
                    .any(|attr| attr.name.local.as_ref() == "open"))
        {
            self.output.push_str(" open");
        }
        self.output.push('>');
    }

    fn attribute(&mut self, key: &str, value: &str) {
        let _ = write!(self.output, " {key}=\"{}\"", escaped(value));
    }
}
