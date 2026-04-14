#[cfg(feature = "gpio")]
use qobuz_player_cli::GpioArgs;
use qobuz_player_cli::{
    ConnectArgs, DelayArgs, RfidArgs, SharedArgs, SharedCommands, handle_shared_commands,
};
use qobuz_player_rfid::RfidState;
use std::{sync::Arc, time::Duration};
use tokio::sync::broadcast;
use tokio_schedule::{Job, every};

use clap::Parser;
use qobuz_player_controls::{
    AppResult, client::Client, database::Database, error::Error,
    notification::NotificationBroadcast, player::Player,
};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Arguments {
    #[clap(long)]
    /// Secret used for web ui auth
    web_secret: Option<String>,

    #[clap(long, default_value_t = 9888)]
    /// Specify port for the web server
    port: u16,

    #[clap(long, default_value_t = false)]
    /// Enable rfid interface
    rfid: bool,

    #[clap(flatten)]
    rfid_config: RfidArgs,

    #[clap(flatten)]
    delay: DelayArgs,

    #[clap(flatten)]
    shared: SharedArgs,

    #[clap(flatten)]
    connect: ConnectArgs,

    #[cfg(feature = "gpio")]
    #[clap(flatten)]
    gpio: GpioArgs,

    #[clap(subcommand)]
    command: Option<SharedCommands>,
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

    if let Some(command) = args.command {
        handle_shared_commands(command, &database).await?;
        return Ok(());
    }

    let database_credentials = database.get_credentials().await?;
    let database_configuration = database.get_configuration().await?;
    let tracklist = database.get_tracklist().await.unwrap_or_default();
    let volume = database.get_volume().await.unwrap_or(1.0);

    let (_, exit_receiver) = broadcast::channel(5);

    let audio_cache = args.shared.audio_cache.unwrap_or_else(|| {
        let mut cache_dir = std::env::temp_dir();
        cache_dir.push("qobuz-player-cache");
        cache_dir
    });

    let max_audio_quality = args.shared.max_audio_quality.unwrap_or_else(|| {
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

    let state_change_delay = args.delay.state_change_delay_ms.map(Duration::from_millis);
    let sample_rate_change_delay = args
        .delay
        .sample_rate_change_delay_ms
        .map(Duration::from_millis);

    let mut player = Player::new(
        tracklist,
        client.clone(),
        volume,
        broadcast.clone(),
        audio_cache,
        database.clone(),
        state_change_delay,
        sample_rate_change_delay,
        args.shared.output_device_id,
    )?;

    let rfid_state = args.rfid.then(RfidState::default);

    {
        let position_receiver = player.position();
        let tracklist_receiver = player.tracklist();
        let volume_receiver = player.volume();
        let status_receiver = player.status();
        let controls = player.controls();
        let broadcast = broadcast.clone();
        let client = client.clone();
        let database = database.clone();
        let rfid_state = rfid_state.clone();

        tokio::spawn(async move {
            if let Err(e) = qobuz_player_web::init(
                controls,
                position_receiver,
                tracklist_receiver,
                volume_receiver,
                status_receiver,
                args.port,
                args.web_secret,
                rfid_state,
                broadcast,
                client,
                database,
            )
            .await
            {
                error_exit(e);
            }
        });
    }

    #[cfg(feature = "gpio")]
    if args.gpio.gpio {
        let status_receiver = player.status();
        tokio::spawn(async move {
            if let Err(e) = qobuz_player_gpio::init(status_receiver).await {
                error_exit(e.into());
            }
        });
    }

    if let Some(rfid_state) = rfid_state {
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
                args.rfid_config.rfid_server_base_address,
                args.rfid_config.rfid_server_secret,
            )
            .await
            {
                error_exit(e);
            }
        });
    }

    if args.connect.enable_connect {
        let app_id = client.app_id().await?;
        let position_receiver = player.position();
        let tracklist_receiver = player.tracklist();
        let volume_receiver = player.volume();
        let status_receiver = player.status();
        let controls = player.controls();

        tokio::spawn(async move {
            if let Err(e) = qobuz_player_connect::init(
                &app_id,
                args.connect.name_args.connect_name,
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

    if args.shared.audio_cache_time_to_live != 0 {
        let clean_up_schedule = every(1).hour().perform(move || {
            let database = database.clone();
            async move {
                if let Ok(deleted_paths) = database
                    .clean_up_cache_entries(time::Duration::hours(
                        args.shared.audio_cache_time_to_live.into(),
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
