import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useToast } from "@/components/Toast";
import { ApiError, endpoints } from "@/lib/api";
import styles from "./MediaDetailPage.module.css";

export function MediaDetailPage() {
  const { mediaType = "", tmdbId = "" } = useParams();
  const id = Number(tmdbId);
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { showToast } = useToast();

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

  const trackMutation = useMutation({
    mutationFn: () => endpoints.createTrack(mediaType, id),
    onSuccess: (track) => {
      if (track.status === "already_exists") {
        showToast(track.status_message || "媒体库中已有，无需下载");
      } else {
        showToast(`已开始追踪「${track.title}」`);
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
  const alreadyTracked = !!existingTrackQuery.data;

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
            {item.vote_average != null && (
              <span className="badge">{item.vote_average.toFixed(1)}</span>
            )}
            {item.imdb_id && <span className="badge">{item.imdb_id}</span>}
            {inLibrary && <span className="badge ok">库中已有</span>}
          </div>

          <h1 className={styles.title}>{item.title}</h1>
          {item.original_title && item.original_title !== item.title && (
            <p className={styles.original}>{item.original_title}</p>
          )}

          {inLibrary && (
            <div className={styles.libraryBox}>
              <strong>媒体库中已存在，不会重新下载</strong>
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

          <div className={styles.actions}>
            {!alreadyTracked && !inLibrary && (
              <button
                type="button"
                className="btn"
                disabled={trackMutation.isPending}
                onClick={() => trackMutation.mutate()}
              >
                {trackMutation.isPending ? "追踪中…" : "追踪"}
              </button>
            )}
            <Link to="/tracks" className="btn ghost">
              {alreadyTracked ? "查看追踪" : "追踪列表"}
            </Link>
          </div>
        </div>
      </article>
    </div>
  );
}
