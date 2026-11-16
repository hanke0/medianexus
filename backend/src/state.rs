use crate::config::Config;
use crate::domain::settings::{IntegrationClients, IntegrationSettings};
use crate::error::AppResult;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: Arc<Config>,
    pub http: reqwest::Client,
    integrations: Arc<RwLock<IntegrationBundle>>,
}

struct IntegrationBundle {
    settings: IntegrationSettings,
    clients: IntegrationClients,
}

impl AppState {
    pub async fn new(db: SqlitePool, config: Config) -> AppResult<Self> {
        let http = reqwest::Client::builder()
            .cookie_store(true)
            .user_agent("MediaNexus/0.1")
            .build()
            .expect("http client");

        let settings = crate::domain::settings::load(&db).await?;
        let clients = settings.build_clients(http.clone());

        Ok(Self {
            db,
            config: Arc::new(config),
            http,
            integrations: Arc::new(RwLock::new(IntegrationBundle { settings, clients })),
        })
    }

    pub async fn settings(&self) -> IntegrationSettings {
        self.integrations.read().await.settings.clone()
    }

    pub async fn clients(&self) -> IntegrationClients {
        self.integrations.read().await.clients.clone()
    }

    pub async fn reload_integrations(&self) -> AppResult<()> {
        let settings = crate::domain::settings::load(&self.db).await?;
        let clients = settings.build_clients(self.http.clone());
        let mut guard = self.integrations.write().await;
        guard.settings = settings;
        guard.clients = clients;
        Ok(())
    }
}
