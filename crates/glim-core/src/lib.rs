//! UI-independent values shared by the application and its services.

mod document;
pub mod formats;
pub mod graph;
pub mod outline;
pub mod resources;
pub mod session;
pub mod structured;
pub mod table;
pub mod update;
mod workspace;

pub use document::{ContentSnapshot, Document, DocumentKind, ReaderMode, language_for_path};
pub use workspace::DirectoryEntry;
