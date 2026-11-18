import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Navigate } from "react-router-dom";
import { useToast } from "@/components/Toast";
import { ApiError, endpoints, type LibraryItemRow, type MaintenanceRunResult } from "@/lib/api";
import { getUser, isAdmin } from "@/lib/auth";
import styles from "./MaintenancePage.module.css";

export function MaintenancePage() {
  const user = getUser();
  const allowed = isAdmin(user);
  const queryClient = useQueryClient();
  const { showToast } = useToast();
  const [lastResult, setLastResult] = useState<MaintenanceRunResult | null>(null);
  const [mediaType, setMediaType] = useState<"all" | "movie" | "tv">("all");
  const [source, setSource] = useState<"all" | "plex" | "jellyfin">("all");

  const toolsQuery = useQuery({
    queryKey: ["maintenance-tools"],
    queryFn: endpoints.maintenanceTools,
    enabled: allowed,
  });

  const itemsQuery = useQuery({
    queryKey: ["library-items", mediaType, source],
    queryFn: () =>
      endpoints.libraryItems({
        media_type: mediaType === "all" ? undefined : mediaType,
        source: source === "all" ? undefined : source,
        limit: 100,
        offset: 0,
      }),
    enabled: allowed,
  });

  const runMutation = useMutation({
    mutationFn: (tool: string) => endpoints.runMaintenance(tool),
    onSuccess: (res) => {
      setLastResult(res);
      showToast(res.message, res.ok ? "ok" : "error");
      queryClient.invalidateQueries({ queryKey: ["library-items"] });
      queryClient.invalidateQueries({ queryKey: ["tracks"] });
    },
    onError: (err: unknown) => {
      setLastResult(null);
      showToast(err instanceof ApiError ? err.message : "执行失败", "error");
    },
  });

  if (!allowed) {
    return <Navigate to="/" replace />;
  }

  return (
    <div className={styles.page}>
      <p className="notice">
        先在「发现」添加追踪；后台会按设置自动搜种、检查下载并入库。这里可手动触发维护任务。
      </p>

      <div className={styles.tools}>
        {(toolsQuery.data ?? []).map((tool) => (
          <article key={tool.id} className={styles.card}>
            <h3>{tool.name}</h3>
            <p>{tool.description}</p>
            <button
              type="button"
              className="btn"
              disabled={runMutation.isPending}
              onClick={() => runMutation.mutate(tool.id)}
            >
              {runMutation.isPending && runMutation.variables === tool.id
                ? "执行中…"
                : "运行"}
            </button>
          </article>
        ))}
      </div>

      {lastResult && lastResult.details.length > 0 && (
        <div className={styles.result}>
          <ul>
            {lastResult.details.map((d) => (
              <li key={d}>{d}</li>
            ))}
          </ul>
          {lastResult.count != null && <p>数量：{lastResult.count}</p>}
        </div>
      )}

      <div className={styles.catalog}>
        <div className={styles.catalogHead}>
          <h3>已同步影视库</h3>
          <div className={styles.filters}>
            <div className={styles.tabs}>
              {(["all", "movie", "tv"] as const).map((v) => (
                <button
                  key={v}
                  type="button"
                  className={mediaType === v ? styles.active : undefined}
                  onClick={() => setMediaType(v)}
                >
                  {v === "all" ? "全部" : v === "movie" ? "电影" : "电视剧"}
                </button>
              ))}
            </div>
            <div className={styles.tabs}>
              {(["all", "plex", "jellyfin"] as const).map((v) => (
                <button
                  key={v}
                  type="button"
                  className={source === v ? styles.active : undefined}
                  onClick={() => setSource(v)}
                >
                  {v === "all" ? "全部源" : v}
                </button>
              ))}
            </div>
          </div>
        </div>

        {itemsQuery.isLoading && <p className="notice">加载中…</p>}
        {!itemsQuery.isLoading && (itemsQuery.data?.length ?? 0) === 0 && (
          <p className="notice">暂无数据，请先运行「同步影视库目录」。</p>
        )}

        <div className={styles.table}>
          {(itemsQuery.data ?? []).map((item: LibraryItemRow) => (
            <div className={styles.row} key={item.id}>
              <span>{item.title}</span>
              <span className="badge">
                {item.media_type === "tv" ? "电视剧" : "电影"}
              </span>
              <span className="badge info">{item.source}</span>
              <span className={styles.muted}>
                {item.year || "—"} · TMDB {item.tmdb_id ?? "—"} · {item.imdb_id || "—"}
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
