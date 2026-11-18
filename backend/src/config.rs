use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
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
    /// MDBList lists shown in the browse dropdown, after the built-in catalogs.
    pub browse_lists: Vec<BrowseList>,
}

/// An MDBList list addressed as `movie:user/slug` or `tv:user/slug`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowseList {
    pub user: String,
    pub slug: String,
    pub media_type: String,
}

impl BrowseList {
    pub fn id(&self) -> String {
        format!("list:{}/{}", self.user, self.slug)
    }

    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    pub fn label(&self) -> String {
        match (self.user.as_str(), self.slug.as_str()) {
            ("snoak", "imdb-top-250-movies") => "IMDb Top 250".into(),
            ("snoak", "imdb-top-250-shows") => "IMDb Top 250".into(),
            ("snoak", "most-popular-movies-on-rotten-tomatoes") => "烂番茄最热".into(),
            ("snoak", "most-popular-shows-on-rotten-tomatoes") => "烂番茄最热".into(),
            _ => self.slug.replace('-', " "),
        }
    }
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
    pub browse_lists: Vec<String>,
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
                "--list" => {
                    if let Some(value) = iter.next() {
                        cli.browse_lists.push(value);
                    }
                }
                "--lists" => {
                    if let Some(value) = iter.next() {
                        push_list_tokens(&mut cli.browse_lists, &value);
                    }
                }
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
                other if other.starts_with("--list=") => {
                    cli.browse_lists
                        .push(other.trim_start_matches("--list=").to_string());
                }
                other if other.starts_with("--lists=") => {
                    push_list_tokens(&mut cli.browse_lists, other.trim_start_matches("--lists="));
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
  --host <addr>              Listen address (env HOST, default lan)
                             lan: 127.0.0.1, assigned ::1, and private LAN
                             only (RFC1918 and IPv6 ULA)
                             0.0.0.0 or :: listens on every interface,
                             including public addresses
                             any other address is bound as given
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
  --list <movie|tv:user/slug>
                             Extra MDBList browse list. Repeatable.
                             The type is required: movie:user/slug or tv:user/slug.
                             The built-in dropdown always includes TMDB 最新/最热/高分,
                             IMDb 最热, and snoak's IMDb Top 250 / Rotten Tomatoes lists.
  --lists <a,b>              Comma-separated extra lists (env BROWSE_LISTS)
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
        let browse_lists = configured_browse_lists(&cli);
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
                .unwrap_or_else(|| "lan".into()),
            port: cli
                .port
                .or_else(|| optional_env("PORT").and_then(|p| p.parse().ok()))
                .unwrap_or(8080),
            library_dir,
            admin_username: cli
                .admin_username
                .or_else(|| optional_env("ADMIN_USERNAME"))
                .unwrap_or_else(|| "admin".into()),
            browse_lists,
            admin_password: cli
                .admin_password
                .or_else(|| optional_env("ADMIN_PASSWORD"))
                .unwrap_or_else(|| "admin123".into()),
        }
    }
}

fn configured_browse_lists(cli: &CliArgs) -> Vec<BrowseList> {
    let mut tokens = cli.browse_lists.clone();
    if let Some(raw) = optional_env("BROWSE_LISTS") {
        push_list_tokens(&mut tokens, &raw);
    }
    let mut lists = default_browse_lists();
    for token in tokens {
        let Some(list) = parse_browse_list(&token) else {
            continue;
        };
        if lists
            .iter()
            .any(|existing| existing.user == list.user && existing.slug == list.slug)
        {
            continue;
        }
        lists.push(list);
    }
    lists
}

fn push_list_tokens(out: &mut Vec<String>, raw: &str) {
    out.extend(
        raw.split(',')
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(str::to_string),
    );
}

fn default_browse_lists() -> Vec<BrowseList> {
    [
        "movie:snoak/imdb-top-250-movies",
        "tv:snoak/imdb-top-250-shows",
        "movie:snoak/most-popular-movies-on-rotten-tomatoes",
        "tv:snoak/most-popular-shows-on-rotten-tomatoes",
    ]
    .into_iter()
    .filter_map(parse_browse_list)
    .collect()
}

