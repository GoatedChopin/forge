use super::{FrontendTemplates, ScaffoldMode, TemplateFile};

const POPULATED_MAIN_RS: &str =
    include_str!("../../../templates/scaffold/demo/project/dioxus/main.rs.tmpl");
const POPULATED_BUILD_RS: &str =
    include_str!("../../../templates/scaffold/demo/project/dioxus/build.rs.tmpl");
const POPULATED_DOCKERFILE: &str =
    include_str!("../../../templates/scaffold/demo/project/dioxus/Dockerfile.tmpl");
const POPULATED_DOCKER_COMPOSE: &str =
    include_str!("../../../templates/scaffold/demo/project/dioxus/docker-compose.yml.tmpl");
const POPULATED_README: &str =
    include_str!("../../../templates/scaffold/demo/project/dioxus/README.md.tmpl");

const EMPTY_MAIN_RS: &str =
    include_str!("../../../templates/scaffold/minimal/project/dioxus/main.rs.tmpl");
const EMPTY_BUILD_RS: &str =
    include_str!("../../../templates/scaffold/minimal/project/dioxus/build.rs.tmpl");
const EMPTY_DOCKERFILE: &str =
    include_str!("../../../templates/scaffold/minimal/project/dioxus/Dockerfile.tmpl");
const EMPTY_DOCKER_COMPOSE: &str =
    include_str!("../../../templates/scaffold/minimal/project/dioxus/docker-compose.yml.tmpl");
const EMPTY_README: &str =
    include_str!("../../../templates/scaffold/minimal/project/dioxus/README.md.tmpl");

const DIOXUS_CARGO_TOML: &str =
    include_str!("../../../templates/scaffold/demo/frontend/dioxus/Cargo.toml.tmpl");
const DIOXUS_CONFIG: &str =
    include_str!("../../../templates/scaffold/demo/frontend/dioxus/Dioxus.toml.tmpl");
const DIOXUS_PACKAGE_JSON: &str =
    include_str!("../../../templates/scaffold/demo/frontend/dioxus/package.json.tmpl");
const DIOXUS_PLAYWRIGHT_CONFIG: &str =
    include_str!("../../../templates/scaffold/demo/frontend/dioxus/playwright.config.ts.tmpl");
const DIOXUS_MAIN_RS: &str =
    include_str!("../../../templates/scaffold/demo/frontend/dioxus/src/main.rs.tmpl");
const DIOXUS_HOME_SPEC: &str =
    include_str!("../../../templates/scaffold/demo/frontend/dioxus/tests/home.spec.ts.tmpl");

const EMPTY_DIOXUS_CARGO_TOML: &str =
    include_str!("../../../templates/scaffold/minimal/frontend/dioxus/Cargo.toml.tmpl");
const EMPTY_DIOXUS_CONFIG: &str =
    include_str!("../../../templates/scaffold/minimal/frontend/dioxus/Dioxus.toml.tmpl");
const EMPTY_DIOXUS_PACKAGE_JSON: &str =
    include_str!("../../../templates/scaffold/minimal/frontend/dioxus/package.json.tmpl");
const EMPTY_DIOXUS_PLAYWRIGHT_CONFIG: &str =
    include_str!("../../../templates/scaffold/minimal/frontend/dioxus/playwright.config.ts.tmpl");
const EMPTY_DIOXUS_MAIN_RS: &str =
    include_str!("../../../templates/scaffold/minimal/frontend/dioxus/src/main.rs.tmpl");
const EMPTY_DIOXUS_HOME_SPEC: &str =
    include_str!("../../../templates/scaffold/minimal/frontend/dioxus/tests/home.spec.ts.tmpl");

const PROJECT_DEMO: &[TemplateFile] = &[
    TemplateFile {
        path: "build.rs",
        template: POPULATED_BUILD_RS,
        render: false,
    },
    TemplateFile {
        path: "Dockerfile",
        template: POPULATED_DOCKERFILE,
        render: true,
    },
    TemplateFile {
        path: "docker-compose.yml",
        template: POPULATED_DOCKER_COMPOSE,
        render: true,
    },
    TemplateFile {
        path: "README.md",
        template: POPULATED_README,
        render: true,
    },
    TemplateFile {
        path: "src/main.rs",
        template: POPULATED_MAIN_RS,
        render: false,
    },
];

const PROJECT_MINIMAL: &[TemplateFile] = &[
    TemplateFile {
        path: "build.rs",
        template: EMPTY_BUILD_RS,
        render: false,
    },
    TemplateFile {
        path: "Dockerfile",
        template: EMPTY_DOCKERFILE,
        render: true,
    },
    TemplateFile {
        path: "docker-compose.yml",
        template: EMPTY_DOCKER_COMPOSE,
        render: true,
    },
    TemplateFile {
        path: "README.md",
        template: EMPTY_README,
        render: true,
    },
    TemplateFile {
        path: "src/main.rs",
        template: EMPTY_MAIN_RS,
        render: false,
    },
];

const FRONTEND_DIRS: &[&str] = &["src", "public"];

const FRONTEND_DEMO_FILES: &[TemplateFile] = &[
    TemplateFile {
        path: "Cargo.toml",
        template: DIOXUS_CARGO_TOML,
        render: true,
    },
    TemplateFile {
        path: "Dioxus.toml",
        template: DIOXUS_CONFIG,
        render: true,
    },
    TemplateFile {
        path: "package.json",
        template: DIOXUS_PACKAGE_JSON,
        render: true,
    },
    TemplateFile {
        path: "playwright.config.ts",
        template: DIOXUS_PLAYWRIGHT_CONFIG,
        render: false,
    },
    TemplateFile {
        path: "src/main.rs",
        template: DIOXUS_MAIN_RS,
        render: true,
    },
    TemplateFile {
        path: "tests/home.spec.ts",
        template: DIOXUS_HOME_SPEC,
        render: true,
    },
];

const FRONTEND_MINIMAL_FILES: &[TemplateFile] = &[
    TemplateFile {
        path: "Cargo.toml",
        template: EMPTY_DIOXUS_CARGO_TOML,
        render: true,
    },
    TemplateFile {
        path: "Dioxus.toml",
        template: EMPTY_DIOXUS_CONFIG,
        render: true,
    },
    TemplateFile {
        path: "package.json",
        template: EMPTY_DIOXUS_PACKAGE_JSON,
        render: true,
    },
    TemplateFile {
        path: "playwright.config.ts",
        template: EMPTY_DIOXUS_PLAYWRIGHT_CONFIG,
        render: false,
    },
    TemplateFile {
        path: "src/main.rs",
        template: EMPTY_DIOXUS_MAIN_RS,
        render: true,
    },
    TemplateFile {
        path: "tests/home.spec.ts",
        template: EMPTY_DIOXUS_HOME_SPEC,
        render: true,
    },
];

pub fn dioxus_project_templates(mode: ScaffoldMode) -> &'static [TemplateFile] {
    match mode {
        ScaffoldMode::Demo => PROJECT_DEMO,
        ScaffoldMode::Minimal => PROJECT_MINIMAL,
    }
}

pub fn dioxus_frontend_templates(mode: ScaffoldMode) -> FrontendTemplates {
    let files = match mode {
        ScaffoldMode::Demo => FRONTEND_DEMO_FILES,
        ScaffoldMode::Minimal => FRONTEND_MINIMAL_FILES,
    };

    FrontendTemplates {
        directories: FRONTEND_DIRS,
        files,
    }
}
