import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { MediaCard } from "@/components/MediaCard";
import { Select } from "@/components/Select";
import { useToast } from "@/components/Toast";
import { ApiError, endpoints, type MediaCatalog, type MediaItem } from "@/lib/api";
import styles from "./BrowsePage.module.css";

type Tab = "movies" | "tv";

const FALLBACK_CATALOGS: MediaCatalog[] = [
  { id: "tmdb:popular", label: "TMDB 最热", media_type: "all" },
  { id: "tmdb:latest", label: "TMDB 最新", media_type: "all" },
  { id: "tmdb:top", label: "TMDB 高分", media_type: "all" },
  { id: "imdb:moviemeter", label: "IMDb 最热", media_type: "all" },
];

export function BrowsePage() {
  const [params, setParams] = useSearchParams();
  const tab: Tab = params.get("tab") === "tv" ? "tv" : "movies";
  const catalog = params.get("list") ?? "tmdb:popular";
  const page = Math.max(1, Number(params.get("page")) || 1);
  const queryText = params.get("q") ?? "";
  const [searchInput, setSearchInput] = useState(queryText);
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const { showToast } = useToast();

  const updateParams = (changes: Record<string, string | null>) => {
    setParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        for (const [key, value] of Object.entries(changes)) {
          if (value) next.set(key, value);
          else next.delete(key);
        }
        return next;
      },
      { replace: true },
    );
  };

  const searching = queryText.trim().length > 0;
  const searchType = tab === "movies" ? "movie" : "tv";
  const catalogs = useQuery({
    queryKey: ["media-catalogs"],
    queryFn: endpoints.catalogs,
  });
  const catalogOptions = (catalogs.data ?? FALLBACK_CATALOGS).filter(
    (item) => item.media_type === "all" || item.media_type === searchType,
  );
  const activeCatalog = catalogOptions.some((item) => item.id === catalog)
    ? catalog
    : "tmdb:popular";

  const query = useQuery({
    queryKey: searching
      ? ["media-search", searchType, queryText, page]
      : ["media", searchType, activeCatalog, page],
    queryFn: () =>
      searching
        ? endpoints.searchMedia(queryText.trim(), page, searchType)
        : endpoints.browse(searchType, page, activeCatalog),
  });

  const trackMutation = useMutation({
    mutationFn: (item: MediaItem) =>
      endpoints.createTrack(item.media_type, item.tmdb_id),
    onSuccess: (track) => {
      if (track.status === "already_exists") {
        showToast(
          track.status_message || `「${track.title}」媒体库中已有，跳过下载`,
        );
      } else {
        showToast(`已开始追踪「${track.title}」，自动化流水线运行中`);
      }
      queryClient.invalidateQueries({ queryKey: ["tracks"] });
    },
    onError: (err: unknown) => {
      showToast(err instanceof ApiError ? err.message : "追踪失败", "error");
    },
  });

  const items = query.data ?? [];

  const submitSearch = (e: FormEvent) => {
    e.preventDefault();
    updateParams({ q: searchInput.trim(), page: null });
  };

  const clearSearch = () => {
    setSearchInput("");
    updateParams({ q: null, page: null });
  };

  return (
    <div className={styles.page}>
      <form className={styles.search} onSubmit={submitSearch}>
        <input
          value={searchInput}
          onChange={(e) => setSearchInput(e.target.value)}
          placeholder={tab === "movies" ? "搜索电影…" : "搜索电视剧…"}
          aria-label="搜索影视"
        />
        <button type="submit" className="btn">
          搜索
        </button>
        {searching && (
          <button type="button" className="btn ghost" onClick={clearSearch}>
            清除
          </button>
        )}
      </form>

      <div className={styles.toolbar}>
        <div className={styles.tabs}>
          <button
            type="button"
            className={tab === "movies" ? styles.active : undefined}
            onClick={() => updateParams({ tab: null, page: null })}
          >
            电影
          </button>
          <button
            type="button"
            className={tab === "tv" ? styles.active : undefined}
            onClick={() => updateParams({ tab: "tv", page: null })}
          >
            电视剧
          </button>
        </div>
        <div className={styles.tools}>
          {!searching && (
            <div className={styles.catalog}>
              <Select
                aria-label="列表"
                value={activeCatalog}
                options={catalogOptions.map((item) => ({
                  value: item.id,
                  label: item.label,
                }))}
                onChange={(value) => updateParams({ list: value, page: null })}
              />
            </div>
          )}
        </div>
      </div>

      {searching && (
        <p className="notice">
          搜索「{queryText}」· {tab === "movies" ? "电影" : "电视剧"}
        </p>
      )}

      {query.isLoading && <p className="notice">加载中…</p>}
      {query.isError && (
        <p className={styles.error}>
          {(query.error as Error).message || "加载失败，请检查 TMDB 配置"}
        </p>
      )}
      {!query.isLoading && !query.isError && items.length === 0 && (
        <p className="notice">{searching ? "没有找到相关结果" : "暂无内容"}</p>
      )}

      <div className={styles.grid}>
        {items.map((item) => (
          <MediaCard
            key={`${item.media_type}-${item.tmdb_id}`}
            item={item}
            actionLabel={item.media_type === "tv" ? "按季" : undefined}
            onTrack={(it) => {
              if (it.media_type === "tv") {
                navigate(`/media/tv/${it.tmdb_id}`);
                return;
              }
              trackMutation.mutate(it);
            }}
            tracking={
              trackMutation.isPending &&
              trackMutation.variables?.tmdb_id === item.tmdb_id &&
              trackMutation.variables?.media_type === item.media_type
            }
          />
        ))}
      </div>

      <div className={styles.pager}>
        <button
          type="button"
          className="btn ghost"
          disabled={page <= 1}
          onClick={() => updateParams({ page: String(page - 1) })}
        >
          上一页
        </button>
        <span className="notice">第 {page} 页</span>
        <button
          type="button"
          className="btn ghost"
          disabled={items.length === 0}
          onClick={() => updateParams({ page: String(page + 1) })}
        >
          下一页
        </button>
      </div>
    </div>
  );
}
