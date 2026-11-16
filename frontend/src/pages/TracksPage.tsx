import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { useToast } from "@/components/Toast";
import { ApiError, endpoints, type Track } from "@/lib/api";
import styles from "./TracksPage.module.css";

function statusBadge(status: string) {
  if (status === "completed" || status === "already_exists") return "ok";
  if (status === "failed") return "danger";
  if (status === "downloading" || status === "searching") return "warn";
  return "info";
}

function statusLabel(status: string) {
  const map: Record<string, string> = {
    pending: "排队中",
    searching: "搜种中",
    torrent_found: "已找到种子",
    downloading: "下载中",
    hardlinking: "硬链接中",
    scanning: "扫描媒体库",
    completed: "已完成",
    already_exists: "库中已有",
    failed: "失败",
  };
  return map[status] ?? status;
}

function canTrigger(status: string) {
  return !["completed", "already_exists"].includes(status);
}

export function TracksPage() {
  const queryClient = useQueryClient();
  const { showToast } = useToast();
  const query = useQuery({
    queryKey: ["tracks"],
    queryFn: endpoints.tracks,
    refetchInterval: 4000,
  });

  const cancelMutation = useMutation({
    mutationFn: (id: string) => endpoints.cancelTrack(id),
    onSuccess: () => {
      showToast("已取消追踪");
      queryClient.invalidateQueries({ queryKey: ["tracks"] });
    },
    onError: (err: unknown) => {
      showToast(err instanceof ApiError ? err.message : "取消失败", "error");
    },
  });

  const triggerMutation = useMutation({
    mutationFn: (id: string) => endpoints.triggerTrack(id),
    onSuccess: () => {
      showToast("已手动触发");
      queryClient.invalidateQueries({ queryKey: ["tracks"] });
    },
    onError: (err: unknown) => {
      showToast(err instanceof ApiError ? err.message : "触发失败", "error");
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => endpoints.deleteTrack(id),
    onSuccess: () => {
      showToast("已删除追踪");
      queryClient.invalidateQueries({ queryKey: ["tracks"] });
    },
    onError: (err: unknown) => {
      showToast(err instanceof ApiError ? err.message : "删除失败", "error");
    },
  });

  const tracks = query.data ?? [];
  const busyId =
    (cancelMutation.isPending && cancelMutation.variables) ||
    (triggerMutation.isPending && triggerMutation.variables) ||
    (deleteMutation.isPending && deleteMutation.variables) ||
    null;

  return (
    <div className={styles.page}>
      {query.isLoading && <p className={styles.muted}>加载中…</p>}
      {!query.isLoading && tracks.length === 0 && (
        <p className={styles.muted}>还没有追踪。去「发现」页点一下追踪即可。</p>
      )}

      <div className={styles.list}>
        {tracks.map((track) => (
          <TrackRow
            key={track.id}
            track={track}
            onCancel={() => cancelMutation.mutate(track.id)}
            onTrigger={() => triggerMutation.mutate(track.id)}
            onDelete={() => deleteMutation.mutate(track.id)}
            busy={busyId === track.id}
            triggering={
              triggerMutation.isPending && triggerMutation.variables === track.id
            }
          />
        ))}
      </div>
    </div>
  );
}

function TrackRow({
  track,
  onCancel,
  onTrigger,
  onDelete,
  busy,
  triggering,
}: {
  track: Track;
  onCancel: () => void;
  onTrigger: () => void;
  onDelete: () => void;
  busy: boolean;
  triggering: boolean;
}) {
  const pct = Math.round((track.progress || 0) * 100);
  const active = !["completed", "already_exists", "failed"].includes(track.status);
  const showTrigger = canTrigger(track.status);

  const detailTo = `/media/${track.media_type}/${track.tmdb_id}`;

  return (
    <article className={styles.row}>
      <Link to={detailTo} className={styles.poster}>
        {track.poster_path ? (
          <img src={track.poster_path} alt={track.title} />
        ) : (
          <div className={styles.ph} />
        )}
      </Link>
      <div className={styles.body}>
        <div className={styles.titleRow}>
          <h3>
            <Link to={detailTo}>{track.title}</Link>
          </h3>
          <span className={`badge ${statusBadge(track.status)}`}>
            {statusLabel(track.status)}
          </span>
        </div>
        <p className={styles.meta}>
          {track.media_type === "tv" ? "电视剧" : "电影"}
          {track.year ? ` · ${track.year}` : ""}
          {track.torrent_site ? ` · ${track.torrent_site}` : ""}
          {track.torrent_name ? ` · ${track.torrent_name}` : ""}
        </p>
        <p className={styles.status}>{track.status_message || "—"}</p>
        {track.error_message && (
          <p className={styles.error}>{track.error_message}</p>
        )}
        {active && (
          <div className={styles.progress}>
            <div style={{ width: `${pct}%` }} />
          </div>
        )}
        {track.hardlink_path && (
          <p className={styles.path}>入库路径：{track.hardlink_path}</p>
        )}
        <div className={styles.actions}>
          {showTrigger && (
            <button
              type="button"
              className="btn"
              disabled={busy}
              onClick={onTrigger}
            >
              {triggering ? "执行中…" : "立即执行"}
            </button>
          )}
          {active && (
            <button type="button" className="btn ghost" disabled={busy} onClick={onCancel}>
              取消
            </button>
          )}
          <button type="button" className="btn danger" disabled={busy} onClick={onDelete}>
            删除
          </button>
        </div>
      </div>
    </article>
  );
}
