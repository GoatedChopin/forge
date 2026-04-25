use anyhow::Result;
use clap::{Parser, ValueEnum};
use clap_complete::generate;
use std::io;

/// Print shell-init snippets for Forge: `eval "$(forge env)"` in your rc file.
///
/// Outputs the env vars Forge dev needs (`SQLX_OFFLINE=true`, optional
/// `FORGE_ROOT`) and a completion source line for the detected shell.
#[derive(Parser)]
pub struct EnvCommand {
    /// Shell to emit init for. Defaults to auto-detect from `$SHELL`.
    #[arg(long)]
    pub shell: Option<Shell>,

    /// Skip emitting completion bindings.
    #[arg(long)]
    pub no_completions: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
}

impl Shell {
    fn detect() -> Option<Self> {
        let s = std::env::var("SHELL").ok()?;
        if s.ends_with("/bash") {
            Some(Shell::Bash)
        } else if s.ends_with("/zsh") {
            Some(Shell::Zsh)
        } else if s.ends_with("/fish") {
            Some(Shell::Fish)
        } else {
            None
        }
    }

    fn clap_shell(self) -> clap_complete::Shell {
        match self {
            Shell::Bash => clap_complete::Shell::Bash,
            Shell::Zsh => clap_complete::Shell::Zsh,
            Shell::Fish => clap_complete::Shell::Fish,
        }
    }
}

impl EnvCommand {
    pub async fn execute(self) -> Result<()> {
        let project_root = super::project_root::find_project_root(&std::env::current_dir()?).ok();

        let detected = self.shell.or_else(Shell::detect);

        match detected {
            Some(Shell::Fish) => emit_fish(&project_root),
            Some(Shell::Bash) | Some(Shell::Zsh) | None => emit_posix(&project_root),
        }

        if !self.no_completions {
            match detected {
                Some(shell) => {
                    let mut cmd = <super::Cli as clap::CommandFactory>::command();
                    let stdout = io::stdout();
                    let mut handle = stdout.lock();
                    generate(shell.clap_shell(), &mut cmd, "forge", &mut handle);
                }
                None => {
                    eprintln!(
                        "# forge env: unrecognised shell ($SHELL={:?}); skipping completions.",
                        std::env::var("SHELL").unwrap_or_default()
                    );
                    eprintln!("# Pass --shell bash|zsh|fish to override.");
                }
            }
        }

        Ok(())
    }
}

fn emit_posix(root: &Option<std::path::PathBuf>) {
    println!("export SQLX_OFFLINE=true");
    if let Some(r) = root {
        println!(
            "export FORGE_ROOT={}",
            shell_quote_posix(&r.display().to_string())
        );
    }
}

fn emit_fish(root: &Option<std::path::PathBuf>) {
    println!("set -x SQLX_OFFLINE true");
    if let Some(r) = root {
        println!(
            "set -x FORGE_ROOT {}",
            shell_quote_posix(&r.display().to_string())
        );
    }
}

fn shell_quote_posix(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.' | ':' | '=' | '+'))
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_simple() {
        assert_eq!(shell_quote_posix("/usr/local/bin"), "/usr/local/bin");
        assert_eq!(shell_quote_posix("a b"), "'a b'");
        assert_eq!(shell_quote_posix("it's"), "'it'\\''s'");
    }
}
