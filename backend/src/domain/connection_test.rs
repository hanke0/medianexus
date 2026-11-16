use crate::domain::auth::AuthUser;
use crate::domain::settings;
use crate::error::{AppError, AppResult};
use crate::infra::media_server;
use crate::infra::plex::PlexClient;
use crate::infra::pt::mteam::MTeamIndexer;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionTarget {
    Mteam,
    Qbittorrent,
    Jellyfin,
    Plex,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ConnectionTestResult {
    pub ok: bool,
    pub message: String,
}

pub async fn test_connection(
    state: &AppState,
    actor: &AuthUser,
    target: ConnectionTarget,
) -> AppResult<ConnectionTestResult> {
    if !actor.is_admin() {
        return Err(AppError::Forbidden("admin only".into()));
    }

    // Ensure we test the latest saved settings (UI flushes before calling).
    state.reload_integrations().await?;
    let settings = settings::load(&state.db).await?;

    let result = match target {
        ConnectionTarget::Mteam => {
            let client = MTeamIndexer::new(
                state.http.clone(),
                settings.mteam_api_base.clone(),
                settings.mteam_api_key.clone(),
            );
            client
                .test_connection()
                .await
                .map(|_| "M-Team 连接成功".into())
        }
        ConnectionTarget::Qbittorrent => {
            let clients = state.clients().await;
            clients
                .qb
                .test_connection()
                .await
                .map(|_| "qBittorrent 连接成功".into())
        }
        ConnectionTarget::Jellyfin => media_server::test_jellyfin(
            &state.http,
            &settings.jellyfin_url,
            &settings.jellyfin_api_key,
        )
        .await
        .map(|_| "Jellyfin 连接成功".into()),
        ConnectionTarget::Plex => {
            let Some(plex) = PlexClient::try_from_settings(state.http.clone(), &settings) else {
                return Ok(ConnectionTestResult {
                    ok: false,
                    message: "Plex 未配置：需要 URL 并用账号完成登录关联".into(),
                });
            };
            plex.test_connection().await.map(|_| "Plex 连接成功".into())
        }
    };

    match result {
        Ok(message) => Ok(ConnectionTestResult { ok: true, message }),
        Err(e) => Ok(ConnectionTestResult {
            ok: false,
            message: e.to_string(),
        }),
    }
}
