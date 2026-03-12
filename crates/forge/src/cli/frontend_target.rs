use anyhow::Result;
use std::fmt;
use std::path::Path;
use std::process::Command;
use std::str::FromStr;

use super::frontend_codegen::{
    BindingGeneratorFn, BindingGeneratorInput, generate_dioxus_bindings, generate_svelte_bindings,
};

type DetectFn = fn(&Path) -> bool;
type PostGenerateFn = fn(&Path) -> Result<()>;
type ExtraFormatFn = fn(&Path) -> Result<bool>;

pub struct FrontendTargetSpec {
    pub id: &'static str,
    pub display_name: &'static str,
    pub default_output_dir: &'static str,
    detect: DetectFn,
    post_generate: PostGenerateFn,
    extra_format: ExtraFormatFn,
    generate_bindings: BindingGeneratorFn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontendTarget {
    SvelteKit,
    Dioxus,
}

const SUPPORTED_FRONTENDS: [FrontendTarget; 2] =
    [FrontendTarget::Dioxus, FrontendTarget::SvelteKit];

const SVELTEKIT_SPEC: FrontendTargetSpec = FrontendTargetSpec {
    id: "sveltekit",
    display_name: "SvelteKit",
    default_output_dir: "frontend/src/lib/forge",
    detect: detect_sveltekit,
    post_generate: post_generate_sveltekit,
    extra_format: no_extra_format,
    generate_bindings: generate_svelte_bindings,
};

const DIOXUS_SPEC: FrontendTargetSpec = FrontendTargetSpec {
    id: "dioxus",
    display_name: "Dioxus",
    default_output_dir: "frontend/src/forge",
    detect: detect_dioxus,
    post_generate: no_post_generate,
    extra_format: format_dioxus_frontend,
    generate_bindings: generate_dioxus_bindings,
};

impl FrontendTarget {
    pub fn detect(frontend_dir: &Path) -> Option<Self> {
        SUPPORTED_FRONTENDS
            .into_iter()
            .find(|target| (target.spec().detect)(frontend_dir))
    }

    pub fn spec(self) -> &'static FrontendTargetSpec {
        match self {
            Self::SvelteKit => &SVELTEKIT_SPEC,
            Self::Dioxus => &DIOXUS_SPEC,
        }
    }

    pub fn default_output_dir(self) -> &'static str {
        self.spec().default_output_dir
    }

    pub fn display_name(self) -> &'static str {
        self.spec().display_name
    }

    pub fn post_generate(self, frontend_dir: &Path) -> Result<()> {
        (self.spec().post_generate)(frontend_dir)
    }

    pub fn extra_format(self, project_dir: &Path) -> Result<bool> {
        (self.spec().extra_format)(project_dir)
    }

    pub fn generate_bindings(self, input: &BindingGeneratorInput<'_>) -> Result<()> {
        (self.spec().generate_bindings)(input)
    }
}

impl fmt::Display for FrontendTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.spec().id)
    }
}

impl FromStr for FrontendTarget {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "svelte" | "sveltekit" => Ok(Self::SvelteKit),
            "dioxus" => Ok(Self::Dioxus),
            other => Err(format!(
                "unsupported frontend target '{other}' (expected 'sveltekit' or 'dioxus')"
            )),
        }
    }
}

fn detect_sveltekit(frontend_dir: &Path) -> bool {
    frontend_dir.join("svelte.config.js").exists()
        || frontend_dir.join("package.json").exists() && frontend_dir.join("src/routes").exists()
}

fn detect_dioxus(frontend_dir: &Path) -> bool {
    frontend_dir.join("Dioxus.toml").exists()
        || frontend_dir.join("dioxus.toml").exists()
        || frontend_dir.join("Cargo.toml").exists()
}

fn post_generate_sveltekit(frontend_dir: &Path) -> Result<()> {
    let _ = Command::new("bunx")
        .args(["svelte-kit", "sync"])
        .current_dir(frontend_dir)
        .output();
    Ok(())
}

fn no_post_generate(_frontend_dir: &Path) -> Result<()> {
    Ok(())
}

fn no_extra_format(_project_dir: &Path) -> Result<bool> {
    Ok(false)
}

fn format_dioxus_frontend(project_dir: &Path) -> Result<bool> {
    let output = Command::new("cargo")
        .args(["fmt", "--manifest-path", "frontend/Cargo.toml"])
        .current_dir(project_dir)
        .output()?;

    Ok(output.status.success())
}
