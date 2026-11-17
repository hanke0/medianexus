use crate::error::{AppError, AppResult};
use std::path::{Path, PathBuf};

const VIDEO_EXTS: &[&str] = &[
    "mkv", "mp4", "avi", "mov", "wmv", "ts", "m2ts", "flv", "webm", "mpg", "mpeg",
];

#[derive(Debug, Clone)]
pub struct FolderNameVars<'a> {
    pub title: &'a str,
    pub year: Option<&'a str>,
    pub original_title: Option<&'a str>,
    pub tmdb_id: i64,
}

/// Library subfolder for a track media type. Movies → `movies`; everything else → `tv`.
pub fn library_kind_dir(media_type: &str) -> &'static str {
    match media_type {
        "movie" => "movies",
        _ => "tv",
    }
}

/// Recursively hardlink files from `src` into `dest_root/{folder}/...`.
/// Falls back to copy if hardlink is not supported (cross-device).
pub async fn hardlink_media(src: &Path, dest_root: &Path, folder_name: &str) -> AppResult<PathBuf> {
    let safe_title = sanitize_name(folder_name);
    let dest_dir = dest_root.join(&safe_title);
    tokio::fs::create_dir_all(&dest_dir).await?;

    let src = src.to_path_buf();
    let dest_dir_clone = dest_dir.clone();
    tokio::task::spawn_blocking(move || hardlink_recursive(&src, &dest_dir_clone))
        .await
        .map_err(|e| AppError::Internal(format!("hardlink task join: {e}")))??;

    Ok(dest_dir)
}

/// TV layout: `{show}/{show}-S01/…`, keeping each file's original name.
pub async fn hardlink_tv_season(
    src: &Path,
    library_tv_root: &Path,
    show_folder: &str,
    season: u32,
) -> AppResult<PathBuf> {
    let show = sanitize_name(show_folder);
    let season_dir = library_tv_root
        .join(&show)
        .join(format!("{show}-S{season:02}"));
    tokio::fs::create_dir_all(&season_dir).await?;

    let src = src.to_path_buf();
    let dest = season_dir.clone();
    tokio::task::spawn_blocking(move || hardlink_recursive(&src, &dest))
        .await
        .map_err(|e| AppError::Internal(format!("hardlink task join: {e}")))??;

    Ok(season_dir)
}

/// Render folder name from template, e.g. `{{title}}-{{year}}`.
pub fn render_folder_name(template: &str, vars: &FolderNameVars<'_>) -> String {
    let year = vars.year.unwrap_or("");
    let original = vars.original_title.unwrap_or("");
    let rendered = template
        .replace("{{title}}", vars.title)
        .replace("{{year}}", year)
        .replace("{{original_title}}", original)
        .replace("{{tmdb_id}}", &vars.tmdb_id.to_string());

    cleanup_template_result(&rendered)
}

fn cleanup_template_result(s: &str) -> String {
    let mut out = s.to_string();
    // Collapse repeated separators left by empty placeholders.
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    while out.contains("  ") {
        out = out.replace("  ", " ");
    }
    out.trim()
        .trim_matches(|c: char| matches!(c, '-' | '_' | '.' | ' '))
        .to_string()
}

fn hardlink_recursive(src: &Path, dest_dir: &Path) -> AppResult<()> {
    let meta = std::fs::metadata(src)?;
    if meta.is_file() {
        if is_video(src) || is_subtitle(src) {
            let dest = dest_dir.join(
                src.file_name()
                    .ok_or_else(|| AppError::Internal("invalid file name".into()))?,
            );
            link_or_copy(src, &dest)?;
        }
        return Ok(());
    }

    if meta.is_dir() {
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let nested = dest_dir.join(entry.file_name());
                std::fs::create_dir_all(&nested)?;
                hardlink_recursive(&path, &nested)?;
            } else {
                hardlink_recursive(&path, dest_dir)?;
            }
        }
    }
    Ok(())
}

fn link_or_copy(src: &Path, dest: &Path) -> AppResult<()> {
    if dest.exists() {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::hard_link(src, dest) {
        Ok(()) => Ok(()),
        Err(e)
            if e.raw_os_error() == Some(18) || e.kind() == std::io::ErrorKind::CrossesDevices =>
        {
            // EXDEV — fall back to copy
            tracing::warn!(
                "hardlink not supported for {} → {}, copying instead",
                src.display(),
                dest.display()
            );
            std::fs::copy(src, dest)?;
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTS.iter().any(|x| x.eq_ignore_ascii_case(e)))
        .unwrap_or(false)
}

fn is_subtitle(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            matches!(
                e.to_ascii_lowercase().as_str(),
                "srt" | "ass" | "ssa" | "sub" | "idx" | "vtt"
            )
        })
        .unwrap_or(false)
}

fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.');
    if trimmed.is_empty() {
        "media".into()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_title_year() {
        let name = render_folder_name(
            "{{title}}-{{year}}",
            &FolderNameVars {
                title: "黑客帝国",
                year: Some("1999"),
                original_title: Some("The Matrix"),
                tmdb_id: 603,
            },
        );
        assert_eq!(name, "黑客帝国-1999");
    }

    #[test]
    fn drops_trailing_dash_when_year_missing() {
        let name = render_folder_name(
            "{{title}}-{{year}}",
            &FolderNameVars {
                title: "Untitled",
                year: None,
                original_title: None,
                tmdb_id: 1,
            },
        );
        assert_eq!(name, "Untitled");
    }

    #[test]
    fn library_kind_dirs() {
        assert_eq!(library_kind_dir("movie"), "movies");
        assert_eq!(library_kind_dir("tv"), "tv");
        assert_eq!(library_kind_dir("other"), "tv");
    }

    #[tokio::test]
    async fn tv_season_layout_keeps_original_names() {
        let root = std::env::temp_dir().join(format!("mn-hl-{}", uuid::Uuid::new_v4()));
        let src = root.join("torrent");
        let lib = root.join("tv");
        std::fs::create_dir_all(src.join("nested")).unwrap();
        std::fs::write(src.join("nested").join("Show.S01E01.1080p.mkv"), b"a").unwrap();
        std::fs::write(src.join("episode.mp4"), b"b").unwrap();
        std::fs::write(src.join("notes.txt"), b"skip").unwrap();

        let dest = hardlink_tv_season(&src, &lib, "TvShowName", 1)
            .await
            .unwrap();
        assert_eq!(dest, lib.join("TvShowName").join("TvShowName-S01"));
        assert!(dest.join("nested").join("Show.S01E01.1080p.mkv").is_file());
        assert!(dest.join("episode.mp4").is_file());
        assert!(!dest.join("notes.txt").exists());
        assert!(!dest.join("S01E01.mkv").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
