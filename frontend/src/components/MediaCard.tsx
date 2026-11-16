import { Link } from "react-router-dom";
import type { MediaItem } from "@/lib/api";
import styles from "./MediaCard.module.css";

type Props = {
  item: MediaItem;
  onTrack: (item: MediaItem) => void;
  tracking?: boolean;
  disabled?: boolean;
};

export function MediaCard({ item, onTrack, tracking, disabled }: Props) {
  const to = `/media/${item.media_type}/${item.tmdb_id}`;

  return (
    <article className={styles.card}>
      <Link to={to} className={styles.poster}>
        {item.poster_url ? (
          <img src={item.poster_url} alt={item.title} loading="lazy" />
        ) : (
          <div className={styles.placeholder}>无封面</div>
        )}
        <div className={styles.overlay}>
          <button
            type="button"
            className="btn"
            disabled={disabled || tracking}
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              onTrack(item);
            }}
          >
            {tracking ? "追踪中…" : "追踪"}
          </button>
        </div>
      </Link>
      <div className={styles.meta}>
        <h3>
          <Link to={to}>{item.title}</Link>
        </h3>
        <p>
          <span className="badge">{item.media_type === "tv" ? "电视剧" : "电影"}</span>
          {item.year && <span className="badge">{item.year}</span>}
          {item.vote_average != null && (
            <span className="badge">{item.vote_average.toFixed(1)}</span>
          )}
        </p>
      </div>
    </article>
  );
}
