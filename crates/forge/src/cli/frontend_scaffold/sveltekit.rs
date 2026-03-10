use super::{FrontendTemplates, ScaffoldMode, TemplateFile};

const POPULATED_MAIN_RS: &str =
    include_str!("../../../templates/scaffold/demo/project/sveltekit/main.rs.tmpl");
const POPULATED_BUILD_RS: &str =
    include_str!("../../../templates/scaffold/demo/project/sveltekit/build.rs.tmpl");
const POPULATED_DOCKERFILE: &str =
    include_str!("../../../templates/scaffold/demo/project/sveltekit/Dockerfile.tmpl");
const POPULATED_DOCKER_COMPOSE: &str =
    include_str!("../../../templates/scaffold/demo/project/sveltekit/docker-compose.yml.tmpl");
const POPULATED_README: &str =
    include_str!("../../../templates/scaffold/demo/project/sveltekit/README.md.tmpl");

const EMPTY_MAIN_RS: &str =
    include_str!("../../../templates/scaffold/minimal/project/sveltekit/main.rs.tmpl");
const EMPTY_BUILD_RS: &str =
    include_str!("../../../templates/scaffold/minimal/project/sveltekit/build.rs.tmpl");
const EMPTY_DOCKERFILE: &str =
    include_str!("../../../templates/scaffold/minimal/project/sveltekit/Dockerfile.tmpl");
const EMPTY_DOCKER_COMPOSE: &str =
    include_str!("../../../templates/scaffold/minimal/project/sveltekit/docker-compose.yml.tmpl");
const EMPTY_README: &str =
    include_str!("../../../templates/scaffold/minimal/project/sveltekit/README.md.tmpl");

const SVELTE_PACKAGE_JSON: &str =
    include_str!("../../../templates/scaffold/demo/frontend/sveltekit/package.json.tmpl");
const SVELTE_CONFIG: &str =
    include_str!("../../../templates/scaffold/demo/frontend/sveltekit/svelte.config.js.tmpl");
const SVELTE_VITE_CONFIG: &str =
    include_str!("../../../templates/scaffold/demo/frontend/sveltekit/vite.config.ts.tmpl");
const SVELTE_TSCONFIG: &str =
    include_str!("../../../templates/scaffold/demo/frontend/sveltekit/tsconfig.json.tmpl");
const SVELTE_APP_HTML: &str =
    include_str!("../../../templates/scaffold/demo/frontend/sveltekit/app.html.tmpl");
const SVELTE_ENV: &str =
    include_str!("../../../templates/scaffold/demo/frontend/sveltekit/env.tmpl");
const SVELTE_LAYOUT: &str =
    include_str!("../../../templates/scaffold/demo/frontend/sveltekit/routes/layout.svelte.tmpl");
const SVELTE_LAYOUT_TS: &str =
    include_str!("../../../templates/scaffold/demo/frontend/sveltekit/routes/layout.ts.tmpl");
const SVELTE_PAGE: &str =
    include_str!("../../../templates/scaffold/demo/frontend/sveltekit/routes/page.svelte.tmpl");
const SVELTE_ESLINT: &str =
    include_str!("../../../templates/scaffold/demo/frontend/sveltekit/eslint.config.js.tmpl");
const SVELTE_PRETTIERIGNORE: &str =
    include_str!("../../../templates/scaffold/demo/frontend/sveltekit/.prettierignore.tmpl");
const SVELTE_HOME_SPEC: &str =
    include_str!("../../../templates/scaffold/demo/frontend/sveltekit/tests/home.spec.ts.tmpl");
const SVELTE_PLAYWRIGHT_CONFIG: &str =
    include_str!("../../../templates/scaffold/demo/frontend/sveltekit/playwright.config.ts.tmpl");

const EMPTY_SVELTE_PACKAGE_JSON: &str =
    include_str!("../../../templates/scaffold/minimal/frontend/sveltekit/package.json.tmpl");
const EMPTY_SVELTE_CONFIG: &str =
    include_str!("../../../templates/scaffold/minimal/frontend/sveltekit/svelte.config.js.tmpl");
const EMPTY_SVELTE_VITE_CONFIG: &str =
    include_str!("../../../templates/scaffold/minimal/frontend/sveltekit/vite.config.ts.tmpl");
const EMPTY_SVELTE_TSCONFIG: &str =
    include_str!("../../../templates/scaffold/minimal/frontend/sveltekit/tsconfig.json.tmpl");
const EMPTY_SVELTE_APP_HTML: &str =
    include_str!("../../../templates/scaffold/minimal/frontend/sveltekit/app.html.tmpl");
const EMPTY_SVELTE_ENV: &str =
    include_str!("../../../templates/scaffold/minimal/frontend/sveltekit/env.tmpl");
const EMPTY_SVELTE_LAYOUT: &str = include_str!(
    "../../../templates/scaffold/minimal/frontend/sveltekit/routes/layout.svelte.tmpl"
);
const EMPTY_SVELTE_LAYOUT_TS: &str =
    include_str!("../../../templates/scaffold/minimal/frontend/sveltekit/routes/layout.ts.tmpl");
const EMPTY_SVELTE_PAGE: &str =
    include_str!("../../../templates/scaffold/minimal/frontend/sveltekit/routes/page.svelte.tmpl");
