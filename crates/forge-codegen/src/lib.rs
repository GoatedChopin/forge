#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

pub mod dioxus;
pub mod parser;
pub mod typescript;

pub use dioxus::DioxusGenerator;
pub use parser::parse_project;
pub use typescript::{
    ApiGenerator, ClientGenerator, Error, GenerateOptions, RUNES_SVELTE_TS, StoreGenerator,
    TypeGenerator, TypeScriptGenerator,
};
