import { clearAuth, getToken } from "@/lib/auth";

export class ApiError extends Error {
  code: string;
  status: number;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.status = status;
    this.code = code;
  }
}

async function parseError(res: Response): Promise<ApiError> {
  try {
    const body = (await res.json()) as { code?: string; message?: string };
    return new ApiError(
      res.status,
      body.code ?? "error",
      body.message ?? res.statusText,
    );
  } catch {
    return new ApiError(res.status, "error", res.statusText);
  }
}

export async function api<T>(
  path: string,
  init: RequestInit = {},
): Promise<T> {
  const headers = new Headers(init.headers);
  if (!headers.has("Content-Type") && init.body) {
    headers.set("Content-Type", "application/json");
  }
  const token = getToken();
  if (token) {
    headers.set("Authorization", `Bearer ${token}`);
  }

  const res = await fetch(`/api${path}`, { ...init, headers });
  if (res.status === 401) {
    // Session invalid / expired — drop local auth. Login form 401 stays on /login.
    clearAuth();
    const onLoginPage = window.location.pathname.startsWith("/login");
    const isLoginRequest = path === "/auth/login";
    if (!onLoginPage && !isLoginRequest) {
      window.location.replace("/login");
    }
    throw await parseError(res);
  }
  if (!res.ok) {
    throw await parseError(res);
  }
  if (res.status === 204) {
    return undefined as T;
  }
  return (await res.json()) as T;
}

export type MediaCatalog = {
  id: string;
  label: string;
  media_type: "movie" | "tv" | "all";
};

export type MediaItem = {
  media_type: string;
  tmdb_id: number;
  title: string;
  original_title?: string | null;
  year?: string | null;
  poster_url?: string | null;
  overview?: string | null;
  imdb_id?: string | null;
  /** IMDb score, 0–10. */
  imdb_rating?: number | null;
  /** Rotten Tomatoes critics score, 0–100. */
  rt_rating?: number | null;
  popularity?: number | null;
  vote_average?: number | null;
  seasons?: TvSeason[];
};

export type TvSeason = {
  season_number: number;
  name: string;
  episode_count: number;
  poster_url?: string | null;
};

export type TvEpisode = {
  episode_number: number;
  name: string;
  overview?: string | null;
  still_url?: string | null;
};

export type LibraryMatch = {
  source: string;
  title: string;
  matched_by: string;
  external_id?: string | null;
};

export type LibraryPresence = {
  found: boolean;
  matches: LibraryMatch[];
};

export type MaintenanceToolDto = {
  id: string;
  name: string;
  description: string;
};

export type MaintenanceRunResult = {
  tool: string;
  ok: boolean;
  message: string;
  details: string[];
  count?: number | null;
};

export type MaintenanceJob = {
  tool: string;
  status: "running" | "succeeded" | "failed";
  started_at: string;
  finished_at?: string | null;
  result?: MaintenanceRunResult | null;
  error?: string | null;
};

export type LibraryItemRow = {
  id: string;
  source: string;
  media_type: string;
  external_id: string;
  title: string;
  year?: string | null;
  tmdb_id?: number | null;
  imdb_id?: string | null;
  synced_at: string;
};

export type Track = {
  id: string;
  user_id: string;
  media_type: string;
  tmdb_id: number;
  season_number?: number | null;
  episode_number?: number | null;
  title: string;
  original_title?: string | null;
  year?: string | null;
  poster_path?: string | null;
  overview?: string | null;
  imdb_id?: string | null;
  status: string;
  progress: number;
  torrent_site?: string | null;
  torrent_id?: string | null;
  torrent_name?: string | null;
  torrent_hash?: string | null;
  torrent_size?: string | null;
  download_path?: string | null;
  hardlink_path?: string | null;
  error_message?: string | null;
  status_message?: string | null;
  created_at: string;
  updated_at: string;
};

export type UserDto = {
  id: string;
  username: string;
  role: string;
  created_at: string;
};

export type PlexLinkStart = {
  auth_url: string;
  pin_code: string;
  pin_id: number;
};

export type PlexLinkStatus = {
  linked: boolean;
  pending: boolean;
  pin_code?: string | null;
  auth_url?: string | null;
  expires_at?: number | null;
};

export type IntegrationSettings = {
  tmdb_api_key: string;
  /** TMDB language, e.g. zh-CN / en-US. Default zh-CN. */
  tmdb_language: string;
  /** MDBList key for IMDb and Rotten Tomatoes scores. Empty hides those scores. */
  mdblist_api_key: string;
  /** Hardlink folder template, e.g. {{title}}-{{year}}. */
  hardlink_folder_template: string;
  mteam_api_key: string;
  mteam_api_base: string;
  qb_url: string;
  qb_username: string;
  qb_password: string;
  qb_category: string;
  qb_save_path: string;
  plex_url: string;
  /** True when Plex JWT link is active (token redacted). */
  plex_linked: boolean;
  plex_token_expires_at?: string;
  jellyfin_url: string;
  jellyfin_api_key: string;
  /** Preferred PT site id (`mteam`, …). Empty = search all configured. */
  pt_primary_site: string;
  /** PT search interval in seconds (default 10800 = 3h). Min 60. Aligned from local midnight. */
  schedule_search_interval_secs: number;
  /** Deprecated; ignored by server. */
  schedule_search_at?: string;
  /** Download status poll interval in seconds (default 60). Min 60. */
  schedule_download_interval_secs: number;
  schedule_download_at?: string;
  /** Library scan + catalog sync interval in seconds (default 86400). Min 60. */
  schedule_library_scan_interval_secs: number;
  schedule_library_scan_at?: string;
  /** Local `HH:MM` when home-page lists are refreshed (default `00:01`). Empty = off. */
  schedule_browse_warm_at: string;
};