const EMPTY_SVELTE_ESLINT: &str =
    include_str!("../../../templates/scaffold/minimal/frontend/sveltekit/eslint.config.js.tmpl");
const EMPTY_SVELTE_PRETTIERIGNORE: &str =
    include_str!("../../../templates/scaffold/minimal/frontend/sveltekit/.prettierignore.tmpl");
const EMPTY_SVELTE_HOME_SPEC: &str =
    include_str!("../../../templates/scaffold/minimal/frontend/sveltekit/tests/home.spec.ts.tmpl");
const EMPTY_SVELTE_PLAYWRIGHT_CONFIG: &str = include_str!(
    "../../../templates/scaffold/minimal/frontend/sveltekit/playwright.config.ts.tmpl"
);

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

const FRONTEND_DIRS: &[&str] = &["src/routes"];

const FRONTEND_DEMO_FILES: &[TemplateFile] = &[
    TemplateFile {
        path: "playwright.config.ts",
        template: SVELTE_PLAYWRIGHT_CONFIG,
        render: false,
    },
    TemplateFile {
        path: "package.json",
        template: SVELTE_PACKAGE_JSON,
        render: true,
    },
    TemplateFile {
        path: "svelte.config.js",
        template: SVELTE_CONFIG,
        render: false,
    },
    TemplateFile {
        path: "vite.config.ts",
        template: SVELTE_VITE_CONFIG,
        render: false,
    },
    TemplateFile {
        path: "tsconfig.json",
        template: SVELTE_TSCONFIG,
        render: false,
    },
    TemplateFile {
        path: "src/app.html",
        template: SVELTE_APP_HTML,
        render: false,
    },
    TemplateFile {
        path: ".env",
        template: SVELTE_ENV,
        render: false,
    },
    TemplateFile {
        path: ".env.example",
        template: SVELTE_ENV,
        render: false,
    },
    TemplateFile {
        path: "eslint.config.js",
        template: SVELTE_ESLINT,
        render: false,
    },
    TemplateFile {
        path: ".prettierignore",
        template: SVELTE_PRETTIERIGNORE,
        render: false,
    },
    TemplateFile {
        path: "src/routes/+layout.svelte",
        template: SVELTE_LAYOUT,
        render: false,
    },
    TemplateFile {
        path: "src/routes/+layout.ts",
        template: SVELTE_LAYOUT_TS,
        render: false,
    },
    TemplateFile {
        path: "src/routes/+page.svelte",
        template: SVELTE_PAGE,
        render: false,
    },
    TemplateFile {
        path: "tests/home.spec.ts",
        template: SVELTE_HOME_SPEC,
        render: false,
    },
];

const FRONTEND_MINIMAL_FILES: &[TemplateFile] = &[
    TemplateFile {
        path: "playwright.config.ts",
        template: EMPTY_SVELTE_PLAYWRIGHT_CONFIG,
        render: false,
    },
    TemplateFile {
        path: "package.json",
        template: EMPTY_SVELTE_PACKAGE_JSON,
        render: true,
    },
    TemplateFile {
        path: "svelte.config.js",
        template: EMPTY_SVELTE_CONFIG,
        render: false,
    },
    TemplateFile {
        path: "vite.config.ts",
        template: EMPTY_SVELTE_VITE_CONFIG,
        render: false,
    },
    TemplateFile {
        path: "tsconfig.json",
        template: EMPTY_SVELTE_TSCONFIG,
        render: false,
    },
    TemplateFile {
        path: "src/app.html",
        template: EMPTY_SVELTE_APP_HTML,
        render: false,
    },
    TemplateFile {
        path: ".env",
        template: EMPTY_SVELTE_ENV,
        render: false,
    },
    TemplateFile {
        path: ".env.example",
        template: EMPTY_SVELTE_ENV,
        render: false,
    },
    TemplateFile {
        path: "eslint.config.js",
        template: EMPTY_SVELTE_ESLINT,
        render: false,
    },
    TemplateFile {
        path: ".prettierignore",
        template: EMPTY_SVELTE_PRETTIERIGNORE,
        render: false,
    },
    TemplateFile {
        path: "src/routes/+layout.svelte",
        template: EMPTY_SVELTE_LAYOUT,
        render: false,
    },
    TemplateFile {
        path: "src/routes/+layout.ts",
        template: EMPTY_SVELTE_LAYOUT_TS,
        render: false,
    },
    TemplateFile {
        path: "src/routes/+page.svelte",
        template: EMPTY_SVELTE_PAGE,
        render: true,
    },
    TemplateFile {
        path: "tests/home.spec.ts",
        template: EMPTY_SVELTE_HOME_SPEC,
        render: false,
    },
];

pub fn sveltekit_project_templates(mode: ScaffoldMode) -> &'static [TemplateFile] {
    match mode {
        ScaffoldMode::Demo => PROJECT_DEMO,
        ScaffoldMode::Minimal => PROJECT_MINIMAL,
    }
}

pub fn sveltekit_frontend_templates(mode: ScaffoldMode) -> FrontendTemplates {
    let files = match mode {
        ScaffoldMode::Demo => FRONTEND_DEMO_FILES,
        ScaffoldMode::Minimal => FRONTEND_MINIMAL_FILES,
    };

    FrontendTemplates {
        directories: FRONTEND_DIRS,
        files,
    }
}
