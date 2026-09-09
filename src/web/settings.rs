use axum::{
    extract::{Path, State},
    routing::{get, put},
    Json, Router,
};
use serde::Deserialize;

use crate::{
    config::Config,
    phrases::{Phrases, TextReaction, VoiceCommand},
    web::{error::ApiError, AppState},
};

/// Маршруты этого файла защищены middleware `require_session` — он
/// навешивается в `web::build_router`, здесь только сами обработчики.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/config", get(get_config).put(put_config))
        .route("/phrases/reactions", get(list_reactions).post(create_reaction))
        .route(
            "/phrases/reactions/{id}",
            put(update_reaction).delete(delete_reaction),
        )
        .route("/phrases/voice-commands", get(list_voice_commands).post(create_voice_command))
        .route(
            "/phrases/voice-commands/{id}",
            put(update_voice_command).delete(delete_voice_command),
        )
        .route(
            "/phrases/unknown-responses",
            get(get_unknown_responses).put(put_unknown_responses),
        )
}

async fn persist_config(state: &AppState, cfg: &Config) -> Result<(), ApiError> {
    let data = serde_json::to_string_pretty(cfg)
        .map_err(|e| ApiError::internal(format!("Ошибка сериализации config.json: {e}")))?;
    tokio::fs::write(&state.config_path, data)
        .await
        .map_err(|e| ApiError::internal(format!("Не удалось записать config.json: {e}")))?;
    Ok(())
}

async fn persist_phrases(state: &AppState, phrases: &Phrases) -> Result<(), ApiError> {
    let data = serde_json::to_string_pretty(phrases)
        .map_err(|e| ApiError::internal(format!("Ошибка сериализации phrases.json: {e}")))?;
    tokio::fs::write(&state.phrases_path, data)
        .await
        .map_err(|e| ApiError::internal(format!("Не удалось записать phrases.json: {e}")))?;
    Ok(())
}

