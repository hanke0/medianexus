import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { RatingBadges } from "@/components/RatingBadges";
import { useToast } from "@/components/Toast";
import { ApiError, endpoints, type Track, type TvSeason } from "@/lib/api";
import styles from "./MediaDetailPage.module.css";

function seasonLabel(season: number, episode?: number | null) {
  const s = `S${String(season).padStart(2, "0")}`;
  if (episode == null) return s;
  return `${s}E${String(episode).padStart(2, "0")}`;
}

function findTrack(tracks: Track[], season: number, episode?: number | null) {
  return tracks.find(
    (t) =>
      t.season_number === season &&
      (episode == null ? t.episode_number == null : t.episode_number === episode),
  );
}

export function MediaDetailPage() {
  const { mediaType = "", tmdbId = "" } = useParams();
  const id = Number(tmdbId);
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { showToast } = useToast();
  const [openSeason, setOpenSeason] = useState<number | null>(null);
  const [bulkPending, setBulkPending] = useState(false);

  const valid =
    (mediaType === "movie" || mediaType === "tv") && Number.isFinite(id) && id > 0;

  const query = useQuery({
    queryKey: ["media-detail", mediaType, id],
    queryFn: () => endpoints.mediaDetail(mediaType, id),
    enabled: valid,
  });

  const libraryQuery = useQuery({
    queryKey: ["media-library", mediaType, id],
    queryFn: () => endpoints.mediaLibrary(mediaType, id),
    enabled: valid,
    staleTime: 60_000,
  });

  const existingTrackQuery = useQuery({
    queryKey: ["track-by-media", mediaType, id],
    queryFn: () => endpoints.trackByMedia(mediaType, id),
    enabled: valid,
  });

  const episodesQuery = useQuery({
    queryKey: ["tv-season", id, openSeason],
    queryFn: () => endpoints.tvSeason(id, openSeason as number),
    enabled: mediaType === "tv" && openSeason != null,
  });

  const trackMutation = useMutation({
    mutationFn: (target?: { season_number?: number; episode_number?: number }) =>
      endpoints.createTrack(mediaType, id, target),
    onSuccess: (track) => {
      if (track.status === "already_exists") {
        showToast(track.status_message || "媒体库中已有，无需下载");
      } else {
        const label =
          track.season_number != null
            ? seasonLabel(track.season_number, track.episode_number)
            : track.title;
        showToast(`已开始追踪「${track.title}」${track.season_number != null ? ` ${label}` : ""}`);
      }
      queryClient.invalidateQueries({ queryKey: ["tracks"] });
      queryClient.invalidateQueries({ queryKey: ["track-by-media", mediaType, id] });
      queryClient.invalidateQueries({ queryKey: ["media-library", mediaType, id] });
    },
    onError: (err: unknown) => {
      showToast(err instanceof ApiError ? err.message : "追踪失败", "error");
    },
  });

  if (!valid) {
    return <p className={styles.error}>无效的影视链接</p>;
  }

  if (query.isLoading) {
    return <p className={styles.muted}>加载中…</p>;
  }

  if (query.isError || !query.data) {
    return (
      <div className={styles.page}>
        <p className={styles.error}>
          {(query.error as Error)?.message || "加载详情失败"}
        </p>
        <button type="button" className="btn ghost" onClick={() => navigate(-1)}>
          返回
        </button>
      </div>
    );
  }

  const item = query.data;
  const backdrop = item.poster_url;
  const inLibrary = libraryQuery.data?.found === true;
  const libraryMatches = libraryQuery.data?.matches ?? [];
  const tracks = existingTrackQuery.data ?? [];
  const alreadyTracked = mediaType === "movie" && tracks.length > 0;
  const seasons = item.seasons ?? [];

  return (
    <div className={styles.page}>
      <button type="button" className={`btn ghost ${styles.back}`} onClick={() => navigate(-1)}>
        返回
      </button>

      <article className={styles.layout}>
        <div className={styles.poster}>
          {backdrop ? (
            <img src={backdrop} alt={item.title} />
          ) : (
            <div className={styles.placeholder}>无封面</div>
          )}
        </div>

        <div className={styles.body}>
          <div className={styles.badges}>
            <span className="badge info">
              {item.media_type === "tv" ? "电视剧" : "电影"}
            </span>
            {item.year && <span className="badge">{item.year}</span>}
            <RatingBadges item={item} />
            {item.imdb_id && <span className="badge">{item.imdb_id}</span>}
            {inLibrary && <span className="badge ok">库中已有</span>}
          </div>

          <h1 className={styles.title}>{item.title}</h1>
          {item.original_title && item.original_title !== item.title && (
            <p className={styles.original}>{item.original_title}</p>
          )}

          {inLibrary && (
            <div className={styles.libraryBox}>
              <strong>
                {mediaType === "tv"
                  ? "媒体库里已有这部剧，仍可以按季补缺失的内容"
                  : "媒体库中已存在，不会重新下载"}
              </strong>
              <ul>
                {libraryMatches.map((m, i) => (
                  <li key={`${m.source}-${m.matched_by}-${i}`}>
                    {m.source.toUpperCase()} · {m.title} · 匹配{" "}
                    {m.matched_by.toUpperCase()}
                    {m.external_id ? ` (${m.external_id})` : ""}
                  </li>
                ))}
              </ul>
            </div>
          )}

          <p className={styles.overview}>
            {item.overview?.trim() || "暂无简介"}
          </p>

          {mediaType === "movie" && (
            <div className={styles.actions}>
              {!alreadyTracked && !inLibrary && (
                <button
                  type="button"
                  className="btn"
                  disabled={trackMutation.isPending}
                  onClick={() => trackMutation.mutate(undefined)}
                >
                  {trackMutation.isPending ? "追踪中…" : "追踪"}
                </button>
              )}
              <Link to="/tracks" className="btn ghost">
                {alreadyTracked ? "查看追踪" : "追踪列表"}
              </Link>
            </div>
          )}

          {mediaType === "tv" && (
            <TvSeasons
              seasons={seasons}
              tracks={tracks}
              openSeason={openSeason}
              episodes={episodesQuery.data ?? []}
              episodesLoading={episodesQuery.isLoading}
              pendingTarget={trackMutation.isPending ? trackMutation.variables : undefined}
              bulkPending={bulkPending}
              onToggle={(season) =>
                setOpenSeason((cur) => (cur === season ? null : season))
              }
              onTrackSeason={(season) => trackMutation.mutate({ season_number: season })}
              onTrackEpisode={(season, episode) =>
                trackMutation.mutate({ season_number: season, episode_number: episode })
              }
              onTrackAll={async () => {
                setBulkPending(true);
                try {
                  let n = 0;
                  for (const season of seasons) {
                    if (season.season_number === 0) continue;
                    if (findTrack(tracks, season.season_number)) continue;
                    await endpoints.createTrack("tv", id, {
                      season_number: season.season_number,
                    });
                    n += 1;
                  }
                  showToast(n > 0 ? `已按季追踪 ${n} 季` : "这些季已在追踪中");
                  queryClient.invalidateQueries({ queryKey: ["tracks"] });
                  queryClient.invalidateQueries({
                    queryKey: ["track-by-media", mediaType, id],
                  });
                } catch (err: unknown) {
                  showToast(err instanceof ApiError ? err.message : "追踪失败", "error");
                } finally {
                  setBulkPending(false);
                }
              }}
            />
          )}
        </div>
      </article>
    </div>
  );
}

