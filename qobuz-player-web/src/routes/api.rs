use std::{sync::Arc, time::Duration};

use axum::{
    Form, Router,
    extract::{Path, State},
    response::IntoResponse,
    routing::{post, put},
};
use qobuz_player_controls::notification::Notification;
use serde::Deserialize;

use crate::{AppState, ResponseResult, ok_or_broadcast};

pub(crate) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/play", put(play))
        .route("/api/pause", put(pause))
        .route("/api/previous", put(previous))
        .route("/api/next", put(next))
        .route("/api/volume", post(set_volume))
        .route("/api/position", post(set_position))
        .route("/api/skip-to/{track_number}", put(skip_to))
        .route("/api/play-track/{track_id}", put(play_track))
        .route("/api/track/favorite", put(track_favorite))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FavoriteTrackAction {
    AddFavorite,
    RemoveFavorite,
    AddToQueue,
    RemoveFromQueue,
    PlayNext,
}
#[derive(Deserialize)]
struct FavoriteTrackParammeters {
    track_id: u32,
    action: FavoriteTrackAction,
    queue_index: u32,
}
async fn track_favorite(
    State(state): State<Arc<AppState>>,
    Form(req): Form<FavoriteTrackParammeters>,
) -> ResponseResult {
    match req.action {
        FavoriteTrackAction::AddFavorite => {
            ok_or_broadcast(
                &state.broadcast,
                state.client.add_favorite_track(req.track_id).await,
            )?;
            state.send_sse("tracklist".into(), "New favorite track".into());
            Ok(state.send_toast(Notification::Info("Track added to favorites".into())))
        }
        FavoriteTrackAction::RemoveFavorite => {
            ok_or_broadcast(
                &state.broadcast,
                state.client.remove_favorite_track(req.track_id).await,
            )?;
            state.send_sse("tracklist".into(), "Removed favorite track".into());
            Ok(state.send_toast(Notification::Info("Track removed from favorites".into())))
        }
        FavoriteTrackAction::AddToQueue => {
            state.controls.add_track_to_queue(req.track_id);
            state.send_sse("tracklist".into(), "Track added to queue".into());
            Ok(state.send_toast(Notification::Info("Track added to queue".into())))
        }
        FavoriteTrackAction::RemoveFromQueue => {
            state.controls.remove_index_from_queue(req.queue_index);
            state.send_sse("tracklist".into(), "Track removed from queue".into());
            Ok(state.send_toast(Notification::Info("Track removed from queue".into())))
        }
        FavoriteTrackAction::PlayNext => {
            state.controls.play_track_next(req.track_id);
            state.send_sse("tracklist".into(), "Track queued next".into());
            Ok(state.send_toast(Notification::Info("Track queued next".into())))
        }
    }
}

async fn play_track(
    State(state): State<Arc<AppState>>,
    Path(track_id): Path<u32>,
) -> impl IntoResponse {
    state.controls.play_track(track_id);
}

async fn play(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state.controls.play();
}

async fn pause(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state.controls.pause();
}

async fn previous(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state.controls.previous();
}

async fn next(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state.controls.next();
}

async fn skip_to(
    State(state): State<Arc<AppState>>,
    Path(track_number): Path<u32>,
) -> impl IntoResponse {
    state.controls.skip_to_position(track_number, true);
}

#[derive(serde::Deserialize, Clone, Copy)]
struct SliderParameters {
    value: i32,
}
async fn set_volume(
    State(state): State<Arc<AppState>>,
    axum::Form(parameters): axum::Form<SliderParameters>,
) -> impl IntoResponse {
    let mut volume = parameters.value;

    if volume < 0 {
        volume = 0;
    };

    if volume > 100 {
        volume = 100;
    };

    let formatted_volume = volume as f32 / 100.0;

    state.controls.set_volume(formatted_volume);
}

async fn set_position(
    State(state): State<Arc<AppState>>,
    axum::Form(parameters): axum::Form<SliderParameters>,
) -> impl IntoResponse {
    let time = Duration::from_millis(parameters.value as u64);
    state.controls.seek(time);
}
