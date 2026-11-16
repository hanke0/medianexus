use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../frontend/dist/"]
struct FrontendAssets;

pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.is_empty() {
        return serve_index();
    }
    if let Some(file) = FrontendAssets::get(path) {
        return asset_response(path, file.data.as_ref());
    }
    // SPA client routes (e.g. /tracks, /settings) fall back to index.html.
    serve_index()
}

fn serve_index() -> Response {
    match FrontendAssets::get("index.html") {
        Some(file) => asset_response("index.html", file.data.as_ref()),
        None => (
            StatusCode::NOT_FOUND,
            "frontend not embedded; run npm run build:bundle",
        )
            .into_response(),
    }
}

fn asset_response(path: &str, data: &[u8]) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .body(Body::from(data.to_vec()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
