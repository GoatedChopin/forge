use console::{StyledObject, style};
use std::fmt::Display;

pub fn ok() -> StyledObject<&'static str> {
    style("[OK]").green().bold()
}

pub fn warn() -> StyledObject<&'static str> {
    style("[!]").yellow().bold()
}

pub fn error() -> StyledObject<&'static str> {
    style("[X]").red().bold()
}

pub fn info() -> StyledObject<&'static str> {
    style("[i]").blue().bold()
}

pub fn step() -> StyledObject<&'static str> {
    style("[>]").cyan().bold()
}

pub fn stop() -> StyledObject<&'static str> {
    style("[.]").cyan().bold()
}

pub fn tool() -> StyledObject<&'static str> {
    style("[*]").cyan().bold()
}

pub fn bullet() -> StyledObject<&'static str> {
    style("-").dim()
}

pub fn section(title: &str) {
    println!();
    println!("{} {}", style("==").cyan().bold(), style(title).bold());
}

pub fn kv(key: &str, value: impl Display) {
    println!("    {:<11} {}", format!("{key}:"), value);
}

pub fn command(cmd: &str, desc: &str) {
    println!("  {} {} {}", bullet(), style(cmd).cyan(), style(desc).dim());
}
