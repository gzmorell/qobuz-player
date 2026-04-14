use clap::{Args, Subcommand};
use qobuz_player_controls::{AppResult, AudioQuality, client::Client, database::Database};
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct SharedArgs {
    #[clap(long)]
    pub audio_cache: Option<PathBuf>,

    #[clap(long, default_value_t = 1)]
    pub audio_cache_time_to_live: u32,

    #[clap(short, long)]
    /// Provide max audio quality (overrides any configured value)
    pub max_audio_quality: Option<AudioQuality>,

    #[clap(long)]
    /// Use provided device for audio output, instead of default.
    /// Use qobuz-player list-devices for output device list
    pub output_device_id: Option<String>,
}

#[derive(Args, Debug)]
pub struct ConnectArgs {
    #[clap(long)]
    pub connect: bool,

    #[clap(flatten)]
    pub name_args: ConnectNameArgs,
}

#[derive(Args, Debug)]
pub struct RfidArgs {
    #[clap(long)]
    /// Use other qobuz-player with web for rfid database
    pub rfid_server_base_address: Option<String>,

    #[clap(long)]
    /// Secret for optional qobuz-player rfid server
    pub rfid_server_secret: Option<String>,
}

#[derive(Args, Debug)]
pub struct ConnectNameArgs {
    #[clap(long, default_value = "qobuz-player")]
    pub connect_name: String,
}

#[derive(Args, Debug)]
pub struct GpioArgs {
    #[clap(long, default_value_t = false)]
    /// Enable gpio interface for raspberry pi. Pin 16 (gpio-23) will be high when playing
    pub gpio: bool,
}

#[derive(Args, Debug)]
pub struct DelayArgs {
    #[clap(long)]
    /// Delay playback when changing state from paused to playing in milliseconds
    pub state_change_delay_ms: Option<u64>,

    #[clap(long)]
    /// Delay playback when changing sample rate in milliseconds
    pub sample_rate_change_delay_ms: Option<u64>,
}

#[derive(Subcommand, Debug)]
pub enum SharedCommands {
    /// Authenticate with Qobuz via browser
    Login,

    /// Logout from Qobuz
    Logout,

    /// Persistently set the maximum audio quality
    SetMaxAudioQuality {
        #[clap(value_enum)]
        quality: AudioQuality,
    },
}

pub async fn handle_shared_commands(command: SharedCommands, database: &Database) -> AppResult<()> {
    match command {
        SharedCommands::Login => {
            let (_client, token) = Client::new_with_oauth_login(AudioQuality::Mp3).await?;

            database.set_user_auth_token(token).await?;
            println!("Login successful! You can now run qobuz-player.");
            Ok(())
        }
        SharedCommands::Logout => {
            database.clear_user_auth_token().await?;
            println!("Logout successful!");
            Ok(())
        }
        SharedCommands::SetMaxAudioQuality { quality } => {
            database.set_max_audio_quality(quality).await?;

            println!("Max audio quality saved.");
            Ok(())
        }
    }
}
