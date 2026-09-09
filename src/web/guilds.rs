use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serenity::model::{channel::ChannelType, id::GuildId};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::web::{error::ApiError, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/guilds", get(list_guilds))
        .route("/guilds/{guild_id}/channels", get(list_channels))
        .route("/guilds/{guild_id}/roles", get(list_roles))
        .route("/anon/settings/{guild_id}", get(get_anon_settings).put(put_anon_settings))
}

fn now_ts() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

fn parse_guild_id(raw: &str) -> Result<GuildId, ApiError> {
    raw.parse::<u64>().map(GuildId::new).map_err(|_| ApiError::bad_request("Некорректный guild_id"))
}

// ---------------------------------------------------------------------------
// GET /api/guilds — список серверов, на которых установлен бот
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct GuildInfo {
    id: String,
    name: String,
}

async fn list_guilds(State(state): State<AppState>) -> Json<Vec<GuildInfo>> {
    let mut guilds = Vec::new();
    for guild_id in state.cache.guilds() {
        if let Some(guild) = state.cache.guild(guild_id) {
            guilds.push(GuildInfo { id: guild_id.to_string(), name: guild.name.clone() });
        }
    }
    guilds.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Json(guilds)
}

// ---------------------------------------------------------------------------
// GET /api/guilds/{id}/channels — текстовые каналы сервера
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ChannelInfo {
    id: String,
    name: String,
}

async fn list_channels(State(state): State<AppState>, Path(guild_id): Path<String>) -> Result<Json<Vec<ChannelInfo>>, ApiError> {
    let guild_id = parse_guild_id(&guild_id)?;
    let guild = state
        .cache
        .guild(guild_id)
        .ok_or_else(|| ApiError::not_found("Сервер не найден (бот на нём не установлен или ещё не синхронизировал кэш)"))?;

    let mut channels: Vec<ChannelInfo> = guild
        .channels
        .values()
        .filter(|c| c.kind == ChannelType::Text)
        .map(|c| ChannelInfo { id: c.id.to_string(), name: c.name.clone() })
        .collect();
    channels.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(Json(channels))
}

// ---------------------------------------------------------------------------
// GET /api/guilds/{id}/roles — роли сервера (без managed-ролей интеграций)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct RoleInfo {
    id: String,
    name: String,
}

async fn list_roles(State(state): State<AppState>, Path(guild_id): Path<String>) -> Result<Json<Vec<RoleInfo>>, ApiError> {
    let guild_id = parse_guild_id(&guild_id)?;
    let guild = state
        .cache
        .guild(guild_id)
        .ok_or_else(|| ApiError::not_found("Сервер не найден (бот на нём не установлен или ещё не синхронизировал кэш)"))?;

    let mut roles: Vec<RoleInfo> = guild
        .roles
        .values()
        .filter(|r| !r.managed && r.name != "@everyone")
        .map(|r| RoleInfo { id: r.id.to_string(), name: r.name.clone() })
        .collect();
    roles.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(Json(roles))
}

// ---------------------------------------------------------------------------
// GET/PUT /api/anon/settings/{guild_id}
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct AnonSettingsResponse {
    guild_id: String,
    enabled: bool,
    channel_id: Option<String>,
    allowed_role_ids: Vec<String>,
    cooldown_seconds: i64,
}

async fn get_anon_settings(
    State(state): State<AppState>,
    Path(guild_id): Path<String>,
) -> Result<Json<AnonSettingsResponse>, ApiError> {
    let row: Option<(i64, Option<String>, String, i64)> = sqlx::query_as(
        "SELECT enabled, channel_id, allowed_role_ids, cooldown_seconds FROM anon_settings WHERE guild_id = ?",
    )
    .bind(&guild_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::internal(format!("Ошибка чтения настроек: {e}")))?;

    let (enabled, channel_id, allowed_role_ids_json, cooldown_seconds) =
        row.unwrap_or((1, None, "[]".to_string(), 30));
    let allowed_role_ids: Vec<String> = serde_json::from_str(&allowed_role_ids_json).unwrap_or_default();

    Ok(Json(AnonSettingsResponse {
        guild_id,
        enabled: enabled != 0,
        channel_id,
        allowed_role_ids,
        cooldown_seconds,
    }))
}

#[derive(Deserialize)]
struct AnonSettingsInput {
    enabled: bool,
    channel_id: Option<String>,
    allowed_role_ids: Vec<String>,
    cooldown_seconds: i64,
}

async fn put_anon_settings(
    State(state): State<AppState>,
    Path(guild_id): Path<String>,
    Json(input): Json<AnonSettingsInput>,
) -> Result<Json<AnonSettingsResponse>, ApiError> {
    // guild_id должен быть реальным сервером, а не произвольной строкой
    parse_guild_id(&guild_id)?;

    if !(0..=86400).contains(&input.cooldown_seconds) {
        return Err(ApiError::bad_request("Кулдаун должен быть от 0 до 86400 секунд"));
    }

    let roles_json = serde_json::to_string(&input.allowed_role_ids).unwrap_or_else(|_| "[]".to_string());

    sqlx::query(
        "INSERT INTO anon_settings (guild_id, enabled, channel_id, allowed_role_ids, cooldown_seconds, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT(guild_id) DO UPDATE SET \
           enabled = excluded.enabled, \
           channel_id = excluded.channel_id, \
           allowed_role_ids = excluded.allowed_role_ids, \
           cooldown_seconds = excluded.cooldown_seconds, \
           updated_at = excluded.updated_at",
    )
    .bind(&guild_id)
    .bind(input.enabled as i64)
    .bind(&input.channel_id)
    .bind(&roles_json)
    .bind(input.cooldown_seconds)
    .bind(now_ts())
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::internal(format!("Не удалось сохранить настройки: {e}")))?;

    Ok(Json(AnonSettingsResponse {
        guild_id,
        enabled: input.enabled,
        channel_id: input.channel_id,
        allowed_role_ids: input.allowed_role_ids,
        cooldown_seconds: input.cooldown_seconds,
    }))
}