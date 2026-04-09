use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use qobuz_player_client::stream::flac_source_stream::SeekableStreamReader;

use crate::{AppResult, client::Client, database::Database, models::Track};

pub enum DownloadResult {
    Cached(PathBuf),
    Streaming(SeekableStreamReader),
}

pub struct Downloader {
    audio_cache_dir: PathBuf,
    database: Arc<Database>,
    client: Arc<Client>,
}

impl Downloader {
    pub fn new(audio_cache_dir: PathBuf, database: Arc<Database>, client: Arc<Client>) -> Self {
        Self {
            audio_cache_dir,
            database,
            client,
        }
    }

    pub async fn ensure_track_is_downloaded(&mut self, track: &Track) -> AppResult<DownloadResult> {
        let cache_path = cache_path(track, &self.audio_cache_dir);
        self.database.set_cache_entry(cache_path.as_path()).await;

        if cache_path.exists() {
            tracing::info!("Playing from cache: {}", cache_path.display());
            return Ok(DownloadResult::Cached(cache_path));
        }

        let stream = self.client.stream_track(track.id, cache_path).await?;

        Ok(DownloadResult::Streaming(stream))
    }
}

fn cache_path(track: &Track, audio_cache_dir: &Path) -> PathBuf {
    let artist_name = track.artist_name.as_deref().unwrap_or("unknown");
    let artist_id = track
        .artist_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let album_title = track.album_title.as_deref().unwrap_or("unknown");
    let album_id = track.album_id.as_deref().unwrap_or("unknown");
    let track_title = &track.title;

    let artist_dir = format!(
        "{} ({})",
        sanitize_name(artist_name),
        sanitize_name(&artist_id),
    );
    let album_dir = format!(
        "{} ({})",
        sanitize_name(album_title),
        sanitize_name(album_id),
    );
    let extension = "flac";
    let track_file = format!(
        "{}_{}.{extension}",
        track.number,
        sanitize_name(track_title)
    );

    audio_cache_dir
        .join(artist_dir)
        .join(album_dir)
        .join(track_file)
}

fn sanitize_name(input: &str) -> String {
    let mut s: String = input
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            c if c.is_control() => '_',
            _ => c,
        })
        .collect();

    s = s.trim_matches([' ', '.']).to_string();

    let mut out = String::with_capacity(s.len());
    let mut prev_underscore = false;
    for ch in s.chars() {
        let ch2 = if ch == ' ' { '_' } else { ch };
        if ch2 == '_' {
            if prev_underscore {
                continue;
            }
            prev_underscore = true;
        } else {
            prev_underscore = false;
        }
        out.push(ch2);
    }

    if out.is_empty() {
        return "unknown".to_string();
    }

    const MAX: usize = 100;
    out.chars().take(MAX).collect()
}
