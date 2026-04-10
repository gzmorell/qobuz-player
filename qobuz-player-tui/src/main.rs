use futures::executor::block_on;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::broadcast;
use tokio_schedule::{Job, every};

use clap::Parser;
#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
use qobuz_player_controls::StatusReceiver;
use qobuz_player_controls::{
    AppResult, AudioQuality, client::Client, database::Database, error::Error,
    notification::NotificationBroadcast, player::Player,
};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Arguments {
    #[clap(short, long)]
    /// Provide max audio quality (overrides any configured value)
    max_audio_quality: Option<AudioQuality>,

    #[clap(long)]
    /// Use provided device for audio output, instead of default.
    /// Use qobuz-player list-devices for output device list
    output_device_id: Option<String>,

    #[clap(long, default_value_t = false)]
    /// Disable the album cover image
    disable_album_cover: bool,

    #[clap(long)]
    /// Cache audio files in directory [default: Temporary directory]
    audio_cache: Option<PathBuf>,

    #[clap(long, default_value_t = 1)]
    /// Hours before audio cache is cleaned. 0 for disable
    audio_cache_time_to_live: u32,
}

#[tokio::main]
async fn main() {
    match run().await {
        Ok(()) => {}
        Err(err) => {
            error_exit(err);
        }
    }
}

pub async fn run() -> AppResult<()> {
    let args = Arguments::parse();

    let database = Arc::new(Database::new().await?);
    let database_credentials = database.get_credentials().await?;
    let database_configuration = database.get_configuration().await?;
    let tracklist = database.get_tracklist().await.unwrap_or_default();
    let volume = database.get_volume().await.unwrap_or(1.0);

    let (exit_sender, exit_receiver) = broadcast::channel(5);

    let audio_cache = args.audio_cache.unwrap_or_else(|| {
        let mut cache_dir = std::env::temp_dir();
        cache_dir.push("qobuz-player-cache");
        cache_dir
    });

    let max_audio_quality = args.max_audio_quality.unwrap_or_else(|| {
        database_configuration
            .max_audio_quality
            .try_into()
            .expect("This should always convert")
    });

    let client = match database_credentials.user_auth_token {
        Some(token) => Arc::new(Client::new(token, max_audio_quality)),
        None => {
            let (client, token) = Client::new_with_oauth_login(max_audio_quality).await?;

            database.set_user_auth_token(token).await?;

            Arc::new(client)
        }
    };

    let broadcast = Arc::new(NotificationBroadcast::new());
    let mut player = Player::new(
        tracklist,
        client.clone(),
        volume,
        broadcast.clone(),
        audio_cache,
        database.clone(),
        None,
        None,
        args.output_device_id,
    )?;

    #[cfg(target_os = "linux")]
    {
        let position_receiver = player.position();
        let tracklist_receiver = player.tracklist();
        let volume_receiver = player.volume();
        let status_receiver = player.status();
        let controls = player.controls();
        let exit_sender = exit_sender.clone();
        tokio::spawn(async move {
            if let Err(e) = qobuz_player_mpris::init(
                position_receiver,
                tracklist_receiver,
                volume_receiver,
                status_receiver,
                controls,
                exit_sender,
            )
            .await
            {
                error_exit(e);
            }
        });
    }

    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    {
        let status_receiver = player.status();
        sleep_inhibitor(status_receiver);
    }

    {
        let position_receiver = player.position();
        let tracklist_receiver = player.tracklist();
        let status_receiver = player.status();
        let controls = player.controls();
        let client = client.clone();
        let broadcast = broadcast.clone();
        tokio::spawn(async move {
            if let Err(e) = qobuz_player_tui::init(
                client,
                broadcast,
                controls,
                position_receiver,
                tracklist_receiver,
                status_receiver,
                exit_sender,
                args.disable_album_cover,
            )
            .await
            {
                error_exit(e);
            };
        });
    };

    if args.audio_cache_time_to_live != 0 {
        let clean_up_schedule = every(1).hour().perform(move || {
            let database = database.clone();
            async move {
                if let Ok(deleted_paths) = database
                    .clean_up_cache_entries(time::Duration::hours(
                        args.audio_cache_time_to_live.into(),
                    ))
                    .await
                {
                    for path in deleted_paths {
                        _ = tokio::fs::remove_file(path.as_path()).await;
                    }
                };
            }
        });

        tokio::spawn(clean_up_schedule);
    }

    player.player_loop(exit_receiver).await?;
    Ok(())
}

fn error_exit(error: Error) {
    eprintln!("{error}");
    std::process::exit(1);
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
pub fn sleep_inhibitor(mut status_receiver: StatusReceiver) {
    std::thread::spawn(move || {
        let mut sleep_inhibitor = SleepInhibitor::new();

        loop {
            use qobuz_player_controls::Status;

            let changed = block_on(async { status_receiver.changed().await });
            if changed.is_err() {
                sleep_inhibitor.restore_sleep();
                break;
            }

            let status = *status_receiver.borrow_and_update();
            match status {
                Status::Paused => sleep_inhibitor.restore_sleep(),
                Status::Playing | Status::Buffering => sleep_inhibitor.block_sleep(),
            }
        }
    });
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
struct SleepInhibitor {
    awake: Option<keepawake::KeepAwake>,
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
impl SleepInhibitor {
    fn new() -> Self {
        Self { awake: None }
    }

    fn block_sleep(&mut self) {
        if self.awake.is_none() {
            let mut builder = keepawake::Builder::default();
            builder
                .idle(true)
                .sleep(true)
                .reason("Audio playback")
                .app_name("qobuz-player");

            if let Ok(awake) = builder.create() {
                self.awake = Some(awake);
            }
        }
    }

    fn restore_sleep(&mut self) {
        let _ = self.awake.take();
    }
}
