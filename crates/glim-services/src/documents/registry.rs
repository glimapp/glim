use super::Content;
use crate::{
    archive, books, drawio, executables, formats, media, opml, plist, preview, safetensors, sqlite,
    vector_formats, work::Cancellation, xmind,
};
use std::path::Path;

pub(super) struct FormatReader {
    pub id: &'static str,
    pub accepts: fn(&Path) -> bool,
    pub open: fn(&Path, &Cancellation) -> anyhow::Result<Content>,
}

// Ordered routing is part of the contract: document containers precede general
// archives, ambiguous native extensions are probed, and text/bytes come last.
pub(super) const READERS: &[FormatReader] = &[
    FormatReader {
        id: "binary-plist",
        accepts: plist::is_binary,
        open: |p, c| plist::read(p, c).map(Content::Page),
    },
    FormatReader {
        id: "opml",
        accepts: opml::supports,
        open: super::outline,
    },
    FormatReader {
        id: "drawio",
        accepts: drawio::supports,
        open: super::drawing,
    },
    FormatReader {
        id: "executable",
        accepts: executables::supports,
        open: |p, c| executables::read_preview_cancellable(p, c).map(Content::Page),
    },
    FormatReader {
        id: "book",
        accepts: books::supports,
        open: |p, c| books::read_cancellable(p, c).map(Content::Book),
    },
    FormatReader {
        id: "xmind",
        accepts: xmind::supports,
        open: super::mind_map,
    },
    FormatReader {
        id: "vector",
        accepts: vector_formats::supports,
        open: super::vector,
    },
    FormatReader {
        id: "archive",
        accepts: archive::supports,
        open: |p, c| archive::read(p, c).map(Content::Archive),
    },
    FormatReader {
        id: "container-report",
        accepts: formats::supports,
        open: |p, c| formats::read_preview_cancellable(p, c).map(Content::Page),
    },
    FormatReader {
        id: "sqlite",
        accepts: |p| sqlite::supports(p) || sqlite::has_header(p),
        open: |p, _| Ok(Content::Sqlite(p.to_path_buf())),
    },
    FormatReader {
        id: "native",
        accepts: preview::can_open,
        open: |p, _| Ok(Content::Native(p.to_path_buf(), false, false)),
    },
    FormatReader {
        id: "safetensors",
        accepts: |p| glim_core::formats::FormatSet::Safetensors.matches(p),
        open: |p, c| safetensors::read_preview_cancellable(p, c).map(Content::Page),
    },
    FormatReader {
        id: "image",
        accepts: media::supports,
        open: |p, _| media::read_image(p).map(Content::Image),
    },
];