fn parse_browse_list(raw: &str) -> Option<BrowseList> {
    let (kind, rest) = raw.trim().split_once(':')?;
    if kind != "movie" && kind != "tv" {
        return None;
    }
    let (user, slug) = rest.split_once('/')?;
    let user = user.trim();
    let slug = slug.trim().trim_matches('/');
    if user.is_empty() || slug.is_empty() || slug.contains('/') {
        return None;
    }
    Some(BrowseList {
        user: user.to_string(),
        slug: slug.to_string(),
        media_type: kind.to_string(),
    })
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

#[derive(Debug)]
pub struct ListenAddrError {
    message: String,
}

impl ListenAddrError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ListenAddrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ListenAddrError {}

/// Addresses the process may bind.
///
/// `lan` is loopback plus private LAN only: 127.0.0.0/8, ::1, RFC1918, and
/// IPv6 ULA (fc00::/7). Any other host is bound as given, including public
/// addresses and wildcards (`0.0.0.0`, `::`).
pub fn resolve_listen_addrs(host: &str, port: u16) -> Result<Vec<SocketAddr>, ListenAddrError> {
    let host = host.trim();
    let ips = if host.eq_ignore_ascii_case("lan") {
        lan_ips()?
    } else {
        explicit_ips(host)?
    };
    if ips.is_empty() {
        return Err(ListenAddrError::new(format!(
            "no addresses resolved for listen host {host}"
        )));
    }
    Ok(ips
        .into_iter()
        .map(|ip| SocketAddr::new(ip, port))
        .collect())
}

fn lan_ips() -> Result<Vec<IpAddr>, ListenAddrError> {
    let ifaces = if_addrs::get_if_addrs()
        .map_err(|err| ListenAddrError::new(format!("failed to list network interfaces: {err}")))?;
    let mut ips = Vec::new();
    for iface in ifaces {
        let ip = iface.ip();
        if is_allowed_listen_ip(ip) {
            ips.push(ip);
        }
    }
    // Always include IPv4 loopback, even if interface enumeration missed `lo`.
    ips.push(IpAddr::V4(Ipv4Addr::LOCALHOST));
    Ok(normalize_ips(ips))
}

fn explicit_ips(host: &str) -> Result<Vec<IpAddr>, ListenAddrError> {
    let bare = host
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(host);
    if let Ok(ip) = bare.parse::<IpAddr>() {
        return Ok(vec![ip]);
    }
    let mut ips = Vec::new();
    let addrs = (host, 0u16)
        .to_socket_addrs()
        .map_err(|err| ListenAddrError::new(format!("invalid listen address {host}: {err}")))?;
    for addr in addrs {
        ips.push(addr.ip());
    }
    if ips.is_empty() {
        return Err(ListenAddrError::new(format!(
            "invalid listen address {host}"
        )));
    }
    Ok(normalize_ips(ips))
}

fn is_allowed_listen_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_allowed_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_allowed_v4(v4);
            }
            v6.is_loopback() || is_unique_local(v6)
        }
    }
}

fn is_allowed_v4(ip: Ipv4Addr) -> bool {
    ip.is_loopback() || ip.is_private()
}

/// fc00::/7 unique local addresses.
fn is_unique_local(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xfe00) == 0xfc00
}

fn normalize_ips(mut ips: Vec<IpAddr>) -> Vec<IpAddr> {
    ips.sort_by_key(|ip| ip_sort_key(*ip));
    ips.dedup();
    ips
}

