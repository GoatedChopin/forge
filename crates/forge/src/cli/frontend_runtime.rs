use anyhow::Result;
use std::fs;
use std::path::Path;

pub struct RuntimeTemplate {
    pub path: &'static str,
    pub template: &'static str,
    pub allow_dead_code: bool,
}

const SVELTE_PACKAGE_JSON: &str =
    include_str!("../../templates/runtime/sveltekit/package.json.tmpl");
const SVELTE_TYPES_TS: &str = include_str!("../../templates/runtime/sveltekit/types.ts.tmpl");
const SVELTE_CLIENT_TS: &str = include_str!("../../templates/runtime/sveltekit/client.ts.tmpl");
const SVELTE_CONTEXT_TS: &str = include_str!("../../templates/runtime/sveltekit/context.ts.tmpl");
const SVELTE_STORES_TS: &str = include_str!("../../templates/runtime/sveltekit/stores.ts.tmpl");
const SVELTE_DATETIME_TS: &str = include_str!("../../templates/runtime/sveltekit/datetime.ts.tmpl");
const SVELTE_PROVIDER: &str =
    include_str!("../../templates/runtime/sveltekit/ForgeProvider.svelte.tmpl");
const SVELTE_INDEX_TS: &str = include_str!("../../templates/runtime/sveltekit/index.ts.tmpl");

const DIOXUS_CARGO_TOML: &str = include_str!("../../templates/runtime/dioxus/Cargo.toml.tmpl");
const DIOXUS_LIB_RS: &str = include_str!("../../templates/runtime/dioxus/src/lib.rs.tmpl");
const DIOXUS_TYPES_RS: &str = include_str!("../../templates/runtime/dioxus/src/types.rs.tmpl");
const DIOXUS_UPLOAD_RS: &str = include_str!("../../templates/runtime/dioxus/src/upload.rs.tmpl");
const DIOXUS_CLIENT_RS: &str = include_str!("../../templates/runtime/dioxus/src/client.rs.tmpl");
const DIOXUS_CONTEXT_RS: &str = include_str!("../../templates/runtime/dioxus/src/context.rs.tmpl");
const DIOXUS_HOOKS_RS: &str = include_str!("../../templates/runtime/dioxus/src/hooks.rs.tmpl");

pub(crate) const SVELTE_RUNTIME_FILES: &[RuntimeTemplate] = &[
    RuntimeTemplate {
        path: "svelte/package.json",
        template: SVELTE_PACKAGE_JSON,
        allow_dead_code: false,
    },
    RuntimeTemplate {
        path: "svelte/types.ts",
        template: SVELTE_TYPES_TS,
        allow_dead_code: false,
    },
    RuntimeTemplate {
        path: "svelte/client.ts",
        template: SVELTE_CLIENT_TS,
        allow_dead_code: false,
    },
    RuntimeTemplate {
        path: "svelte/context.ts",
        template: SVELTE_CONTEXT_TS,
        allow_dead_code: false,
    },
    RuntimeTemplate {
        path: "svelte/stores.ts",
        template: SVELTE_STORES_TS,
        allow_dead_code: false,
    },
    RuntimeTemplate {
        path: "svelte/datetime.ts",
        template: SVELTE_DATETIME_TS,
        allow_dead_code: false,
    },
    RuntimeTemplate {
        path: "svelte/ForgeProvider.svelte",
        template: SVELTE_PROVIDER,
        allow_dead_code: false,
    },
    RuntimeTemplate {
        path: "svelte/index.ts",
        template: SVELTE_INDEX_TS,
        allow_dead_code: false,
    },
];

pub(crate) const DIOXUS_RUNTIME_FILES: &[RuntimeTemplate] = &[
    RuntimeTemplate {
        path: "dioxus/Cargo.toml",
        template: DIOXUS_CARGO_TOML,
        allow_dead_code: false,
    },
    RuntimeTemplate {
        path: "dioxus/src/lib.rs",
        template: DIOXUS_LIB_RS,
        allow_dead_code: false,
    },
    RuntimeTemplate {
        path: "dioxus/src/types.rs",
        template: DIOXUS_TYPES_RS,
        allow_dead_code: true,
    },
    RuntimeTemplate {
        path: "dioxus/src/upload.rs",
        template: DIOXUS_UPLOAD_RS,
        allow_dead_code: true,
    },
    RuntimeTemplate {
        path: "dioxus/src/client.rs",
        template: DIOXUS_CLIENT_RS,
        allow_dead_code: false,
    },
    RuntimeTemplate {
        path: "dioxus/src/context.rs",
        template: DIOXUS_CONTEXT_RS,
        allow_dead_code: false,
    },
    RuntimeTemplate {
        path: "dioxus/src/hooks.rs",
        template: DIOXUS_HOOKS_RS,
        allow_dead_code: false,
    },
];

pub(crate) fn update_frontend_package_json(frontend_dir: &Path) -> Result<()> {
    let package_json_path = frontend_dir.join("package.json");
    if !package_json_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&package_json_path)?;
    if content.contains("\"@forge/svelte\"") {
        return Ok(());
    }

    let mut json: serde_json::Value = serde_json::from_str(&content)?;
    if let Some(deps) = json.get_mut("dependencies") {
        if let Some(obj) = deps.as_object_mut() {
            obj.insert(
                "@forge/svelte".to_string(),
                serde_json::Value::String("file:./.forge/svelte".to_string()),
            );
        }
    } else {
        let mut deps = serde_json::Map::new();
        deps.insert(
            "@forge/svelte".to_string(),
            serde_json::Value::String("file:./.forge/svelte".to_string()),
        );
        if let Some(obj) = json.as_object_mut() {
            obj.insert("dependencies".to_string(), serde_json::Value::Object(deps));
        }
    }

    fs::write(&package_json_path, serde_json::to_string_pretty(&json)?)?;
    Ok(())
}

pub(crate) fn update_frontend_cargo_toml(frontend_dir: &Path) -> Result<()> {
    let cargo_toml_path = frontend_dir.join("Cargo.toml");
    if !cargo_toml_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&cargo_toml_path)?;
    if content.contains("forge-dioxus") || content.contains("forge_dioxus") {
        return Ok(());
    }

    let mut cargo: toml::Value = toml::from_str(&content)?;
    if cargo.get("dependencies").is_none() {
        cargo
            .as_table_mut()
            .expect("Cargo.toml root should be a table")
            .insert(
                "dependencies".to_string(),
                toml::Value::Table(toml::map::Map::new()),
            );
    }

    if let Some(deps) = cargo
        .get_mut("dependencies")
        .and_then(|value| value.as_table_mut())
    {
        let mut dep = toml::map::Map::new();
        dep.insert(
            "path".to_string(),
            toml::Value::String("./.forge/dioxus".to_string()),
        );
        dep.insert(
            "package".to_string(),
            toml::Value::String("forge-dioxus".to_string()),
        );
        deps.insert("forge_dioxus".to_string(), toml::Value::Table(dep));
    }

    fs::write(&cargo_toml_path, toml::to_string_pretty(&cargo)?)?;
    Ok(())
}
