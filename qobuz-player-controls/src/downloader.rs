use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{cmaf, crypto, database::Database, notification::NotificationBroadcast};
use qobuz_player_client::qobuz_models::TrackURL;
use qobuz_player_models::Track;
use tokio::{
    sync::watch::{self, Receiver, Sender},
    task::JoinHandle,
};

pub struct Downloader {
    audio_cache_dir: PathBuf,
    database: Arc<Database>,
    broadcast: Arc<NotificationBroadcast>,
    done_buffering_tx: Sender<PathBuf>,
    download_handle: Option<JoinHandle<()>>,
}

impl Downloader {
    pub fn new(
        audio_cache_dir: PathBuf,
        broadcast: Arc<NotificationBroadcast>,
        database: Arc<Database>,
    ) -> Self {
        let (done_buffering_tx, _) = watch::channel(Default::default());

        Self {
            audio_cache_dir,
            done_buffering_tx,
            database,
            broadcast,
            download_handle: None,
        }
    }

    pub fn done_buffering(&self) -> Receiver<PathBuf> {
        self.done_buffering_tx.subscribe()
    }

    pub async fn ensure_track_is_downloaded(
        &mut self,
        track_url: TrackURL,
        session_infos: Option<String>,
        track: &Track,
    ) -> Option<PathBuf> {
        if let Some(handle) = &self.download_handle {
            handle.abort();
            self.download_handle = None;
        }

        let cache_path = cache_path(track, &track_url.mime_type, &self.audio_cache_dir);
        self.database.set_cache_entry(cache_path.as_path()).await;

        if cache_path.exists() {
            return Some(cache_path);
        }

        let done_buffering = self.done_buffering_tx.clone();
        let broadcast = self.broadcast.clone();
        let n_segments = track_url.n_segments;
        let template = track_url.url_template.clone();
        let key_str = track_url.key.clone();

        tracing::info!("Downloading segmented track: {}", track.title);

        let handle = tokio::spawn(async move {
            let content_key = match (key_str, session_infos.as_deref()) {
                (Some(key_str), Some(infos)) => {
                    match crypto::derive_session_key(infos)
                        .and_then(|session_key| crypto::unwrap_content_key(&session_key, &key_str))
                    {
                        Ok(ck) => Some(ck),
                        Err(e) => {
                            broadcast.send_error(format!("Failed to derive content key: {e}"));
                            return;
                        }
                    }
                }
                _ => None,
            };
            let key = content_key.unwrap_or([0u8; 16]);

            let seg0_url = template.replace("$SEGMENT$", "0");
            let init_bytes = match fetch_segment(&seg0_url, 0).await {
                Ok(b) => b,
                Err(e) => {
                    broadcast.send_error(format!("Init segment error: {e}"));
                    return;
                }
            };

            let init_info = match cmaf::parse_init_segment(&init_bytes) {
                Ok(info) => info,
                Err(e) => {
                    broadcast.send_error(format!("Init segment parse error: {e}"));
                    return;
                }
            };
            let flac_header = init_info.flac_header;

            let mut out = Vec::new();
            out.extend_from_slice(&flac_header);

            for seg_idx in 1..n_segments {
                let url = template.replace("$SEGMENT$", &seg_idx.to_string());
                let seg_bytes = match fetch_segment(&url, seg_idx).await {
                    Ok(b) => b,
                    Err(e) => {
                        broadcast.send_error(format!("Segment {seg_idx} fetch error: {e}"));
                        return;
                    }
                };

                let crypto_info = match cmaf::parse_segment_crypto(&seg_bytes) {
                    Ok(c) => c,
                    Err(e) => {
                        broadcast.send_error(format!("Crypto parse error seg {seg_idx}: {e}"));
                        return;
                    }
                };

                let mut offset = crypto_info.data_offset;
                for entry in crypto_info.entries {
                    let end = offset + entry.size as usize;
                    if end > seg_bytes.len() {
                        broadcast.send_error(format!("Segment {seg_idx} truncated"));
                        return;
                    }

                    let mut frame = seg_bytes[offset..end].to_vec();
                    if entry.flags != 0 {
                        crypto::decrypt_frame(&key, &entry.iv, &mut frame);
                    }

                    out.extend_from_slice(&frame);
                    offset = end;
                }
            }

            if let Some(parent) = cache_path.parent()
                && let Err(e) = fs::create_dir_all(parent)
            {
                broadcast.send_error(format!("Cache mkdir error: {e}"));
                return;
            }

            let tmp = cache_path.with_extension("partial");
            if let Err(e) = fs::write(&tmp, &out) {
                broadcast.send_error(format!("Failed to write temp file: {e}"));
                return;
            }
            if let Err(e) = fs::rename(&tmp, &cache_path) {
                let _ = fs::remove_file(&tmp);
                broadcast.send_error(format!("Failed to finalize cache: {e}"));
                return;
            }

            done_buffering.send(cache_path.clone()).unwrap();
        });

        self.download_handle = Some(handle);
        None
    }
}

async fn fetch_segment(url: &str, index: u8) -> Result<Vec<u8>, String> {
    let resp = reqwest::get(url)
        .await
        .map_err(|e| format!("Segment {index} request failed: {e}"))?;
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Segment {index} read error: {e}"))?;
    Ok(bytes.to_vec())
}

fn cache_path(track: &Track, mime: &str, audio_cache_dir: &Path) -> PathBuf {
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
        sanitize_name(&artist_id)
    );
    let album_dir = format!(
        "{} ({})",
        sanitize_name(album_title),
        sanitize_name(album_id)
    );
    let extension = guess_extension(mime);

    let track_file = format!(
        "{}_{}.{}",
        track.number,
        sanitize_name(track_title),
        extension
    );

    audio_cache_dir
        .join(artist_dir)
        .join(album_dir)
        .join(track_file)
}

fn sanitize_name(input: &str) -> String {
    let mut s = input
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            c if c.is_control() => '_',
            _ => c,
        })
        .collect::<String>();

    s = s.trim_matches([' ', '.']).to_string();

    let mut out = String::new();
    let mut prev_us = false;
    for ch in s.chars() {
        let ch2 = if ch == ' ' { '_' } else { ch };
        if ch2 == '_' {
            if prev_us {
                continue;
            }
            prev_us = true;
        } else {
            prev_us = false;
        }
        out.push(ch2);
    }

    if out.is_empty() {
        "unknown".into()
    } else {
        out.chars().take(100).collect()
    }
}

fn guess_extension(mime: &str) -> String {
    match mime {
        m if m.contains("flac") => "flac".to_string(),
        m if m.contains("mp4") => "mp4".to_string(),
        m if m.contains("mpeg") => "mp3".to_string(),
        m if m.contains("mp3") => "mp3".to_string(),
        _ => "unknown".to_string(),
    }
}