fn validate_id(id: &str) -> Result<(), ApiError> {
    let id = id.trim();
    if id.is_empty() {
        return Err(ApiError::bad_request("id не может быть пустым"));
    }
    if id.chars().count() > 64 {
        return Err(ApiError::bad_request("id слишком длинный (максимум 64 символа)"));
    }
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(ApiError::bad_request(
            "id может содержать только латинские буквы, цифры, _ и -",
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// config.json
// ---------------------------------------------------------------------------

async fn get_config(State(state): State<AppState>) -> Json<Config> {
    Json(state.config.read().await.clone())
}

async fn put_config(State(state): State<AppState>, Json(payload): Json<Config>) -> Result<Json<Config>, ApiError> {
    if payload.command_prefix.trim().is_empty() {
        return Err(ApiError::bad_request("Префикс команд не может быть пустым"));
    }
    if !(0.0..=1.0).contains(&payload.similarity_threshold) {
        return Err(ApiError::bad_request("Порог похожести должен быть в диапазоне от 0 до 1"));
    }
    if payload.sounds_dir.trim().is_empty() {
        return Err(ApiError::bad_request("Папка со звуками не может быть пустой"));
    }

    {
        let mut cfg = state.config.write().await;
        *cfg = payload.clone();
    }
    persist_config(&state, &payload).await?;

    Ok(Json(payload))
}

// ---------------------------------------------------------------------------
// text_reactions
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ReactionCreateInput {
    id: String,
    triggers: Vec<String>,
    responses: Vec<String>,
}

#[derive(Deserialize)]
pub struct ReactionUpdateInput {
    triggers: Vec<String>,
    responses: Vec<String>,
}

fn validate_triggers_responses(triggers: &[String], responses: &[String]) -> Result<(), ApiError> {
    if triggers.iter().all(|t| t.trim().is_empty()) || triggers.is_empty() {
        return Err(ApiError::bad_request("Нужен хотя бы один триггер"));
    }
    if responses.iter().all(|r| r.trim().is_empty()) || responses.is_empty() {
        return Err(ApiError::bad_request("Нужен хотя бы один вариант ответа"));
    }
    Ok(())
}

async fn list_reactions(State(state): State<AppState>) -> Json<Vec<TextReaction>> {
    Json(state.phrases.read().await.text_reactions.clone())
}

async fn create_reaction(
    State(state): State<AppState>,
    Json(input): Json<ReactionCreateInput>,
) -> Result<Json<TextReaction>, ApiError> {
    validate_id(&input.id)?;
    validate_triggers_responses(&input.triggers, &input.responses)?;

    let reaction = TextReaction {
        id: input.id.trim().to_lowercase(),
        triggers: input.triggers,
        responses: input.responses,
    };

    let snapshot = {
        let mut phrases = state.phrases.write().await;
        if phrases.text_reactions.iter().any(|r| r.id == reaction.id) {
            return Err(ApiError::conflict(format!("Реакция с id «{}» уже существует", reaction.id)));
        }
        phrases.text_reactions.push(reaction.clone());
        phrases.clone()
    };

    persist_phrases(&state, &snapshot).await?;
    Ok(Json(reaction))
}

async fn update_reaction(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<ReactionUpdateInput>,
) -> Result<Json<TextReaction>, ApiError> {
    validate_triggers_responses(&input.triggers, &input.responses)?;

    let (updated, snapshot) = {
        let mut phrases = state.phrases.write().await;
        let entry = phrases
            .text_reactions
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or_else(|| ApiError::not_found(format!("Реакция «{id}» не найдена")))?;
        entry.triggers = input.triggers;
        entry.responses = input.responses;
        let updated = entry.clone();
        (updated, phrases.clone())
    };

    persist_phrases(&state, &snapshot).await?;
    Ok(Json(updated))
}

async fn delete_reaction(State(state): State<AppState>, Path(id): Path<String>) -> Result<axum::http::StatusCode, ApiError> {
    let snapshot = {
        let mut phrases = state.phrases.write().await;
        let before = phrases.text_reactions.len();
        phrases.text_reactions.retain(|r| r.id != id);
        if phrases.text_reactions.len() == before {
            return Err(ApiError::not_found(format!("Реакция «{id}» не найдена")));
        }
        phrases.clone()
    };

    persist_phrases(&state, &snapshot).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// voice_commands
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct VoiceCommandCreateInput {
    id: String,
    triggers: Vec<String>,
    file: String,
}

#[derive(Deserialize)]
pub struct VoiceCommandUpdateInput {
    triggers: Vec<String>,
    file: String,
}

async fn list_voice_commands(State(state): State<AppState>) -> Json<Vec<VoiceCommand>> {
    Json(state.phrases.read().await.voice_commands.clone())
}

async fn create_voice_command(
    State(state): State<AppState>,
    Json(input): Json<VoiceCommandCreateInput>,
) -> Result<Json<VoiceCommand>, ApiError> {
    validate_id(&input.id)?;
    if input.triggers.is_empty() {
        return Err(ApiError::bad_request("Нужен хотя бы один триггер"));
    }
    if input.file.trim().is_empty() {
        return Err(ApiError::bad_request("Не указан звуковой файл"));
    }

    let command = VoiceCommand {
        id: input.id.trim().to_lowercase(),
        triggers: input.triggers,
        file: input.file,
    };

    let snapshot = {
        let mut phrases = state.phrases.write().await;
        if phrases.voice_commands.iter().any(|c| c.id == command.id) {
            return Err(ApiError::conflict(format!("Команда с id «{}» уже существует", command.id)));
        }
        phrases.voice_commands.push(command.clone());
        phrases.clone()
    };

    persist_phrases(&state, &snapshot).await?;
    Ok(Json(command))
}

async fn update_voice_command(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<VoiceCommandUpdateInput>,
) -> Result<Json<VoiceCommand>, ApiError> {
    if input.triggers.is_empty() {
        return Err(ApiError::bad_request("Нужен хотя бы один триггер"));
    }
    if input.file.trim().is_empty() {
        return Err(ApiError::bad_request("Не указан звуковой файл"));
    }

    let (updated, snapshot) = {
        let mut phrases = state.phrases.write().await;
        let entry = phrases
            .voice_commands
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or_else(|| ApiError::not_found(format!("Команда «{id}» не найдена")))?;
        entry.triggers = input.triggers;
        entry.file = input.file;
        let updated = entry.clone();
        (updated, phrases.clone())
    };

    persist_phrases(&state, &snapshot).await?;
    Ok(Json(updated))
}

async fn delete_voice_command(State(state): State<AppState>, Path(id): Path<String>) -> Result<axum::http::StatusCode, ApiError> {
    let snapshot = {
        let mut phrases = state.phrases.write().await;
        let before = phrases.voice_commands.len();
        phrases.voice_commands.retain(|c| c.id != id);
        if phrases.voice_commands.len() == before {
            return Err(ApiError::not_found(format!("Команда «{id}» не найдена")));
        }
        phrases.clone()
    };

    persist_phrases(&state, &snapshot).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// unknown_response
// ---------------------------------------------------------------------------

async fn get_unknown_responses(State(state): State<AppState>) -> Json<Vec<String>> {
    Json(state.phrases.read().await.unknown_response.clone())
}

async fn put_unknown_responses(
    State(state): State<AppState>,
    Json(list): Json<Vec<String>>,
) -> Result<Json<Vec<String>>, ApiError> {
    let cleaned: Vec<String> = list.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    if cleaned.is_empty() {
        return Err(ApiError::bad_request("Нужен хотя бы один вариант фразы-заглушки"));
    }

    let snapshot = {
        let mut phrases = state.phrases.write().await;
        phrases.unknown_response = cleaned.clone();
        phrases.clone()
    };

    persist_phrases(&state, &snapshot).await?;
    Ok(Json(cleaned))
}