export type ConnectionTarget = "mteam" | "qbittorrent" | "jellyfin" | "plex";

export type ConnectionTestResult = {
  ok: boolean;
  message: string;
};

export const endpoints = {
  login: (username: string, password: string) =>
    api<{ token: string; user: { id: string; username: string; role: string } }>(
      "/auth/login",
      { method: "POST", body: JSON.stringify({ username, password }) },
    ),
  me: () => api<{ id: string; username: string; role: string }>("/auth/me"),
  catalogs: () => api<MediaCatalog[]>("/media/catalogs"),
  browse: (mediaType: "movie" | "tv", page: number, catalog: string) =>
    api<MediaItem[]>(
      `/media/${mediaType === "movie" ? "movies" : "tv"}?page=${page}&catalog=${encodeURIComponent(catalog)}`,
    ),
  searchMedia: (q: string, page = 1, media_type: "movie" | "tv" | "all" = "all") =>
    api<MediaItem[]>(
      `/media/search?q=${encodeURIComponent(q)}&page=${page}&media_type=${media_type}`,
    ),
  mediaDetail: (media_type: string, tmdb_id: number) =>
    api<MediaItem>(`/media/${media_type}/${tmdb_id}`),
  tvSeason: (tmdb_id: number, season_number: number) =>
    api<TvEpisode[]>(`/media/tv/${tmdb_id}/seasons/${season_number}`),
  mediaLibrary: (media_type: string, tmdb_id: number) =>
    api<LibraryPresence>(`/media/${media_type}/${tmdb_id}/library`),
  tracks: () => api<Track[]>("/tracks"),
  createTrack: (
    media_type: string,
    tmdb_id: number,
    target?: { season_number?: number; episode_number?: number },
  ) =>
    api<Track>("/tracks", {
      method: "POST",
      body: JSON.stringify({ media_type, tmdb_id, ...target }),
    }),
  cancelTrack: (id: string) =>
    api<Track>(`/tracks/${id}/cancel`, { method: "POST" }),
  triggerTrack: (id: string) =>
    api<Track>(`/tracks/${id}/trigger`, { method: "POST" }),
  trackByMedia: (media_type: string, tmdb_id: number) =>
    api<Track[]>(`/tracks/by-media/${media_type}/${tmdb_id}`),
  deleteTrack: (id: string) =>
    api<void>(`/tracks/${id}`, { method: "DELETE" }),
  users: () => api<UserDto[]>("/users"),
  createUser: (username: string, password: string, role: string) =>
    api<UserDto>("/users", {
      method: "POST",
      body: JSON.stringify({ username, password, role }),
    }),
  deleteUser: (id: string) =>
    api<void>(`/users/${id}`, { method: "DELETE" }),
  settings: () => api<IntegrationSettings>("/settings"),
  updateSettings: (data: IntegrationSettings) =>
    api<IntegrationSettings>("/settings", {
      method: "PUT",
      body: JSON.stringify(data),
    }),
  plexLinkStart: (forward_url: string) =>
    api<PlexLinkStart>("/settings/plex/link/start", {
      method: "POST",
      body: JSON.stringify({ forward_url }),
    }),
  plexLinkStatus: () => api<PlexLinkStatus>("/settings/plex/link/status"),
  plexUnlink: () =>
    api<PlexLinkStatus>("/settings/plex/link", { method: "DELETE" }),
  testConnection: (target: ConnectionTarget) =>
    api<ConnectionTestResult>(`/settings/test/${target}`, { method: "POST" }),
  maintenanceTools: () => api<MaintenanceToolDto[]>("/maintenance/tools"),
  runMaintenance: (tool: string) =>
    api<MaintenanceJob>("/maintenance/run", {
      method: "POST",
      body: JSON.stringify({ tool }),
    }),
  maintenanceJobs: () => api<MaintenanceJob[]>("/maintenance/jobs"),
  libraryItems: (params?: {
    media_type?: string;
    source?: string;
    limit?: number;
    offset?: number;
  }) => {
    const q = new URLSearchParams();
    if (params?.media_type) q.set("media_type", params.media_type);
    if (params?.source) q.set("source", params.source);
    if (params?.limit != null) q.set("limit", String(params.limit));
    if (params?.offset != null) q.set("offset", String(params.offset));
    const qs = q.toString();
    return api<LibraryItemRow[]>(
      `/maintenance/library-items${qs ? `?${qs}` : ""}`,
    );
  },
};
