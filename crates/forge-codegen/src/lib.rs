#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

pub mod parser;
pub mod typescript;

pub use parser::parse_project;
pub use typescript::{
    ApiGenerator, ClientGenerator, Error, GenerateOptions, StoreGenerator, TypeGenerator,
    TypeScriptGenerator,
};
