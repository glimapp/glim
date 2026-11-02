//! UI-independent values shared by the application and its services.

mod document;
pub mod session;
pub mod update;
mod workspace;

pub use document::{ContentSnapshot, Document, DocumentKind, ReaderMode, language_for_path};
pub use workspace::DirectoryEntry;
