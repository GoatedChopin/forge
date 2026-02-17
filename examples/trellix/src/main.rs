use forge::prelude::*;

mod functions;
mod schema;

#[cfg(feature = "embedded-frontend")]
mod embedded {
    use axum::{
        body::Body,
        http::Request,
        http::{StatusCode, header},
        response::{IntoResponse, Response},
    };
    use rust_embed::Embed;
    use std::{future::Future, pin::Pin};

    #[derive(Embed)]
    #[folder = "frontend/build"]
    pub struct Assets;

    pub fn serve_frontend(req: Request<Body>) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        Box::pin(async move {
            let path = req.uri().path().trim_start_matches('/');

            let (file, asset_path) = Assets::get(path)
                .map(|f| (f, path))
                .or_else(|| Assets::get("index.html").map(|f| (f, "index.html")))
                .map(|(f, p)| {
                    let mime = mime_guess::from_path(p).first_or_octet_stream();
                    (StatusCode::OK, [(header::CONTENT_TYPE, mime.as_ref().to_owned())], f.data).into_response()
                })
                .unwrap_or_else(|| StatusCode::NOT_FOUND.into_response());
            file
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = ForgeConfig::from_file("forge.toml")?;
    let mut builder = Forge::builder();

    let reg = builder.function_registry_mut();
    reg.register_mutation::<functions::RegisterMutation>();
    reg.register_mutation::<functions::LoginMutation>();
    reg.register_query::<functions::ListProjectsQuery>();
    reg.register_query::<functions::GetProjectQuery>();
    reg.register_mutation::<functions::CreateProjectMutation>();
    reg.register_mutation::<functions::UpdateProjectMutation>();
    reg.register_mutation::<functions::UnarchiveProjectMutation>();
    reg.register_query::<functions::ListTasksQuery>();
    reg.register_mutation::<functions::CreateTaskMutation>();
    reg.register_mutation::<functions::UpdateTaskMutation>();
    reg.register_mutation::<functions::DeleteTaskMutation>();
    reg.register_mutation::<functions::MoveTaskMutation>();
    builder.job_registry_mut().register::<functions::ExportProjectJob>();
    builder.cron_registry_mut().register::<functions::OverdueCheckerCron>();
    builder.workflow_registry_mut().register::<functions::ScheduleProjectArchiveWorkflow>();

    #[cfg(feature = "embedded-frontend")]
    builder.frontend_handler(embedded::serve_frontend);

    builder.config(config).build()?.run().await
}
