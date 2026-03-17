use forge::prelude::*;

mod functions;
mod schema;

#[tokio::main]
async fn main() -> Result<()> {
    let config_path = std::env::var("FORGE_CONFIG")
        .map_err(|_| ForgeError::Config("FORGE_CONFIG is required for benchmark app".into()))?;
    let config = ForgeConfig::from_file(&config_path)?;

    Forge::builder()
        .register_mutation::<functions::RegisterMutation>()
        .register_mutation::<functions::CreateCounterMutation>()
        .register_mutation::<functions::IncrementMutation>()
        .register_query::<functions::GetCounterQuery>()
        .register_query::<functions::ListCountersQuery>()
        .config(config)
        .build()?
        .run()
        .await
}
