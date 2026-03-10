mod dioxus;
mod sveltekit;

pub use dioxus::{dioxus_frontend_templates, dioxus_project_templates};
pub use sveltekit::{sveltekit_frontend_templates, sveltekit_project_templates};

#[derive(Clone, Copy)]
pub enum ScaffoldMode {
    Demo,
    Minimal,
}

pub struct TemplateFile {
    pub path: &'static str,
    pub template: &'static str,
    pub render: bool,
}

pub struct FrontendTemplates {
    pub directories: &'static [&'static str],
    pub files: &'static [TemplateFile],
}

const SHARED_FRONTEND_PLAYWRIGHT_GLOBAL_SETUP: &str =
    include_str!("../../../templates/scaffold/shared/frontend/tests/global-setup.ts.tmpl");
const SHARED_FRONTEND_PLAYWRIGHT_FIXTURES: &str =
    include_str!("../../../templates/scaffold/shared/frontend/tests/fixtures.ts.tmpl");

const SHARED_FRONTEND_FILES: &[TemplateFile] = &[
    TemplateFile {
        path: "tests/global-setup.ts",
        template: SHARED_FRONTEND_PLAYWRIGHT_GLOBAL_SETUP,
        render: false,
    },
    TemplateFile {
        path: "tests/fixtures.ts",
        template: SHARED_FRONTEND_PLAYWRIGHT_FIXTURES,
        render: false,
    },
];

pub fn shared_frontend_templates() -> &'static [TemplateFile] {
    SHARED_FRONTEND_FILES
}
