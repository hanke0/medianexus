use std::path::PathBuf;
use uuid::Uuid;

/// Process-level config (CLI / env). Integration secrets live in SQLite.
#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub jwt_secret_ephemeral: bool,
    pub host: String,
    pub port: u16,
    pub library_dir: PathBuf,
    pub admin_username: String,
    pub admin_password: String,
}

#[derive(Debug, Default)]
pub struct CliArgs {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database_url: Option<String>,
    pub library_dir: Option<PathBuf>,
    pub jwt_secret: Option<String>,
    pub admin_username: Option<String>,
    pub admin_password: Option<String>,
}

impl CliArgs {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Self {
        let mut cli = CliArgs::default();
        let mut iter = args.into_iter().skip(1);
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--host" => cli.host = iter.next(),
                "--port" => {
                    cli.port = iter.next().and_then(|p| p.parse().ok());
                }
                "--database-url" | "--db" => cli.database_url = iter.next(),
                "--library-dir" | "--library" => {
                    cli.library_dir = iter.next().map(PathBuf::from);
                }
                "--jwt-secret" => cli.jwt_secret = iter.next(),
                "--admin-username" => cli.admin_username = iter.next(),
                "--admin-password" => cli.admin_password = iter.next(),
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                "--version" | "-V" => {
                    // Handled early in main; keep here so unknown-flag noise is avoided.
                    print_version();
                    std::process::exit(0);
                }
                other if other.starts_with("--host=") => {
                    cli.host = Some(other.trim_start_matches("--host=").to_string());
                }
                other if other.starts_with("--port=") => {
                    cli.port = other.trim_start_matches("--port=").parse().ok();
                }
                other if other.starts_with("--database-url=") => {
                    cli.database_url =
                        Some(other.trim_start_matches("--database-url=").to_string());
                }
                other if other.starts_with("--db=") => {
                    cli.database_url = Some(other.trim_start_matches("--db=").to_string());
                }
                other if other.starts_with("--library-dir=") => {
                    cli.library_dir =
                        Some(PathBuf::from(other.trim_start_matches("--library-dir=")));
                }
                other if other.starts_with("--library=") => {
                    cli.library_dir = Some(PathBuf::from(other.trim_start_matches("--library=")));
                }
                other if other.starts_with("--jwt-secret=") => {
                    cli.jwt_secret = Some(other.trim_start_matches("--jwt-secret=").to_string());
                }
                _ => {}
            }
        }
        cli
    }
}

fn print_help() {
    eprintln!(
        "\
MediaNexus backend

Usage:
  medianexus-backend [options]

Options:
  --host <addr>              Listen address (env HOST, default 127.0.0.1)
  --port <port>              Listen port (env PORT, default 8080)
  --database-url <url>       SQLite URL (env DATABASE_URL)
                             default: sqlite:medianexus.db?mode=rwc
  --db <url>                 Alias for --database-url
  --library-dir <path>       Hardlink destination (env LIBRARY_DIR)
                             default: ./nexusmedia
  --library <path>           Alias for --library-dir
  --jwt-secret <secret>      JWT secret (env JWT_SECRET); if omitted, random each start
  --admin-username <name>    Bootstrap admin (env ADMIN_USERNAME, default admin)
  --admin-password <pass>    Bootstrap admin (env ADMIN_PASSWORD, default admin123)
  -h, --help                 Show help
  -V, --version              Show version
"
    );
}

fn print_version() {
    println!("medianexus-backend {}", env!("CARGO_PKG_VERSION"));
    #[cfg(feature = "embed-frontend")]
    println!("  + embed-frontend");
}

impl Config {
    pub fn load() -> Self {
        // Process environment only — do not auto-load a `.env` file.
        let cli = CliArgs::parse(std::env::args());
        Self::from_cli_and_env(cli)
    }

    pub fn from_cli_and_env(cli: CliArgs) -> Self {
        let (jwt_secret, jwt_secret_ephemeral) =
            match cli.jwt_secret.or_else(|| optional_env("JWT_SECRET")) {
                Some(s) => (s, false),
                None => (generate_jwt_secret(), true),
            };

        let database_url = cli
            .database_url
            .or_else(|| optional_env("DATABASE_URL"))
            .unwrap_or_else(|| "sqlite:medianexus.db?mode=rwc".into());

        let library_dir = cli
            .library_dir
            .or_else(|| optional_env("LIBRARY_DIR").map(PathBuf::from))
            .or_else(|| optional_env("HARDLINK_ROOT").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("nexusmedia"));

        Self {
            database_url,
            jwt_secret,
            jwt_secret_ephemeral,
            host: cli
                .host
                .or_else(|| optional_env("HOST"))
                .unwrap_or_else(|| "127.0.0.1".into()),
            port: cli
                .port
                .or_else(|| optional_env("PORT").and_then(|p| p.parse().ok()))
                .unwrap_or(8080),
            library_dir,
            admin_username: cli
                .admin_username
                .or_else(|| optional_env("ADMIN_USERNAME"))
                .unwrap_or_else(|| "admin".into()),
            admin_password: cli
                .admin_password
                .or_else(|| optional_env("ADMIN_PASSWORD"))
                .unwrap_or_else(|| "admin123".into()),
        }
    }
}

fn optional_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn generate_jwt_secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
