//! Classify PT release names as a season pack (全集) or a single episode.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TvReleaseKind {
    SeasonPack,
    Episode(u32),
}

pub fn target_label(season: i64, episode: Option<i64>) -> String {
    match episode {
        Some(ep) => format!("S{season:02}E{ep:02}"),
        None => format!("S{season:02}"),
    }
}

/// First `SxxEyy` (or `1x02`) in a filename, when the name is a single episode.
pub fn episode_tag(name: &str) -> Option<(u32, u32)> {
    let parsed = parse(name);
    if parsed.range || parsed.episodes.len() != 1 {
        return None;
    }
    Some(parsed.episodes[0])
}

/// How `name` relates to `season`. Names that belong to another season are ignored.
pub fn classify_release(name: &str, season: u32) -> Option<TvReleaseKind> {
    let parsed = parse(name);
    let mut this_eps = Vec::new();
    let mut other = false;
    for (s, e) in &parsed.episodes {
        if *s == season {
            if !this_eps.contains(e) {
                this_eps.push(*e);
            }
        } else {
            other = true;
        }
    }
    if parsed.bare_seasons.iter().any(|s| *s != season) {
        other = true;
    }
    let cn = chinese_seasons(name);
    if cn.iter().any(|s| *s != season) {
        other = true;
    }
    if other {
        return None;
    }
    let this_bare = parsed.bare_seasons.contains(&season) || cn.contains(&season);
    if this_eps.is_empty() && !this_bare {
        return None;
    }
    if parsed.pack_word || parsed.range || this_eps.len() >= 2 || this_eps.is_empty() {
        return Some(TvReleaseKind::SeasonPack);
    }
    Some(TvReleaseKind::Episode(this_eps[0]))
}

struct Parsed {
    episodes: Vec<(u32, u32)>,
    bare_seasons: Vec<u32>,
    range: bool,
    pack_word: bool,
}

fn parse(name: &str) -> Parsed {
    let bytes = name.as_bytes();
    let mut episodes = Vec::new();
    let mut bare_seasons = Vec::new();
    let mut range = false;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] >= 0x80 {
            i += utf8_len(bytes[i]);
            continue;
        }
        if let Some(hit) = try_episode(bytes, i) {
            episodes.push((hit.season, hit.episode));
            range |= hit.range;
            i = hit.end;
            continue;
        }
        if let Some(hit) = try_bare_s(bytes, i) {
            if !bare_seasons.contains(&hit.season) {
                bare_seasons.push(hit.season);
            }
            i = hit.end;
            continue;
        }
        if let Some(hit) = try_season_word(bytes, i) {
            match hit.kind {
                HitKind::Episode(ep) => episodes.push((hit.season, ep)),
                HitKind::Bare => {
                    if !bare_seasons.contains(&hit.season) {
                        bare_seasons.push(hit.season);
                    }
                }
            }
            range |= hit.range;
            i = hit.end;
            continue;
        }
        if let Some(hit) = try_x_pattern(bytes, i) {
            episodes.push((hit.season, hit.episode));
            i = hit.end;
            continue;
        }
        i += 1;
    }
    Parsed {
        episodes,
        bare_seasons,
        range,
        pack_word: has_pack_word(name),
    }
}

struct Hit {
    season: u32,
    episode: u32,
    end: usize,
    range: bool,
    kind: HitKind,
}

enum HitKind {
    Episode(u32),
    Bare,
}

fn try_episode(bytes: &[u8], i: usize) -> Option<Hit> {
    if !boundary(bytes, i) || bytes[i].to_ascii_lowercase() != b's' {
        return None;
    }
    let (season, mut j) = read_u32(bytes, i + 1)?;
    if season > 80 {
        return None;
    }
    j = skip_sep(bytes, j);
    if starts_with_ci(bytes, j, b"ep") {
        j += 2;
    } else if j < bytes.len() && bytes[j].to_ascii_lowercase() == b'e' {
        j += 1;
    } else {
        return None;
    }
    let (episode, mut end) = read_u32(bytes, j)?;
    if episode == 0 || episode > 9999 {
        return None;
    }
    let mut range = false;
    // Don't treat the range dash as a separator (`S01E01-E12`).
    if let Some(range_end) = try_range(bytes, end, episode) {
        range = true;
        end = range_end;
    }
    Some(Hit {
        season,
        episode,
        end,
        range,
        kind: HitKind::Episode(episode),
    })
}

fn try_bare_s(bytes: &[u8], i: usize) -> Option<Hit> {
    if !boundary(bytes, i) || bytes[i].to_ascii_lowercase() != b's' {
        return None;
    }
    let (season, end) = read_u32(bytes, i + 1)?;
    if season > 80 {
        return None;
    }
    let k = skip_sep(bytes, end);
    if k < bytes.len() && bytes[k].to_ascii_lowercase() == b'e' {
        return None;
    }
    Some(Hit {
        season,
        episode: 0,
        end,
        range: false,
        kind: HitKind::Bare,
    })
}

