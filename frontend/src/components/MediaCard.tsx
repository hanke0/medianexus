import { Link } from "react-router-dom";
import type { MediaItem } from "@/lib/api";
import styles from "./MediaCard.module.css";

function CardScores({ item }: { item: MediaItem }) {
  const scores = [
    item.imdb_rating != null ? `IMDb ${item.imdb_rating.toFixed(1)}` : null,
    item.rt_rating != null ? `RT ${Math.round(item.rt_rating)}` : null,
    item.vote_average != null ? `TMDB ${item.vote_average.toFixed(1)}` : null,
  ].filter((score): score is string => score != null);

  if (scores.length === 0) return null;

  return (
    <p className={styles.scores}>
      {scores.map((score) => (
        <span key={score}>{score}</span>
      ))}
    </p>
  );
}

type Props = {
  item: MediaItem;
  onTrack: (item: MediaItem) => void;
  tracking?: boolean;
  disabled?: boolean;
  actionLabel?: string;
};

export function MediaCard({ item, onTrack, tracking, disabled, actionLabel }: Props) {
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
            {tracking ? "追踪中…" : actionLabel ?? "追踪"}
          </button>
        </div>
      </Link>
      <div className={styles.meta}>
        <h3>
          <Link to={to}>{item.title}</Link>
        </h3>
        {item.year ? <p className={styles.facts}>{item.year}</p> : null}
        <CardScores item={item} />
      </div>
    </article>
  );
}
