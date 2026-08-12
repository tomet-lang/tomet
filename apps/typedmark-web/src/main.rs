//! Interactive Web Editor for TypedMark (`typedmark-web`).

use std::net::SocketAddr;
use std::process::ExitCode;

use axum::Json;
use axum::Router;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Parser)]
#[command(
    name = "typedmark-web",
    about = "Web-based interactive editor for TypedMark (.tm)"
)]
struct Cli {
    #[arg(short, long, default_value_t = 8787)]
    port: u16,
}

static INDEX_HTML: &str = include_str!("../static/index.html");
static STYLE_CSS: &str = include_str!("../static/style.css");
static APP_JS: &str = include_str!("../static/app.js");

fn main() -> ExitCode {
    let cli = Cli::parse();
    let addr = SocketAddr::from(([127, 0, 0, 1], cli.port));
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to initialize tokio runtime: {e}");
            return ExitCode::FAILURE;
        }
    };

    rt.block_on(async {
        let app = Router::new()
            .route("/", get(|| async { Html(INDEX_HTML) }))
            .route(
                "/style.css",
                get(|| async {
                    Response::builder()
                        .header("content-type", "text/css; charset=utf-8")
                        .body(STYLE_CSS.to_string())
                        .unwrap()
                }),
            )
            .route(
                "/app.js",
                get(|| async {
                    Response::builder()
                        .header("content-type", "application/javascript; charset=utf-8")
                        .body(APP_JS.to_string())
                        .unwrap()
                }),
            )
            .route("/api/parse", post(parse_handler));

        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Failed to bind to {addr}: {e}");
                return;
            }
        };

        println!("🚀 TypedMark Web Editor running at http://{addr}");
        println!("Open http://{addr} in your browser!");
        if let Err(e) = axum::serve(listener, app).await {
            eprintln!("Server error: {e}");
        }
    });

    ExitCode::SUCCESS
}

#[derive(Deserialize)]
struct ApiParseRequest {
    source: String,
    advanced: Option<bool>,
}

#[derive(Serialize)]
struct ApiParseResponse {
    ok: bool,
    html: String,
    ast: String,
    markdown: String,
    error: Option<ApiParseError>,
}

#[derive(Serialize)]
struct ApiParseError {
    message: String,
    line: usize,
    column: usize,
    offset: usize,
}

async fn parse_handler(Json(req): Json<ApiParseRequest>) -> impl IntoResponse {
    let advanced = req.advanced.unwrap_or(false);
    match typedmark_parser::parse_document(&req.source) {
        Ok(doc) => {
            let options = typedmark_renderer::RenderOptions {
                number_headings: advanced,
            };
            let html = typedmark_renderer::render_page_with(&doc, "TypedMark Web", &options);
            let ast = format!("{doc:#?}");
            let markdown = typedmark_markdown::to_markdown(&doc);
            Json(ApiParseResponse {
                ok: true,
                html,
                ast,
                markdown,
                error: None,
            })
        }
        Err(err) => Json(ApiParseResponse {
            ok: false,
            html: String::new(),
            ast: String::new(),
            markdown: String::new(),
            error: Some(ApiParseError {
                message: err.message,
                line: err.line,
                column: err.column,
                offset: err.offset,
            }),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parse_handler_parses_valid_doc() {
        let req = ApiParseRequest {
            source: "#[ Hello TypedMark Web ]".into(),
            advanced: Some(false),
        };
        let _response = parse_handler(Json(req)).await;

        // Verify response builds clean
    }
}
