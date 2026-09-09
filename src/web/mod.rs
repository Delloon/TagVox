mod auth;
pub mod error;
mod guilds;
mod settings;
mod sounds;

use axum::{
    extract::State,
    middleware,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use serenity::{cache::Cache, gateway::ShardManager, model::id::GuildId};
use sqlx::SqlitePool;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Instant};
use tokio::sync::{Mutex, RwLock};
use tower_http::services::ServeDir;

use crate::{
    config::Config,
    panel_config::{AuthMode, DiscordOauthConfig},
    phrases::Phrases,
    state::GuildPlayback,
};

/// Общее состояние, которое видят и обработчик Discord-сообщений, и
/// веб-панель — это один и тот же процесс, поэтому статус в панели всегда
/// отражает реальное текущее состояние бота, без отдельного API между
/// процессами. `config`/`phrases` — те же `Arc<RwLock<...>>`, что вставлены
/// в TypeMap серенити, так что правки из панели сразу видны боту.
#[derive(Clone)]
pub struct AppState {
    pub shard_manager: Arc<ShardManager>,
    pub playback_states: Arc<Mutex<HashMap<GuildId, GuildPlayback>>>,
    pub cache: Arc<Cache>,
    pub start_time: Instant,
    pub db: SqlitePool,
    pub base_dir: Arc<PathBuf>,
    pub oauth: Arc<DiscordOauthConfig>,
    pub initial_admin_discord_id: Arc<String>,
    pub auth_mode: AuthMode,
    pub password_hash: Arc<String>,
    /// Одноразовые anti-CSRF токены для OAuth-логина: значение -> момент выдачи.
    pub oauth_states: Arc<Mutex<HashMap<String, Instant>>>,
    pub config: Arc<RwLock<Config>>,
    pub phrases: Arc<RwLock<Phrases>>,
    pub config_path: PathBuf,
    pub phrases_path: PathBuf,
}

/// Собирает Axum-роутер:
/// - `/api/auth/*` — публичные маршруты логина/логаута (без сессии)
/// - остальные `/api/*` — защищены middleware, требующим валидную сессию
/// - всё прочее — статика из папки `web/`, которая лежит рядом с exe
pub fn build_router(state: AppState, web_dir: PathBuf) -> Router {
    let auth_routes = Router::new()
        .route("/auth/mode", get(auth::mode))
        .route("/auth/discord/login", get(auth::login))
        .route("/auth/discord/callback", get(auth::callback))
        .route("/auth/password/login", post(auth::password_login))
        .route("/auth/logout", post(auth::logout));

    let protected_routes = Router::new()
        .route("/status", get(status_handler))
        .merge(settings::router())
        .merge(sounds::api_router())
        .merge(guilds::router())
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::require_session));

    let api = Router::new().merge(auth_routes).merge(protected_routes);

    // /sounds/{name} — сырые байты файла для <audio>, вне /api, но за той же
    // сессией (браузер сам приложит cookie к запросу тега <audio>).
    let sounds_file_routes = sounds::file_router()
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::require_session));

    Router::new()
        .nest("/api", api)
        .merge(sounds_file_routes)
        .fallback_service(ServeDir::new(web_dir))
        .with_state(state)
}

#[derive(Serialize)]
struct StatusResponse {
    online: bool,
    ping_ms: Option<u128>,
    guild_count: usize,
    uptime_seconds: u64,
    shard_count: usize,
}

async fn status_handler(State(state): State<AppState>) -> Json<StatusResponse> {
    let runners = state.shard_manager.runners.lock().await;
    let shard_count = runners.len();
    let ping_ms = runners.values().find_map(|r| r.latency).map(|d| d.as_millis());
    let online = ping_ms.is_some();
    let guild_count = state.cache.guilds().len();
    let uptime_seconds = state.start_time.elapsed().as_secs();

    Json(StatusResponse {
        online,
        ping_ms,
        guild_count,
        uptime_seconds,
        shard_count,
    })
}