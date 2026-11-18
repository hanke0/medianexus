import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";
import { useNavigate } from "react-router-dom";
import { MediaCard } from "@/components/MediaCard";
import { useToast } from "@/components/Toast";
import { ApiError, endpoints, type MediaItem } from "@/lib/api";
import styles from "./BrowsePage.module.css";

type Tab = "movies" | "tv";
type Kind = "latest" | "popular" | "top";
type Source = "tmdb" | "imdb" | "rt";

export function BrowsePage() {
  const [tab, setTab] = useState<Tab>("movies");
  const [kind, setKind] = useState<Kind>("latest");
  const [source, setSource] = useState<Source>("tmdb");
  const [page, setPage] = useState(1);
  const [searchInput, setSearchInput] = useState("");
  const [queryText, setQueryText] = useState("");
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const { showToast } = useToast();

  const searching = queryText.trim().length > 0;
  const searchType = tab === "movies" ? "movie" : "tv";

  const query = useQuery({
    queryKey: searching
      ? ["media-search", searchType, queryText, page]
      : ["media", tab, kind, source, page],
    queryFn: () =>
      searching
        ? endpoints.searchMedia(queryText.trim(), page, searchType)
        : tab === "movies"
          ? endpoints.movies(page, kind, source)
          : endpoints.tv(page, kind, source),
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
    setQueryText(searchInput.trim());
    setPage(1);
  };

  const clearSearch = () => {
    setSearchInput("");
    setQueryText("");
    setPage(1);
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
            onClick={() => {
              setTab("movies");
              setPage(1);
            }}
          >
            电影
          </button>
          <button
            type="button"
            className={tab === "tv" ? styles.active : undefined}
            onClick={() => {
              setTab("tv");
              setPage(1);
            }}
          >
            电视剧
          </button>
        </div>
        <div className={styles.tools}>
          {!searching && (
            <>
              <div className={styles.tabs} aria-label="排序">
                <button
                  type="button"
                  className={kind === "latest" ? styles.active : undefined}
                  onClick={() => {
                    setKind("latest");
                    setPage(1);
                  }}
                >
                  最新
                </button>
                <button
                  type="button"
                  className={kind === "popular" ? styles.active : undefined}
                  onClick={() => {
                    setKind("popular");
                    setPage(1);
                  }}
                >
                  最热
                </button>
                <button
                  type="button"
                  className={kind === "top" ? styles.active : undefined}
                  onClick={() => {
                    setKind("top");
                    setPage(1);
                  }}
                >
                  高分
                </button>
              </div>
              <div className={styles.tabs} aria-label="数据来源">
                <button
                  type="button"
                  className={source === "tmdb" ? styles.active : undefined}
                  onClick={() => {
                    setSource("tmdb");
                    setPage(1);
                  }}
                >
                  TMDB
                </button>
                <button
                  type="button"
                  className={source === "imdb" ? styles.active : undefined}
                  onClick={() => {
                    setSource("imdb");
                    setPage(1);
                  }}
                >
                  IMDb
                </button>
                <button
                  type="button"
                  className={source === "rt" ? styles.active : undefined}
                  onClick={() => {
                    setSource("rt");
                    setPage(1);
                  }}
                >
                  烂番茄
                </button>
              </div>
            </>
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
          onClick={() => setPage((p) => Math.max(1, p - 1))}
        >
          上一页
        </button>
        <span className="notice">第 {page} 页</span>
        <button
          type="button"
          className="btn ghost"
          disabled={items.length === 0}
          onClick={() => setPage((p) => p + 1)}
        >
          下一页
        </button>
      </div>
    </div>
  );
}
