use medianexus_backend::api::router::build_router;
use medianexus_backend::config::Config;
use medianexus_backend::domain::auth::ensure_default_admin;
use medianexus_backend::domain::settings;
use medianexus_backend::infra::db;
use medianexus_backend::state::AppState;
use medianexus_backend::worker;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_version() {
    println!("medianexus-backend {VERSION}");
    #[cfg(feature = "embed-frontend")]
    println!("  + embed-frontend");
}

fn version_flag_requested() -> bool {
    std::env::args().any(|a| a == "--version" || a == "-V")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if version_flag_requested() {
        print_version();
        return Ok(());
    }

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "medianexus_backend=debug,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::load();
    if config.jwt_secret_ephemeral {
        tracing::warn!(
            "JWT_SECRET not set; using ephemeral secret for this process (tokens reset on restart)"
        );
    }
    tracing::info!(
        library_dir = %config.library_dir.display(),
        database_url = %config.database_url,
        "runtime config"
    );

    let pool = db::connect(&config).await?;
    settings::seed_defaults_if_empty(&pool).await?;
    medianexus_backend::infra::plex::ensure_device_identity(&pool).await?;
    let state = AppState::new(pool, config.clone()).await?;
    if let Err(err) =
        medianexus_backend::infra::plex::sync_server_access(&state.http, &state.db).await
    {
        tracing::warn!(error = %err, "Plex PMS token sync on startup failed");
    } else if let Err(err) = state.reload_integrations().await {
        tracing::warn!(error = %err, "reload integrations after Plex sync failed");
    }
    ensure_default_admin(&state).await?;
    worker::spawn(state.clone());

    let app = build_router(state);
    let addr = format!("{}:{}", config.host, config.port);
    tracing::info!("MediaNexus listening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
