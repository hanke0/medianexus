use crate::api::handlers;
use crate::api::middleware::auth::require_auth;
use crate::openapi::ApiDoc;
use crate::state::AppState;
use axum::{
    middleware,
    routing::{delete, get, post},
    Router,
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnFailure, TraceLayer};
use tracing::Level;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub fn build_router(state: AppState) -> Router {
    let public = Router::new()
        .route("/health", get(handlers::health::health))
        .route("/auth/login", post(handlers::auth::login));

    let protected = Router::new()
        .route("/auth/me", get(handlers::auth::me))
        .route(
            "/users",
            get(handlers::users::list_users).post(handlers::users::create_user),
        )
        .route("/users/{id}", delete(handlers::users::delete_user))
        .route("/media/search", get(handlers::media::search_media))
        .route("/media/movies", get(handlers::media::list_movies))
        .route("/media/tv", get(handlers::media::list_tv))
        .route(
            "/media/{media_type}/{tmdb_id}/library",
            get(handlers::media::library_presence),
        )
        .route(
            "/media/{media_type}/{tmdb_id}",
            get(handlers::media::get_media),
        )
        .route(
            "/tracks",
            get(handlers::tracks::list_tracks).post(handlers::tracks::create_track),
        )
        .route(
            "/tracks/by-media/{media_type}/{tmdb_id}",
            get(handlers::tracks::find_track_by_media),
        )
        .route(
            "/tracks/{id}",
            get(handlers::tracks::get_track).delete(handlers::tracks::delete_track),
        )
        .route("/tracks/{id}/cancel", post(handlers::tracks::cancel_track))
        .route("/tracks/{id}/trigger", post(handlers::tracks::trigger_track))
        .route(
            "/settings",
            get(handlers::settings::get_settings).put(handlers::settings::update_settings),
        )
        .route(
            "/settings/plex/link/start",
            post(handlers::settings::plex_link_start),
        )
        .route(
            "/settings/plex/link/status",
            get(handlers::settings::plex_link_status),
        )
        .route(
            "/settings/plex/link",
            delete(handlers::settings::plex_unlink),
        )
        .route(
            "/settings/test/{target}",
            post(handlers::settings::test_connection),
        )
        .route("/maintenance/tools", get(handlers::maintenance::list_tools))
        .route("/maintenance/run", post(handlers::maintenance::run_tool))
        .route(
            "/maintenance/library-items",
            get(handlers::maintenance::list_library_items),
        )
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    let api = Router::new().merge(public).merge(protected);

    let router = Router::new()
        .nest("/api", api)
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()));

    #[cfg(feature = "embed-frontend")]
    let router = router.fallback(crate::api::embed::static_handler);

    router
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_failure(DefaultOnFailure::new().level(Level::ERROR)),
        )
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state)
}
