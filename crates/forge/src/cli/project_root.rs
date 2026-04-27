use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Walk up from `start` until a directory containing `forge.toml` is found.
///
/// Returns the canonical project root. Errors if no `forge.toml` is found
/// before hitting the filesystem root.
pub fn find_project_root(start: &Path) -> Result<PathBuf> {
    let canonical = start
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", start.display()))?;

    let mut cursor: &Path = &canonical;
    loop {
        if cursor.join("forge.toml").is_file() {
            return Ok(cursor.to_path_buf());
        }
        match cursor.parent() {
            Some(parent) => cursor = parent,
            None => anyhow::bail!(
                "forge.toml not found in {} or any parent directory.\n\
                 Run this command from inside a Forge project, or create one with:\n  \
                 forge new my-app --template with-svelte/minimal",
                start.display()
            ),
        }
    }
}

/// Find the project root from the current working directory and `cd` to it.
///
/// Returns the resolved root so callers can log it once.
pub fn enter_project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("failed to read current directory")?;
    let root = find_project_root(&cwd)?;
    if root != cwd {
        std::env::set_current_dir(&root)
            .with_context(|| format!("failed to chdir to {}", root.display()))?;
    }
    Ok(root)
}

/// Probe for the `cargo sqlx` subcommand. The bare `cargo-sqlx` binary expects
/// the subcommand name as its first arg ("cargo-sqlx sqlx ..."), so calling
/// `cargo-sqlx --version` directly always errors. Going through cargo is the
/// canonical, version-stable invocation.
pub fn cargo_sqlx_available() -> bool {
    std::process::Command::new("cargo")
        .args(["sqlx", "--version"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// True when a Docker daemon is reachable (`docker info` succeeds).
pub async fn docker_available() -> bool {
    tokio::process::Command::new("docker")
        .args(["info"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn finds_root_from_subdirectory() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let root = tmp.path();
        fs::write(root.join("forge.toml"), "[project]\nname = \"test\"\n")
            .expect("write forge.toml");
        let nested = root.join("frontend").join("src");
        fs::create_dir_all(&nested).expect("create nested dir");

        let found = find_project_root(&nested).expect("walk up succeeds");
        assert_eq!(found, root.canonicalize().expect("canonical root"));
    }

    #[test]
    fn finds_root_when_already_at_root() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let root = tmp.path();
        fs::write(root.join("forge.toml"), "").expect("write forge.toml");

        let found = find_project_root(root).expect("returns root");
        assert_eq!(found, root.canonicalize().expect("canonical root"));
    }

    #[test]
    fn errors_when_no_forge_toml_anywhere() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let nested = tmp.path().join("a").join("b");
        fs::create_dir_all(&nested).expect("create nested dir");

        let err = find_project_root(&nested).expect_err("should error");
        let msg = format!("{}", err);
        assert!(msg.contains("forge.toml not found"), "msg was: {msg}");
    }
}
