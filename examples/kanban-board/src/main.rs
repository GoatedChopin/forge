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

            Assets::get(path)
                .map(|f| (f, path))
                .or_else(|| Assets::get("index.html").map(|f| (f, "index.html")))
                .map(|(f, p)| {
                    let mime = mime_guess::from_path(p).first_or_octet_stream();
                    (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, mime.as_ref().to_owned())],
                        f.data,
                    )
                        .into_response()
                })
                .unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let config = ForgeConfig::from_file("forge.toml")?;
    let builder = Forge::builder()
        .register_mutation::<functions::RegisterMutation>()
        .register_mutation::<functions::LoginMutation>()
        .register_query::<functions::ListProjectsQuery>()
        .register_query::<functions::GetProjectQuery>()
        .register_mutation::<functions::CreateProjectMutation>()
        .register_mutation::<functions::UpdateProjectMutation>()
        .register_mutation::<functions::UnarchiveProjectMutation>()
        .register_query::<functions::ListTasksQuery>()
        .register_mutation::<functions::CreateTaskMutation>()
        .register_mutation::<functions::UpdateTaskMutation>()
        .register_mutation::<functions::DeleteTaskMutation>()
        .register_mutation::<functions::MoveTaskMutation>()
        .register_job::<functions::ExportProjectJob>()
        .register_cron::<functions::OverdueCheckerCron>()
        .register_workflow::<functions::ScheduleProjectArchiveWorkflow>();

    #[cfg(feature = "embedded-frontend")]
    let builder = builder.frontend_handler(embedded::serve_frontend);

    builder.config(config).build()?.run().await
}
