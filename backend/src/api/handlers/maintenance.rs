use crate::api::middleware::auth::AuthContext;
use crate::domain::maintenance::{self, LibraryItemRow, MaintenanceRunResult, MaintenanceToolDto};
use crate::error::AppResult;
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    Extension, Json,
};
use serde::Deserialize;
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct RunToolRequest {
    pub tool: String,
}

#[derive(Deserialize)]
pub struct LibraryItemsQuery {
    pub media_type: Option<String>,
    pub source: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

#[utoipa::path(
    get,
    path = "/maintenance/tools",
    tag = "Maintenance",
    responses((status = 200, body = [MaintenanceToolDto])),
    security(("bearer_auth" = []))
)]
pub async fn list_tools(
    Extension(AuthContext(user)): Extension<AuthContext>,
) -> AppResult<Json<Vec<MaintenanceToolDto>>> {
    if !user.is_admin() {
        return Err(crate::error::AppError::Forbidden("admin only".into()));
    }
    Ok(Json(maintenance::list_tools()))
}

#[utoipa::path(
    post,
    path = "/maintenance/run",
    tag = "Maintenance",
    request_body = RunToolRequest,
    responses((status = 200, body = MaintenanceRunResult)),
    security(("bearer_auth" = []))
)]
pub async fn run_tool(
    State(state): State<AppState>,
    Extension(AuthContext(user)): Extension<AuthContext>,
    Json(req): Json<RunToolRequest>,
) -> AppResult<Json<MaintenanceRunResult>> {
    Ok(Json(
        maintenance::run_tool(&state, &user, req.tool.trim()).await?,
    ))
}

#[utoipa::path(
    get,
    path = "/maintenance/library-items",
    tag = "Maintenance",
    params(
        ("media_type" = Option<String>, Query, description = "movie | tv"),
        ("source" = Option<String>, Query, description = "plex | jellyfin"),
        ("limit" = Option<i64>, Query, description = "page size"),
        ("offset" = Option<i64>, Query, description = "offset")
    ),
    responses((status = 200, body = [LibraryItemRow])),
    security(("bearer_auth" = []))
)]
pub async fn list_library_items(
    State(state): State<AppState>,
    Extension(AuthContext(user)): Extension<AuthContext>,
    Query(q): Query<LibraryItemsQuery>,
) -> AppResult<Json<Vec<LibraryItemRow>>> {
    Ok(Json(
        maintenance::list_library_items(
            &state,
            &user,
            q.media_type.as_deref(),
            q.source.as_deref(),
            q.limit,
            q.offset,
        )
        .await?,
    ))
}