fn try_season_word(bytes: &[u8], i: usize) -> Option<Hit> {
    if !boundary(bytes, i) || !starts_with_ci(bytes, i, b"season") {
        return None;
    }
    let mut j = i + 6;
    if j < bytes.len() && bytes[j].is_ascii_alphabetic() {
        return None;
    }
    j = skip_sep(bytes, j);
    let (season, mut end) = read_u32(bytes, j)?;
    if season > 80 {
        return None;
    }
    j = skip_sep(bytes, end);
    if starts_with_episode_word(bytes, j) {
        let word_len = if starts_with_ci(bytes, j, b"episode") {
            7
        } else {
            2
        };
        let mut k = j + word_len;
        if k < bytes.len() && bytes[k].is_ascii_alphabetic() {
            return None;
        }
        k = skip_sep(bytes, k);
        let (episode, ep_end) = read_u32(bytes, k)?;
        if episode == 0 || episode > 9999 {
            return None;
        }
        end = ep_end;
        return Some(Hit {
            season,
            episode,
            end,
            range: false,
            kind: HitKind::Episode(episode),
        });
    }
    Some(Hit {
        season,
        episode: 0,
        end,
        range: false,
        kind: HitKind::Bare,
    })
}

fn try_x_pattern(bytes: &[u8], i: usize) -> Option<Hit> {
    if !boundary(bytes, i) {
        return None;
    }
    let (season, j) = read_u32(bytes, i)?;
    let season_digits = j - i;
    if season_digits == 0 || season_digits > 2 || !(1..=80).contains(&season) {
        return None;
    }
    if j >= bytes.len() || bytes[j].to_ascii_lowercase() != b'x' {
        return None;
    }
    let (episode, end) = read_u32(bytes, j + 1)?;
    let ep_digits = end - (j + 1);
    if ep_digits < 2 || episode == 0 || episode > 9999 {
        return None;
    }
    if end < bytes.len() && bytes[end].is_ascii_alphanumeric() {
        return None;
    }
    Some(Hit {
        season,
        episode,
        end,
        range: false,
        kind: HitKind::Episode(episode),
    })
}

fn try_range(bytes: &[u8], i: usize, start_ep: u32) -> Option<usize> {
    if i >= bytes.len() {
        return None;
    }
    let dash = bytes[i] == b'-' || bytes[i] == b'~';
    if !dash {
        return None;
    }
    let mut j = skip_sep(bytes, i + 1);
    let mut marked = false;
    if j < bytes.len() && bytes[j].to_ascii_lowercase() == b's' {
        let (_, next) = read_u32(bytes, j + 1)?;
        j = skip_sep(bytes, next);
        marked = true;
    }
    if starts_with_ci(bytes, j, b"ep") {
        j += 2;
        marked = true;
    } else if j < bytes.len() && bytes[j].to_ascii_lowercase() == b'e' {
        j += 1;
        marked = true;
    }
    j = skip_sep(bytes, j);
    let (n, end) = read_u32(bytes, j)?;
    // `S01E01-1080p` is a single episode, not a range.
    if !marked && (n > 200 || n <= start_ep) {
        return None;
    }
    Some(end)
}

fn starts_with_episode_word(bytes: &[u8], i: usize) -> bool {
    if starts_with_ci(bytes, i, b"episode") {
        let k = i + 7;
        return k >= bytes.len() || !bytes[k].is_ascii_alphabetic();
    }
    if starts_with_ci(bytes, i, b"ep") {
        let k = i + 2;
        return k >= bytes.len() || !bytes[k].is_ascii_alphabetic();
    }
    false
}

fn has_pack_word(name: &str) -> bool {
    name.contains("全集")
        || name.contains("全季")
        || name.contains("整季")
        || has_quan_count(name)
        || contains_ascii_word(name, "complete")
        || (contains_ascii_word(name, "season") && contains_ascii_word(name, "pack"))
}

fn has_quan_count(name: &str) -> bool {
    let chars: Vec<char> = name.chars().collect();
    for i in 0..chars.len() {
        if chars[i] != '全' {
            continue;
        }
        let mut j = i + 1;
        let mut digits = 0;
        while j < chars.len() && chars[j].is_ascii_digit() {
            digits += 1;
            j += 1;
        }
        if digits > 0 && j < chars.len() && chars[j] == '集' {
            return true;
        }
    }
    false
}

fn chinese_seasons(name: &str) -> Vec<u32> {
    let chars: Vec<char> = name.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '第' {
            if let Some((n, len)) = parse_cn_or_ascii(&chars[i + 1..]) {
                let after = i + 1 + len;
                if after < chars.len()
                    && chars[after] == '季'
                    && (1..=80).contains(&n)
                    && !out.contains(&n)
                {
                    out.push(n);
                }
            }
        }
        i += 1;
    }
    out
}

fn parse_cn_or_ascii(chars: &[char]) -> Option<(u32, usize)> {
    if chars.first().is_some_and(|c| c.is_ascii_digit()) {
        let mut n: u32 = 0;
        let mut len = 0;
        for c in chars {
            if let Some(d) = c.to_digit(10) {
                n = n.saturating_mul(10).saturating_add(d);
                len += 1;
            } else {
                break;
            }
        }
        return (len > 0).then_some((n, len));
    }
    parse_chinese_num(chars)
}