function TvSeasons({
  seasons,
  tracks,
  openSeason,
  episodes,
  episodesLoading,
  pendingTarget,
  bulkPending,
  onToggle,
  onTrackSeason,
  onTrackEpisode,
  onTrackAll,
}: {
  seasons: TvSeason[];
  tracks: Track[];
  openSeason: number | null;
  episodes: { episode_number: number; name: string }[];
  episodesLoading: boolean;
  pendingTarget?: { season_number?: number; episode_number?: number };
  bulkPending: boolean;
  onToggle: (season: number) => void;
  onTrackSeason: (season: number) => void;
  onTrackEpisode: (season: number, episode: number) => void;
  onTrackAll: () => void;
}) {
  if (seasons.length === 0) {
    return <p className={styles.muted}>没有读到分季信息。</p>;
  }
  return (
    <section className={styles.seasons}>
      <div className={styles.seasonTools}>
        <p className={styles.muted}>
          按季追踪。有全集优先下载全集，否则下载已有的单集。文件放在
          剧名/剧名-S01/ 下，文件名保持下载时的原名。
        </p>
        <button type="button" className="btn ghost" disabled={bulkPending} onClick={onTrackAll}>
          {bulkPending ? "追踪中…" : "追踪全部季"}
        </button>
      </div>
      {seasons.map((season) => {
        const tracked = findTrack(tracks, season.season_number);
        const pending =
          pendingTarget?.season_number === season.season_number &&
          pendingTarget.episode_number == null;
        const open = openSeason === season.season_number;
        return (
          <article key={season.season_number} className={styles.season}>
            <div className={styles.seasonHead}>
              <div>
                <strong>
                  {season.season_number === 0
                    ? "特别篇"
                    : seasonLabel(season.season_number)}
                </strong>
                <span className={styles.muted}>
                  {" "}
                  · {season.name}
                  {season.episode_count > 0 ? ` · ${season.episode_count} 集` : ""}
                </span>
                {tracked && (
                  <span className={`badge ${tracked.status === "failed" ? "danger" : "info"}`}>
                    {" "}
                    {tracked.status_message || tracked.status}
                  </span>
                )}
              </div>
              <div className={styles.actions}>
                {!tracked && (
                  <button
                    type="button"
                    className="btn"
                    disabled={pending || bulkPending}
                    onClick={() => onTrackSeason(season.season_number)}
                  >
                    {pending ? "追踪中…" : "追踪本季"}
                  </button>
                )}
                <button
                  type="button"
                  className="btn ghost"
                  onClick={() => onToggle(season.season_number)}
                >
                  {open ? "收起单集" : "按集追踪"}
                </button>
              </div>
            </div>
            {open && (
              <div className={styles.episodes}>
                {episodesLoading && <p className={styles.muted}>加载分集…</p>}
                {!episodesLoading &&
                  episodes.map((ep) => {
                    const row = findTrack(tracks, season.season_number, ep.episode_number);
                    const epPending =
                      pendingTarget?.season_number === season.season_number &&
                      pendingTarget.episode_number === ep.episode_number;
                    return (
                      <div key={ep.episode_number} className={styles.episode}>
                        <span>
                          {seasonLabel(season.season_number, ep.episode_number)} · {ep.name}
                          {row ? ` · ${row.status_message || row.status}` : ""}
                        </span>
                        {!row && (
                          <button
                            type="button"
                            className="btn ghost"
                            disabled={epPending}
                            onClick={() =>
                              onTrackEpisode(season.season_number, ep.episode_number)
                            }
                          >
                            {epPending ? "追踪中…" : "追踪"}
                          </button>
                        )}
                      </div>
                    );
                  })}
              </div>
            )}
          </article>
        );
      })}
      <Link to="/tracks" className="btn ghost">
        追踪列表
      </Link>
    </section>
  );
}