fn ip_sort_key(ip: IpAddr) -> (u8, u8, u128) {
    let group = u8::from(!ip.is_loopback());
    match ip {
        IpAddr::V4(v4) => (group, 0, u128::from(u32::from(v4))),
        IpAddr::V6(v6) => (group, 1, u128::from(v6)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_wildcard_and_public_addresses() {
        let cases = [
            ("0.0.0.0", IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
            ("::", IpAddr::V6(Ipv6Addr::UNSPECIFIED)),
            ("[::]", IpAddr::V6(Ipv6Addr::UNSPECIFIED)),
            ("1.1.1.1", IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))),
            ("8.8.8.8", IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))),
            ("2001:db8::1", "2001:db8::1".parse().expect("doc prefix")),
        ];
        for (host, ip) in cases {
            let addrs = resolve_listen_addrs(host, 8080).unwrap_or_else(|err| {
                panic!("{host}: {err}");
            });
            assert_eq!(addrs, vec![SocketAddr::new(ip, 8080)], "{host}");
        }
    }

    #[test]
    fn allows_explicit_loopback_and_private_addresses() {
        let cases = [
            ("127.0.0.1", IpAddr::V4(Ipv4Addr::LOCALHOST)),
            ("10.1.2.3", IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3))),
            ("172.16.0.1", IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))),
            ("172.31.255.1", IpAddr::V4(Ipv4Addr::new(172, 31, 255, 1))),
            ("192.168.1.20", IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20))),
            ("::1", IpAddr::V6(Ipv6Addr::LOCALHOST)),
            ("[::1]", IpAddr::V6(Ipv6Addr::LOCALHOST)),
            ("fd00::1", "fd00::1".parse().expect("ula")),
        ];
        for (host, ip) in cases {
            let addrs = resolve_listen_addrs(host, 8080).unwrap_or_else(|err| {
                panic!("{host}: {err}");
            });
            assert_eq!(addrs, vec![SocketAddr::new(ip, 8080)], "{host}");
        }
    }

    #[test]
    fn lan_mode_is_only_loopback_and_private() {
        let addrs = resolve_listen_addrs("LAN", 8080).expect("lan");
        assert!(
            addrs
                .iter()
                .any(|addr| addr.ip() == IpAddr::V4(Ipv4Addr::LOCALHOST)),
            "missing 127.0.0.1: {addrs:?}"
        );
        assert!(addrs.iter().all(|addr| addr.port() == 8080));
        assert!(addrs.iter().all(|addr| is_allowed_listen_ip(addr.ip())));
        assert!(addrs.iter().all(|addr| !addr.ip().is_unspecified()));
        let mut sorted = addrs.clone();
        sorted.sort_by_key(|addr| ip_sort_key(addr.ip()));
        sorted.dedup();
        assert_eq!(addrs, sorted);
    }

    #[tokio::test]
    async fn lan_sockets_bind_their_own_addresses() {
        let probe = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("probe");
        let port = probe.local_addr().expect("probe addr").port();
        drop(probe);

        let addrs = resolve_listen_addrs("lan", port).expect("lan");
        let mut listeners = Vec::new();
        for addr in addrs {
            let listener = tokio::net::TcpListener::bind(addr)
                .await
                .unwrap_or_else(|err| panic!("bind {addr}: {err}"));
            assert_eq!(listener.local_addr().expect("local").ip(), addr.ip());
            listeners.push(listener);
        }

        let _stream = tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .expect("connect localhost");
        drop(listeners);
    }

    #[test]
    fn browse_lists_keep_defaults_and_append_cli_lists() {
        let cli = CliArgs::parse(
            [
                "medianexus-backend",
                "--list",
                "movie:alice/my-list",
                "--lists",
                "tv:bob/other,movie:snoak/imdb-top-250-movies,snoak/missing-type",
            ]
            .map(String::from),
        );
        let config = Config::from_cli_and_env(cli);
        let ids: Vec<_> = config
            .browse_lists
            .iter()
            .map(|list| format!("{}/{}", list.user, list.slug))
            .collect();
        assert!(ids.starts_with(&[
            "snoak/imdb-top-250-movies".into(),
            "snoak/imdb-top-250-shows".into(),
            "snoak/most-popular-movies-on-rotten-tomatoes".into(),
            "snoak/most-popular-shows-on-rotten-tomatoes".into(),
        ]));
        assert!(ids.iter().any(|id| id == "alice/my-list"));
        assert!(ids.iter().any(|id| id == "bob/other"));
        assert!(!ids.iter().any(|id| id.contains("missing-type")));
        assert_eq!(
            ids.iter()
                .filter(|id| id.as_str() == "snoak/imdb-top-250-movies")
                .count(),
            1
        );
        let shows = config
            .browse_lists
            .iter()
            .find(|list| list.slug == "imdb-top-250-shows")
            .unwrap();
        assert_eq!(shows.media_type(), "tv");
        assert_eq!(shows.label(), "IMDb Top 250");
    }

    #[test]
    fn localhost_name_stays_on_loopback() {
        let addrs = resolve_listen_addrs("localhost", 8080).expect("localhost");
        assert!(!addrs.is_empty());
        assert!(addrs.iter().all(|addr| addr.ip().is_loopback()));
    }
}