fn parse_chinese_num(chars: &[char]) -> Option<(u32, usize)> {
    let d = |c: char| -> Option<u32> {
        Some(match c {
            '一' => 1,
            '二' | '两' => 2,
            '三' => 3,
            '四' => 4,
            '五' => 5,
            '六' => 6,
            '七' => 7,
            '八' => 8,
            '九' => 9,
            _ => return None,
        })
    };
    if chars.first() == Some(&'十') {
        if let Some(n) = chars.get(1).copied().and_then(d) {
            return Some((10 + n, 2));
        }
        return Some((10, 1));
    }
    let first = chars.first().copied().and_then(d)?;
    if chars.get(1) == Some(&'十') {
        if let Some(n) = chars.get(2).copied().and_then(d) {
            return Some((first * 10 + n, 3));
        }
        return Some((first * 10, 2));
    }
    Some((first, 1))
}

fn contains_ascii_word(hay: &str, word: &str) -> bool {
    let h = hay.as_bytes();
    let w = word.as_bytes();
    if w.is_empty() || h.len() < w.len() {
        return false;
    }
    let mut i = 0;
    while i + w.len() <= h.len() {
        if h[i..i + w.len()].eq_ignore_ascii_case(w) {
            let before = i == 0 || !h[i - 1].is_ascii_alphanumeric();
            let after = i + w.len() >= h.len() || !h[i + w.len()].is_ascii_alphanumeric();
            if before && after {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn boundary(bytes: &[u8], i: usize) -> bool {
    i == 0 || !bytes[i - 1].is_ascii_alphanumeric()
}

fn skip_sep(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && matches!(bytes[i], b'.' | b'_' | b'-' | b' ' | b'[' | b']') {
        i += 1;
    }
    i
}

fn read_u32(bytes: &[u8], i: usize) -> Option<(u32, usize)> {
    if i >= bytes.len() || !bytes[i].is_ascii_digit() {
        return None;
    }
    let mut j = i;
    let mut n: u32 = 0;
    while j < bytes.len() && bytes[j].is_ascii_digit() {
        n = n
            .saturating_mul(10)
            .saturating_add((bytes[j] - b'0') as u32);
        j += 1;
        if j - i > 4 {
            break;
        }
    }
    Some((n, j))
}

fn starts_with_ci(bytes: &[u8], i: usize, prefix: &[u8]) -> bool {
    let Some(slice) = bytes.get(i..i + prefix.len()) else {
        return false;
    };
    slice.eq_ignore_ascii_case(prefix)
}

fn utf8_len(b: u8) -> usize {
    if b & 0xE0 == 0xC0 {
        2
    } else if b & 0xF0 == 0xE0 {
        3
    } else if b & 0xF8 == 0xF0 {
        4
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_episode_tags() {
        assert_eq!(
            classify_release("Show.S01E03.1080p.mkv", 1),
            Some(TvReleaseKind::Episode(3))
        );
        assert_eq!(
            classify_release("Show.S01.E03.1080p", 1),
            Some(TvReleaseKind::Episode(3))
        );
        assert_eq!(
            classify_release("Show.1x02.720p", 1),
            Some(TvReleaseKind::Episode(2))
        );
        assert_eq!(
            classify_release("Show Season 1 Episode 4", 1),
            Some(TvReleaseKind::Episode(4))
        );
        assert_eq!(episode_tag("Show.S01E03.1080p.mkv"), Some((1, 3)));
    }

    #[test]
    fn season_packs_preferred_shape() {
        assert_eq!(
            classify_release("Show.S01.1080p.BluRay", 1),
            Some(TvReleaseKind::SeasonPack)
        );
        assert_eq!(
            classify_release("Show.S01.全集.1080p", 1),
            Some(TvReleaseKind::SeasonPack)
        );
        assert_eq!(
            classify_release("Show.S01E01-E12.1080p", 1),
            Some(TvReleaseKind::SeasonPack)
        );
        assert_eq!(
            classify_release("Show.S01E01.S01E02", 1),
            Some(TvReleaseKind::SeasonPack)
        );
        assert_eq!(
            classify_release("某某 第一季 全集", 1),
            Some(TvReleaseKind::SeasonPack)
        );
        assert_eq!(
            classify_release("某某 第12季 全24集", 12),
            Some(TvReleaseKind::SeasonPack)
        );
        assert_eq!(
            classify_release("Show.S01.Complete", 1),
            Some(TvReleaseKind::SeasonPack)
        );
        assert_eq!(
            classify_release("Show.S01E01-1080p.mkv", 1),
            Some(TvReleaseKind::Episode(1))
        );
    }

    #[test]
    fn ignores_other_seasons() {
        assert_eq!(classify_release("Show.S02E01.1080p", 1), None);
        assert_eq!(classify_release("Show.S01E01.S02E01", 1), None);
        assert_eq!(classify_release("全集", 1), None);
    }

    #[test]
    fn label() {
        assert_eq!(target_label(1, None), "S01");
        assert_eq!(target_label(1, Some(3)), "S01E03");
    }
}
