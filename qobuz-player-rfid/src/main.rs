use qobuz_player_rfid::RfidState;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::broadcast;
use tokio_schedule::{Job, every};

use clap::Parser;
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

    #[clap(long)]
    /// Delay playback when changing state from paused to playing in milliseconds
    state_change_delay_ms: Option<u64>,

    #[clap(long)]
    /// Delay playback when changing sample rate in milliseconds
    sample_rate_change_delay_ms: Option<u64>,

    #[clap(long)]
    /// Use other qobuz-player with web for rfid database
    rfid_server_base_address: Option<String>,

    #[clap(long)]
    /// Secret for optional qobuz-player rfid server
    rfid_server_secret: Option<String>,

    #[cfg(feature = "gpio")]
    #[clap(long, default_value_t = false)]
    /// Enable gpio interface for raspberry pi. Pin 16 (gpio-23) will be high when playing
    gpio: bool,

    #[clap(long)]
    /// Cache audio files in directory [default: Temporary directory]
    audio_cache: Option<PathBuf>,

    #[clap(long, default_value_t = 1)]
    /// Hours before audio cache is cleaned. 0 for disable
    audio_cache_time_to_live: u32,

    #[clap(long, default_value_t = false)]
    /// Enable qobuz connect (experimental)
    connect: bool,

    #[clap(long, default_value_t = String::from("qobuz-player"))]
    /// Set qobuz connect device name
    connect_name: String,

    #[clap(long)]
    /// Authenticate with Qobuz via browser
    login: bool,

    #[clap(long)]
    /// Logout
    logout: bool,

    #[clap(long)]
    /// Set max audio quality. Persisted
    #[clap(value_enum)]
    set_max_audio_quality: Option<AudioQuality>,
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

    if args.logout {
        database.clear_user_auth_token().await?;
        println!("Logout successful!");
        return Ok(());
    }

    if args.login {
        let (_client, token) = Client::new_with_oauth_login(AudioQuality::Mp3).await?;

        database.set_user_auth_token(token).await?;
        println!("Login successful! You can now run qobuz-player.");
        return Ok(());
    }

    if let Some(quality) = args.set_max_audio_quality {
        database.set_max_audio_quality(quality).await?;

        println!("Max audio quality saved.");
        return Ok(());
    }

    let database_credentials = database.get_credentials().await?;
    let database_configuration = database.get_configuration().await?;
    let tracklist = database.get_tracklist().await.unwrap_or_default();
    let volume = database.get_volume().await.unwrap_or(1.0);

    let (_, exit_receiver) = broadcast::channel(5);

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

    let state_change_delay = args.state_change_delay_ms.map(Duration::from_millis);
    let sample_rate_change_delay = args.sample_rate_change_delay_ms.map(Duration::from_millis);

    let mut player = Player::new(
        tracklist,
        client.clone(),
        volume,
        broadcast.clone(),
        audio_cache,
        database.clone(),
        state_change_delay,
        sample_rate_change_delay,
        args.output_device_id,
    )?;

    #[cfg(feature = "gpio")]
    if args.gpio {
        let status_receiver = player.status();
        tokio::spawn(async move {
            if let Err(e) = qobuz_player_gpio::init(status_receiver).await {
                error_exit(e.into());
            }
        });
    }

    {
        let rfid_state = RfidState::default();
        let tracklist_receiver = player.tracklist();
        let controls = player.controls();
        let database = database.clone();

        tokio::spawn(async move {
            if let Err(e) = qobuz_player_rfid::init(
                rfid_state,
                tracklist_receiver,
                controls,
                database,
                broadcast,
                args.rfid_server_base_address,
                args.rfid_server_secret,
            )
            .await
            {
                error_exit(e);
            }
        });
    }

    if args.connect {
        let app_id = client.app_id().await?;
        let position_receiver = player.position();
        let tracklist_receiver = player.tracklist();
        let volume_receiver = player.volume();
        let status_receiver = player.status();
        let controls = player.controls();

        tokio::spawn(async move {
            if let Err(e) = qobuz_player_connect::init(
                &app_id,
                args.connect_name,
                controls,
                position_receiver,
                tracklist_receiver,
                status_receiver,
                volume_receiver,
                max_audio_quality,
            )
            .await
            {
                error_exit(e);
            }
        });
    }

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
