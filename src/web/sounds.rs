use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::path::{Path as StdPath, PathBuf};
use tokio::fs;

use crate::web::{error::ApiError, AppState};

const ALLOWED_EXTENSIONS: &[&str] = &["mp3", "ogg", "wav", "flac"];
const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024; // 50 МБ

/// Маршруты `/api/sounds*` — список, загрузка, удаление. Защищены той же
/// middleware `require_session`, что и остальные `/api/*` (навешивается в
/// `web::build_router`).
pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/sounds", get(list_sounds).post(upload_sound))
        .route("/sounds/{name}", delete(delete_sound))
        // Тело запроса для JSON-ручек крошечное, а сюда прилетают целые
        // аудиофайлы — поднимаем лимит только для этого роутера.
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES))
}

/// Отдельный роутер верхнего уровня `/sounds/{name}` — сами байты файла для
/// `<audio src="...">` в браузере. Не под `/api`, но защищён той же сессией
/// (браузер сам приложит cookie к запросу тега `<audio>`, т.к. это
/// same-origin запрос, а кука с SameSite=Lax это разрешает).
pub fn file_router() -> Router<AppState> {
    Router::new().route("/sounds/{name}", get(serve_sound_file))
}

async fn resolve_sounds_dir(state: &AppState) -> PathBuf {
    let sounds_dir = state.config.read().await.sounds_dir.clone();
    state.base_dir.join(sounds_dir)
}

/// Разрешает только голое имя файла — без слэшей/бэкслэшей и без "..", чтобы
/// нельзя было выйти за пределы папки sounds/ (path traversal).
fn sanitize_filename(name: &str) -> Result<String, ApiError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ApiError::bad_request("Имя файла не может быть пустым"));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        return Err(ApiError::bad_request("Некорректное имя файла"));
    }
    Ok(trimmed.to_string())
}

fn has_allowed_extension(name: &str) -> bool {
    StdPath::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|ext| ALLOWED_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn content_type_for(name: &str) -> &'static str {
    match StdPath::new(name).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase().as_str() {
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        _ => "application/octet-stream",
    }
}

// ---------------------------------------------------------------------------
// GET /api/sounds
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct SoundInfo {
    name: String,
    size_bytes: u64,
    modified_at: String,
}

async fn list_sounds(State(state): State<AppState>) -> Result<Json<Vec<SoundInfo>>, ApiError> {
    let dir = resolve_sounds_dir(&state).await;

    let mut entries = match fs::read_dir(&dir).await {
        Ok(e) => e,
        // Папка sounds/ ещё не создавалась (ни одного файла не загружали) —
        // это не ошибка, просто пустой список.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Json(Vec::new())),
        Err(e) => return Err(ApiError::internal(format!("Не удалось прочитать папку sounds: {e}"))),
    };

    let mut sounds = Vec::new();
    loop {
        let entry = match entries.next_entry().await {
            Ok(Some(e)) => e,
            Ok(None) => break,
            Err(e) => return Err(ApiError::internal(format!("Ошибка чтения sounds: {e}"))),
        };

        let name = entry.file_name().to_string_lossy().to_string();
        if !has_allowed_extension(&name) {
            continue;
        }

        let metadata = match entry.metadata().await {
            Ok(m) if m.is_file() => m,
            _ => continue,
        };

        let modified_at = metadata
            .modified()
            .map(|t| DateTime::<Utc>::from(t).to_rfc3339())
            .unwrap_or_default();

        sounds.push(SoundInfo { name, size_bytes: metadata.len(), modified_at });
    }

    sounds.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(sounds))
}

// ---------------------------------------------------------------------------
// POST /api/sounds (multipart/form-data, поле "file")
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct UploadedSound {
    name: String,
    size_bytes: u64,
}

async fn upload_sound(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<UploadedSound>, ApiError> {
    let dir = resolve_sounds_dir(&state).await;
    fs::create_dir_all(&dir)
        .await
        .map_err(|e| ApiError::internal(format!("Не удалось создать папку sounds: {e}")))?;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(format!("Некорректная загрузка: {e}")))?
    {
        if field.name() != Some("file") {
            continue;
        }

        let original_name = field
            .file_name()
            .map(|s| s.to_string())
            .ok_or_else(|| ApiError::bad_request("Не указано имя файла"))?;
        let filename = sanitize_filename(&original_name)?;

        if !has_allowed_extension(&filename) {
            return Err(ApiError::bad_request(format!(
                "Неподдерживаемый формат. Разрешены: {}",
                ALLOWED_EXTENSIONS.join(", ")
            )));
        }

        let dest_path = dir.join(&filename);
        if dest_path.exists() {
            return Err(ApiError::conflict(format!("Файл «{filename}» уже существует")));
        }

        let bytes = field
            .bytes()
            .await
            .map_err(|e| ApiError::bad_request(format!("Ошибка чтения файла: {e}")))?;

        if bytes.is_empty() {
            return Err(ApiError::bad_request("Файл пустой"));
        }
        if bytes.len() > MAX_UPLOAD_BYTES {
            return Err(ApiError::bad_request(format!(
                "Файл слишком большой (максимум {} МБ)",
                MAX_UPLOAD_BYTES / 1024 / 1024
            )));
        }

        fs::write(&dest_path, &bytes)
            .await
            .map_err(|e| ApiError::internal(format!("Не удалось сохранить файл: {e}")))?;

        return Ok(Json(UploadedSound { name: filename, size_bytes: bytes.len() as u64 }));
    }

    Err(ApiError::bad_request("В запросе не найдено поле file"))
}

// ---------------------------------------------------------------------------
// DELETE /api/sounds/{name}
// ---------------------------------------------------------------------------

async fn delete_sound(State(state): State<AppState>, Path(name): Path<String>) -> Result<StatusCode, ApiError> {
    let filename = sanitize_filename(&name)?;
    let dir = resolve_sounds_dir(&state).await;
    let path = dir.join(&filename);

    if !path.exists() {
        return Err(ApiError::not_found(format!("Файл «{filename}» не найден")));
    }

    fs::remove_file(&path)
        .await
        .map_err(|e| ApiError::internal(format!("Не удалось удалить файл: {e}")))?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// GET /sounds/{name} — сырые байты для <audio>
// ---------------------------------------------------------------------------

async fn serve_sound_file(State(state): State<AppState>, Path(name): Path<String>) -> Result<Response, ApiError> {
    let filename = sanitize_filename(&name)?;
    if !has_allowed_extension(&filename) {
        return Err(ApiError::bad_request("Некорректное имя файла"));
    }

    let dir = resolve_sounds_dir(&state).await;
    let path = dir.join(&filename);

    let bytes = fs::read(&path)
        .await
        .map_err(|_| ApiError::not_found(format!("Файл «{filename}» не найден")))?;

    Ok((
        [(header::CONTENT_TYPE, content_type_for(&filename))],
        bytes,
    )
        .into_response())
}