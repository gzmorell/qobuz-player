use std::fs;
use std::num::NonZero;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use rodio::cpal::traits::HostTrait;
use rodio::queue::queue;
use rodio::{Decoder, DeviceTrait, Source};
use tokio::sync::watch::{self, Receiver, Sender};
use tokio::task::JoinHandle;
use tokio::time::sleep;

use crate::error::Error;
use crate::stderr_redirect::silence_stderr;
use crate::{AppResult, VolumeReceiver};

pub struct Sink {
    player: Option<rodio::Player>,
    mixer: Option<rodio::MixerDeviceSink>,
    sender: Option<Arc<rodio::queue::SourcesQueueInput>>,
    volume: VolumeReceiver,
    track_finished: Sender<()>,
    track_handle: Option<JoinHandle<()>>,
    duration_played: Arc<Mutex<Duration>>,
    preferred_device_name: Option<String>,
}

impl Sink {
    pub fn new(volume: VolumeReceiver, preferred_device_name: Option<String>) -> AppResult<Self> {
        let (track_finished, _) = watch::channel(());
        Ok(Self {
            player: None,
            mixer: None,
            sender: None,
            volume,
            track_finished,
            track_handle: Default::default(),
            duration_played: Default::default(),
            preferred_device_name,
        })
    }

    pub fn track_finished(&self) -> Receiver<()> {
        self.track_finished.subscribe()
    }

    pub fn position(&self) -> Duration {
        let position = self
            .player
            .as_ref()
            .map(|x| x.get_pos())
            .unwrap_or_default();

        let duration_played = *self.duration_played.lock();

        if position < duration_played {
            return Default::default();
        }

        position - duration_played
    }

    pub fn play(&self) {
        if let Some(player) = &self.player {
            player.play();
        }
    }

    pub fn pause(&self) {
        if let Some(player) = &self.player {
            player.pause();
        }
    }

    pub fn seek(&self, duration: Duration) -> AppResult<()> {
        if let Some(player) = &self.player {
            match player.try_seek(duration) {
                Ok(_) => {
                    *self.duration_played.lock() = Default::default();
                }
                Err(err) => return Err(err.into()),
            };
        }

        Ok(())
    }

    pub fn clear(&mut self) -> AppResult<()> {
        tracing::info!("Clearing sink");
        self.clear_queue()?;

        self.player = None;
        self.mixer = None;
        self.sender = None;

        *self.duration_played.lock() = Default::default();

        if let Some(handle) = self.track_handle.take() {
            handle.abort();
        }

        Ok(())
    }

    pub fn clear_queue(&mut self) -> AppResult<()> {
        tracing::info!("Clearing sink queue");
        *self.duration_played.lock() = Default::default();

        if let Some(player) = self.player.as_ref() {
            player.clear();
        };
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.player.is_none()
    }

    pub fn query_track(&mut self, track_path: &Path) -> AppResult<QueryTrackResult> {
        tracing::info!("Sink query track: {}", track_path.to_string_lossy());

        let file = fs::File::open(track_path).map_err(|err| Error::StreamError {
            message: format!("Failed to read file: {track_path:?}: {err}"),
        })?;

        let source = Decoder::try_from(file)?;

        let sample_rate = source.sample_rate();
        let same_sample_rate = self
            .mixer
            .as_ref()
            .map(|mixer| mixer.config().sample_rate() == sample_rate)
            .unwrap_or(true);

        if !same_sample_rate {
            return Ok(QueryTrackResult::RecreateStreamRequired);
        }

        let needs_stream = self.mixer.is_none() || self.player.is_none();

        if needs_stream {
            let mut mixer =
                if let Some(preferred_device_name) = self.preferred_device_name.as_deref() {
                    silence_stderr(|| open_preferred_stream(sample_rate, preferred_device_name))?
                } else {
                    open_default_stream(sample_rate)?
                };
            mixer.log_on_drop(false);

            let (sender, receiver) = queue(true);
            let player = rodio::Player::connect_new(mixer.mixer());
            player.append(receiver);
            set_volume(&player, &self.volume.borrow());

            self.player = Some(player);
            self.sender = Some(sender);
            self.mixer = Some(mixer);
        }

        let track_finished = self.track_finished.clone();
        let track_duration = source.total_duration().unwrap_or_default();

        let duration_played = self.duration_played.clone();
        let signal = self.sender.as_ref().unwrap().append_with_signal(source);

        let track_handle = tokio::spawn(async move {
            loop {
                if signal.try_recv().is_ok() {
                    *duration_played.lock() += track_duration;
                    track_finished.send(()).expect("infallible");
                    break;
                }
                sleep(Duration::from_millis(200)).await;
            }
        });

        self.track_handle = Some(track_handle);

        Ok(QueryTrackResult::Queued)
    }

    pub fn sync_volume(&self) {
        if let Some(player) = &self.player {
            set_volume(player, &self.volume.borrow());
        }
    }
}

fn set_volume(sink: &rodio::Player, volume: &f32) {
    let volume = volume.clamp(0.0, 1.0).powi(3);
    sink.set_volume(volume);
}

fn open_default_stream(sample_rate: NonZero<u32>) -> AppResult<rodio::MixerDeviceSink> {
    rodio::DeviceSinkBuilder::from_default_device()
        .and_then(|x| x.with_sample_rate(sample_rate).open_stream())
        .or_else(|original_err| {
            let mut devices = rodio::cpal::default_host().output_devices()?;

            Ok(devices
                .find_map(|d| {
                    rodio::DeviceSinkBuilder::from_device(d)
                        .and_then(|x| x.with_sample_rate(sample_rate).open_sink_or_fallback())
                        .ok()
                })
                .ok_or(original_err)?)
        })
}

fn open_preferred_stream(
    sample_rate: NonZero<u32>,
    preferred_device_name: &str,
) -> AppResult<rodio::MixerDeviceSink> {
    let devices = rodio::cpal::default_host().output_devices()?;

    for device in devices {
        if device.id().map(|x| x.1).ok().as_deref() == Some(preferred_device_name) {
            let Ok(stream) = rodio::DeviceSinkBuilder::from_device(device)
                .and_then(|x| x.with_sample_rate(sample_rate).open_sink_or_fallback())
            else {
                break;
            };

            return Ok(stream);
        }
    }

    let devices = rodio::cpal::default_host().output_devices()?;
    let available_devices: Vec<String> = devices.flat_map(|x| x.id().map(|x| x.1)).collect();
    let available_devices = available_devices.join(", ");

    Err(Error::SinkDeviceError {
        message: format!("Unable to find device. Available devices: {available_devices}"),
    })
}

pub enum QueryTrackResult {
    Queued,
    RecreateStreamRequired,
}

impl Drop for Sink {
    fn drop(&mut self) {
        self.clear().unwrap();
    }
}
