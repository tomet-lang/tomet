use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use axum::Router;
use axum::extract::State;
use axum::response::Html;
use axum::routing::get;

use crate::commands::html::render_file;

#[derive(Clone)]
struct ServeState {
    file: PathBuf,
    advanced: bool,
    lang: Option<String>,
}

pub(crate) fn serve(
    file: &PathBuf,
    port: u16,
    advanced: bool,
    lang: Option<String>,
) -> anyhow::Result<()> {
    let state = ServeState {
        file: file.clone(),
        advanced,
        lang,
    };
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let app = Router::new()
            .route("/", get(render_handler))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind(addr).await?;
        println!("Serving at http://{addr} (Ctrl+C to stop)");
        axum::serve(listener, app).await?;
        Ok(())
    })
}

async fn render_handler(State(state): State<ServeState>) -> Html<String> {
    Html(
        render_file(&state.file, state.advanced, state.lang.clone())
            .unwrap_or_else(|e| error_page(&state.file, &e)),
    )
}

fn error_page(file: &Path, err: &anyhow::Error) -> String {
    format!(
        "<!DOCTYPE html>\n<html lang=\"ja\">\n<head><meta charset=\"utf-8\"><title>error</title></head>\n<body>\n<h1>Parse error</h1>\n<p>{}</p>\n<pre>{}</pre>\n</body>\n</html>\n",
        escape_html(&file.display().to_string()),
        escape_html(&err.to_string()),
    )
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
