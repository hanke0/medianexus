import type { MediaItem } from "@/lib/api";

type Props = {
  item: Pick<MediaItem, "imdb_rating" | "rt_rating" | "vote_average">;
};

export function RatingBadges({ item }: Props) {
  return (
    <>
      {item.imdb_rating != null && (
        <span className="badge imdb">IMDb {item.imdb_rating.toFixed(1)}</span>
      )}
      {item.rt_rating != null && (
        <span className="badge rt">RT {Math.round(item.rt_rating)}%</span>
      )}
      {item.vote_average != null && (
        <span className="badge">TMDB {item.vote_average.toFixed(1)}</span>
      )}
    </>
  );
}
