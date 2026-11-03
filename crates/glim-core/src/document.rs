use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentKind {
    Markdown,
    Text,
}

impl DocumentKind {
    pub fn from_path(path: &Path) -> Self {
        if crate::formats::FormatSet::Markdown.matches(path) {
            Self::Markdown
        } else {
            Self::Text
        }
    }
}

/// A successfully decoded, read-only document. No GPUI types cross this boundary.
#[derive(Debug)]
pub struct Document {
    pub path: PathBuf,
    pub kind: DocumentKind,
    pub text: String,
}

/// Tree-sitter registry names; unsupported extensions stay readable as plain text.
pub fn language_for_path(path: &Path) -> &'static str {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let lower = name.to_ascii_lowercase();
    if lower == "dockerfile"
        || lower == "containerfile"
        || lower.starts_with("dockerfile.")
        || lower.starts_with("containerfile.")
        || lower.ends_with(".dockerfile")
    {
        return "dockerfile";
    }
    if matches!(
        name,
        ".npmrc"
            | ".yarnrc"
            | ".editorconfig"
            | ".gitconfig"
            | ".gitmodules"
            | ".pypirc"
            | "pip.conf"
            | "setup.cfg"
            | "tox.ini"
    ) {
        return "ini";
    }
    if name == ".env" || name.starts_with(".env.") {
        return "dotenv";
    }
    if matches!(
        name,
        ".gitignore" | ".dockerignore" | ".npmignore" | ".ignore" | ".gitattributes"
    ) {
        return "ignore";
    }
    if matches!(name, ".prettierrc" | ".stylelintrc" | ".swcrc") {
        return "json";
    }
    if name == "Cargo.lock" || name == "uv.lock" || name == "poetry.lock" || name == "Pipfile" {
        return "toml";
    }
    if matches!(
        name,
        "Gemfile"
            | "Rakefile"
            | "Guardfile"
            | "Podfile"
            | "Vagrantfile"
            | "Brewfile"
            | "Fastfile"
            | "Appfile"
            | ".irbrc"
            | ".pryrc"
    ) {
        return "ruby";
    }
    if name == "CMakeLists.txt" {
        return "cmake";
    }
    if matches!(name, "Makefile" | "GNUmakefile" | "makefile") {
        return "make";
    }
    if matches!(name, "BUILD" | "WORKSPACE") {
        return "python";
    }
    if matches!(name, ".bashrc" | ".bash_profile" | ".zshrc" | ".zprofile") {
        return "bash";
    }
    if matches!(name, ".eslintrc" | ".babelrc" | ".czrc") {
        return "json";
    }
    crate::formats::language_for_extension(
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default(),
    )
}

/// Saved contents and derived fingerprints occupy separate domains.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContentSnapshot {
    Text(std::sync::Arc<str>),
    Fingerprint(String),
    Untracked,
}
impl ContentSnapshot {
    pub fn saved_text(&self) -> Option<std::sync::Arc<str>> {
        match self {
            Self::Text(text) => Some(text.clone()),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReaderMode {
    Source,
    Preview,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognizes_source_aliases_without_misclassifying_unknown_files() {
        for (path, language) in [
            ("Main.JAVA", "java"),
            ("app.rb", "ruby"),
            ("Gemfile", "ruby"),
            ("Podfile", "ruby"),
            ("package.gemspec", "ruby"),
            ("index.php", "php"),
            ("Main.cs", "csharp"),
            ("build.gradle.kts", "kotlin"),
            ("init.lua", "lua"),
            ("build.sbt", "scala"),
            ("mix.exs", "elixir"),
            ("build.zig", "zig"),
            ("query.gql", "graphql"),
            ("api.proto", "proto"),
            ("CMakeLists.txt", "cmake"),
            ("Page.astro", "astro"),
            ("App.svelte", "svelte"),
            ("index.html.erb", "erb"),
            ("index.ejs", "ejs"),
            ("schema.sql", "sql"),
            ("Component.vue", "html"),
            ("material.mtlx", "html"),
            ("shader.glsl", "c"),
            ("kernel.cu", "cpp"),
            ("types.pxd", "python"),
            ("events.jsonl", "json"),
            ("Makefile", "make"),
            (".zshrc", "bash"),
            ("table.csv", "plain"),
            ("unknown.blend", "plain"),
        ] {
            assert_eq!(language_for_path(Path::new(path)), language, "{path}");
        }
    }
}
