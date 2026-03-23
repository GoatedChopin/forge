use forge::prelude::*;

mod functions;
mod schema;

#[cfg(feature = "embedded-frontend")]
#[derive(rust_embed::Embed)]
#[folder = "frontend/build"]
struct Assets;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let config = ForgeConfig::from_file("forge.toml")?;
    let builder = Forge::builder().auto_register();

    #[cfg(feature = "embedded-frontend")]
    let builder = builder.frontend_handler(forge::serve_embedded_assets::<Assets>);

    builder.config(config).build()?.run().await
}
