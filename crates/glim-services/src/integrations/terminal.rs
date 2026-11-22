//! User-scoped CLI and agent skill installation from the signed bundle.
use super::{Kind, Target, bundle, home};
use anyhow::{Context, Result, bail, ensure};
use std::{
    collections::BTreeSet,
    env, fs,
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, symlink},
    path::{Path, PathBuf},
};

fn link(source: &Path, destination: &Path) -> Result<()> {
    ensure!(source.is_file(), "Missing {}", source.display());
    fs::create_dir_all(destination.parent().context("Missing parent")?)?;
    if fs::read_link(destination).ok().as_deref() == Some(source) {
        return Ok(());
    }
    symlink(source, destination).with_context(|| destination.display().to_string())
}

fn shell_path(home: &Path) -> Result<()> {
    let shell = env::var_os("SHELL").map(PathBuf::from);
    let (profile, line) = match shell
        .as_deref()
        .and_then(Path::file_name)
        .and_then(|s| s.to_str())
        .unwrap_or("zsh")
    {
        "fish" => (
            env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"))
                .join("fish/conf.d/glim.fish"),
            "fish_add_path --path \"$HOME/.local/bin\"",
        ),
        "zsh" => (
            env::var_os("ZDOTDIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.to_owned())
                .join(".zshrc"),
            POSIX_PATH,
        ),
        "bash" => (
            [".bash_profile", ".bash_login", ".profile"]
                .into_iter()
                .map(|name| home.join(name))
                .find(|path| path.is_file())
                .unwrap_or_else(|| home.join(".bash_profile")),
            POSIX_PATH,
        ),
        "sh" => (home.join(".profile"), POSIX_PATH),
        other => bail!("Unsupported shell: {other}"),
    };
    fs::create_dir_all(profile.parent().context("Missing profile directory")?)?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .mode(0o600)
        .open(&profile)
        .with_context(|| profile.display().to_string())?;
    file.lock()?;
    ensure!(
        file.metadata()?.is_file(),
        "Invalid profile: {}",
        profile.display()
    );
    let mut original = String::new();
    file.read_to_string(&mut original)?;
    if !original.lines().any(|value| value == line) {
        file.write_all(format!("\n# Glim CLI\n{line}\n").as_bytes())?;
    }
    Ok(())
}

const POSIX_PATH: &str = r#"case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) export PATH="$HOME/.local/bin:$PATH" ;; esac"#;

pub fn install(targets: &[Target]) -> Result<()> {
    let home = home()?;
    let resources = bundle()?.join("Contents/Resources");
    link(&resources.join("glim"), &home.join(".local/bin/glim"))?;
    let mut errors = Vec::new();
    if let Err(error) = shell_path(&home) {
        errors.push(format!("{error:#}"));
    }
    let mut roots = BTreeSet::new();
    for target in targets {
        let Some(parent) = target.config.parent() else {
            continue;
        };
        let root = match target.kind {
            Kind::ClaudeDesktop => continue,
            Kind::ClaudeCode if parent == home => home.join(".claude"),
            Kind::VsCode => home.join(".copilot"),
            Kind::Codex => {
                roots.insert(home.join(".agents"));
                parent.to_owned()
            }
            _ => parent.to_owned(),
        };
        roots.insert(root);
    }
    for root in roots {
        let dest = root.join("skills/glim/SKILL.md");
        if let Err(error) = link(&resources.join("skills/glim/SKILL.md"), &dest) {
            errors.push(format!("{error:#}"));
        }
    }
    ensure!(errors.is_empty(), "{}", errors.join("\n"));
    Ok(())
}
