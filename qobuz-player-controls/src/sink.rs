use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use qobuz_player_client::qobuz_models::TrackURL;
use qobuz_player_models::Track;
use rodio::cpal::traits::HostTrait;
use rodio::{decoder::DecoderBuilder, queue::queue};
use tokio::sync::watch::{self, Receiver, Sender};
use tokio::task::JoinHandle;

use crate::database::Database;
use crate::notification::NotificationBroadcast;
use crate::{Result, VolumeReceiver};

pub struct Sink {
    stream_handle: Option<rodio::OutputStream>,
    sink: Option<rodio::Sink>,
    sender: Option<Arc<rodio::queue::SourcesQueueInput>>,
    current_download: Arc<Mutex<Option<JoinHandle<()>>>>,
    done_buffering_tx: Sender<()>,
    broadcast: Arc<NotificationBroadcast>,
    audio_cache_dir: PathBuf,
    database: Arc<Database>,
    volume: VolumeReceiver,
}

impl Sink {
    pub fn new(
        volume: VolumeReceiver,
        broadcast: Arc<NotificationBroadcast>,
        audio_cache_dir: PathBuf,
        database: Arc<Database>,
    ) -> Result<Self> {
        let (done_buffering_tx, _) = watch::channel(());

        Ok(Self {
            sink: Default::default(),
            stream_handle: Default::default(),
            sender: Default::default(),
            current_download: Default::default(),
            done_buffering_tx,
            broadcast,
            audio_cache_dir,
            database,
            volume,
        })
    }

    pub fn done_buffering(&self) -> Receiver<()> {
        self.done_buffering_tx.subscribe()
    }

    pub async fn clear(&mut self) -> Result<()> {
        if let Some(handle) = self.current_download.lock()?.take() {
            handle.abort();
        }

        self.sink = None;
        self.sender = None;
        self.stream_handle = None;

        Ok(())
    }

    pub fn play(&self) {
        if let Some(sink) = &self.sink {
            sink.play();
        }
    }

    pub fn pause(&self) {
        if let Some(sink) = &self.sink {
            sink.pause();
        }
    }

    pub fn seek(&self, duration: Duration) -> Result<()> {
        if let Some(sink) = &self.sink {
            sink.try_seek(duration)?;
        }

        Ok(())
    }

    pub fn query_track(&mut self, track_url: TrackURL, track: &Track) -> Result<QueryTrackResult> {
        if let Some(handle) = self.current_download.lock()?.take() {
            handle.abort();
        }

        let sample_rate = (track_url.sampling_rate * 1000.0) as u32;

        if self.stream_handle.is_none() || self.sink.is_none() || self.sender.is_none() {
            let mut stream_handle = open_default_stream(sample_rate)?;
            stream_handle.log_on_drop(false);

            let (sender, receiver) = queue(true);
            let sink = rodio::Sink::connect_new(stream_handle.mixer());
            sink.append(receiver);
            set_volume(&sink, &self.volume.borrow());
            self.sink = Some(sink);
            self.sender = Some(sender);
            self.stream_handle = Some(stream_handle);
        }

        let same_sample_rate =
            sample_rate == self.stream_handle.as_ref().unwrap().config().sample_rate();

        let track_url_url = track_url.url;
        let sender = self.sender.as_ref().unwrap().clone();
        let done_buffering_tx = self.done_buffering_tx.clone();
        let broadcast = self.broadcast.clone();
        let database = self.database.clone();

        let cache_path = {
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
            let extension = guess_extension(&track_url.mime_type);
            let track_file = format!(
                "{}_{}.{extension}",
                track.number,
                sanitize_name(track_title)
            );
            self.audio_cache_dir
                .join(artist_dir)
                .join(album_dir)
                .join(track_file)
        };

        let handle = tokio::spawn(async move {
            database.set_cache_entry(cache_path.as_path()).await;

            let maybe_cached_bytes = fs::read(&cache_path).ok();

            let bytes: Vec<u8> = if let Some(bytes) = maybe_cached_bytes {
                bytes
            } else {
                let Ok(resp) = reqwest::get(&track_url_url).await else {
                    broadcast.send_error("Unable to get track audio file".to_string());
                    return;
                };
                let Ok(body) = resp.bytes().await else {
                    broadcast.send_error("Unable to get audio file bytes".to_string());
                    return;
                };
                let bytes = body.to_vec();

                if let Some(parent) = cache_path.parent()
                    && let Err(e) = fs::create_dir_all(parent)
                {
                    broadcast.send_error(format!("Unable to create cache directory: {e}"));
                }

                let tmp = cache_path.with_extension("partial");
                if let Err(e) = fs::write(&tmp, &bytes) {
                    broadcast.send_error(format!("Unable to write cache temp file: {e}"));
                } else if let Err(e) = fs::rename(&tmp, cache_path) {
                    let _ = fs::remove_file(&tmp);
                    broadcast.send_error(format!("Unable to finalize cache file: {e}"));
                }

                bytes
            };

            let cursor = Cursor::new(bytes);
            let Ok(source) = DecoderBuilder::new()
                .with_data(cursor)
                .with_seekable(true)
                .build()
            else {
                broadcast.send_error("Unable to decode audio file".to_string());
                return;
            };

            done_buffering_tx.send(()).expect("infallible");

            if !same_sample_rate {
                return;
            }

            sender.append(source);
        });

        *self.current_download.lock()? = Some(handle);

        Ok(match same_sample_rate {
            true => QueryTrackResult::Queued,
            false => QueryTrackResult::NotQueued,
        })
    }

    pub fn sync_volume(&self) {
        if let Some(sink) = &self.sink {
            set_volume(sink, &self.volume.borrow());
        }
    }
}

fn set_volume(sink: &rodio::Sink, volume: &f32) {
    let volume = volume.clamp(0.0, 1.0).powi(3);
    sink.set_volume(volume);
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

fn open_default_stream(sample_rate: u32) -> Result<rodio::OutputStream> {
    rodio::OutputStreamBuilder::from_default_device()
        .and_then(|x| x.with_sample_rate(sample_rate).open_stream())
        .or_else(|original_err| {
            let mut devices = rodio::cpal::default_host().output_devices()?;

            Ok(devices
                .find_map(|d| {
                    rodio::OutputStreamBuilder::from_device(d)
                        .and_then(|x| x.with_sample_rate(sample_rate).open_stream_or_fallback())
                        .ok()
                })
                .ok_or(original_err)?)
        })
}

fn guess_extension(mime: &str) -> String {
    match mime {
        m if m.contains("mp4") => "mp4".to_string(),
        m if m.contains("mp3") => "mp3".to_string(),
        m if m.contains("flac") => "flac".to_string(),
        _ => "unknown".to_string(),
    }
}

pub enum QueryTrackResult {
    Queued,
    NotQueued,
}
