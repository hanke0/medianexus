use crate::api::handlers;
use crate::domain::maintenance::{LibraryItemRow, MaintenanceRunResult, MaintenanceToolDto};
use crate::domain::connection_test::ConnectionTestResult;
use crate::domain::settings::IntegrationSettings;
use crate::domain::track::{CreateTrackRequest, Track};
use crate::error::ApiErrorBody;
use crate::infra::media_server::LibraryPresence;
use crate::infra::tmdb::MediaItem;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::health::health,
        handlers::auth::login,
        handlers::auth::me,
        handlers::users::list_users,
        handlers::users::create_user,
        handlers::users::delete_user,
        handlers::media::search_media,
        handlers::media::list_movies,
        handlers::media::list_tv,
        handlers::media::get_media,
        handlers::media::library_presence,
        handlers::tracks::list_tracks,
        handlers::tracks::create_track,
        handlers::tracks::get_track,
        handlers::tracks::cancel_track,
        handlers::tracks::trigger_track,
        handlers::tracks::find_track_by_media,
        handlers::tracks::delete_track,
        handlers::settings::get_settings,
        handlers::settings::update_settings,
        handlers::settings::plex_link_start,
        handlers::settings::plex_link_status,
        handlers::settings::plex_unlink,
        handlers::settings::test_connection,
        handlers::maintenance::list_tools,
        handlers::maintenance::run_tool,
        handlers::maintenance::list_library_items,
    ),
    components(schemas(
        ApiErrorBody,
        handlers::auth::LoginRequest,
        handlers::auth::LoginResponse,
        handlers::auth::UserInfo,
        handlers::users::UserDto,
        handlers::users::CreateUserRequest,
        handlers::health::HealthResponse,
        MediaItem,
        Track,
        CreateTrackRequest,
        IntegrationSettings,
        ConnectionTestResult,
        handlers::settings::PlexLinkStartRequest,
        crate::infra::plex::PlexLinkStart,
        crate::infra::plex::PlexLinkStatus,
        LibraryPresence,
        crate::infra::media_server::LibraryMatch,
        MaintenanceToolDto,
        MaintenanceRunResult,
        handlers::maintenance::RunToolRequest,
        LibraryItemRow,
    )),
    modifiers(&SecurityAddon),
    tags(
        (name = "Health"),
        (name = "Auth"),
        (name = "Users"),
        (name = "Media"),
        (name = "Tracks"),
        (name = "Settings"),
        (name = "Maintenance"),
    ),
    servers((url = "/api"))
)]
pub struct ApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
    }
}
