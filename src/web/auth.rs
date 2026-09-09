use axum::{
    body::Body,
    extract::{Query, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use rand::Rng;
use serde::Deserialize;
use serde_json::json;
use std::time::{Duration as StdDuration, Instant, SystemTime, UNIX_EPOCH};

use crate::{panel_config::AuthMode, password, web::AppState};

const SESSION_COOKIE: &str = "tagvox_session";
const SESSION_TTL_SECS: i64 = 60 * 60 * 24 * 14; // 2 недели
const OAUTH_STATE_TTL: StdDuration = StdDuration::from_secs(600); // 10 минут

/// Данные авторизованного пользователя, которые middleware кладёт в
/// request extensions — обработчики могут достать их через `Extension<AuthedUser>`.
#[derive(Clone)]
pub struct AuthedUser {
    pub discord_user_id: String,
}

fn now_ts() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

fn random_token(len: usize) -> String {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

/// Читает одну куку из заголовка Cookie вручную (без внешних крейтов).
fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        let part = part.trim();
        if let Some((k, v)) = part.split_once('=') {
            if k == name {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn redirect_response(location: &str, set_cookie: Option<String>) -> Response {
    let mut builder = Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, location);
    if let Some(cookie) = set_cookie {
        builder = builder.header(header::SET_COOKIE, cookie);
    }
    builder.body(Body::empty()).unwrap()
}

fn error_json(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

/// Создаёт запись в таблице sessions и возвращает готовое значение заголовка
/// Set-Cookie. Используется и Discord-колбэком, и логином по паролю.
async fn create_session_cookie(state: &AppState, user_id: &str) -> Result<String, Response> {
    let session_id = random_token(48);
    let created_at = now_ts();
    let expires_at = created_at + SESSION_TTL_SECS;

    if let Err(e) = sqlx::query(
        "INSERT INTO sessions (id, discord_user_id, created_at, expires_at) VALUES (?, ?, ?, ?)",
    )
    .bind(session_id.as_str())
    .bind(user_id)
    .bind(created_at)
    .bind(expires_at)
    .execute(&state.db)
    .await
    {
        tracing::error!("Не удалось сохранить сессию: {e:?}");
        return Err(error_json(StatusCode::INTERNAL_SERVER_ERROR, "Не удалось создать сессию"));
    }

    Ok(format!(
        "{SESSION_COOKIE}={session_id}; Path=/; Max-Age={SESSION_TTL_SECS}; HttpOnly; SameSite=Lax"
    ))
}

/// GET /api/auth/mode — сообщает фронтенду, какой экран логина показывать.
#[derive(serde::Serialize)]
pub struct AuthModeResponse {
    mode: &'static str,
}

pub async fn mode(State(state): State<AppState>) -> Json<AuthModeResponse> {
    let mode = match state.auth_mode {
        AuthMode::Discord => "discord",
        AuthMode::Password => "password",
    };
    Json(AuthModeResponse { mode })
}

#[derive(Deserialize)]
pub struct PasswordLoginInput {
    password: String,
}

/// POST /api/auth/password/login — вход по общему паролю (режим "password").
/// В отличие от Discord-флоу, это обычный fetch-запрос, а не переход
/// браузера, поэтому в ответ идёт просто 204 + кука, без редиректа.
pub async fn password_login(State(state): State<AppState>, Json(input): Json<PasswordLoginInput>) -> Response {
    if state.auth_mode != AuthMode::Password {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "Вход по паролю не включён (auth_mode в panel.toml)");
    }
    if state.password_hash.is_empty() {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "Пароль ещё не задан в panel.toml");
    }
    if !password::verify_password(&input.password, &state.password_hash) {
        return error_json(StatusCode::UNAUTHORIZED, "Неверный пароль");
    }

    let cookie = match create_session_cookie(&state, "local-password-user").await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header(header::SET_COOKIE, cookie)
        .body(Body::empty())
        .unwrap()
}

/// GET /api/auth/discord/login — редиректит на страницу авторизации Discord.
pub async fn login(State(state): State<AppState>) -> Response {
    if state.oauth.client_id.is_empty() || state.oauth.redirect_uri.is_empty() {
        return error_json(
            StatusCode::SERVICE_UNAVAILABLE,
            "Discord OAuth не настроен: заполни discord_oauth в panel.toml",
        );
    }

    let oauth_state = random_token(32);
    {
        let mut states = state.oauth_states.lock().await;
        states.retain(|_, ts: &mut Instant| ts.elapsed() < OAUTH_STATE_TTL);
        states.insert(oauth_state.clone(), Instant::now());
    }

    let url = format!(
        "https://discord.com/api/oauth2/authorize?client_id={}&redirect_uri={}&response_type=code&scope=identify&state={}",
        urlencoding::encode(&state.oauth.client_id),
        urlencoding::encode(&state.oauth.redirect_uri),
        urlencoding::encode(&oauth_state),
    );

    redirect_response(&url, None)
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct DiscordTokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct DiscordUser {
    id: String,
    username: String,
}

/// GET /api/auth/discord/callback — обмен кода на токен, проверка допуска
/// (allow-list в таблице admins), создание сессии.
pub async fn callback(State(state): State<AppState>, Query(q): Query<CallbackQuery>) -> Response {
    if let Some(err) = q.error {
        return redirect_response(&format!("/?login_error={}", urlencoding::encode(&err)), None);
    }

    let (Some(code), Some(returned_state)) = (q.code, q.state) else {
        return error_json(StatusCode::BAD_REQUEST, "Отсутствует code или state в запросе от Discord");
    };

    // Проверяем и одноразово сжигаем anti-CSRF state
    {
        let mut states = state.oauth_states.lock().await;
        states.retain(|_, ts: &mut Instant| ts.elapsed() < OAUTH_STATE_TTL);
        if states.remove(&returned_state).is_none() {
            return error_json(
                StatusCode::BAD_REQUEST,
                "Неизвестный или истёкший state — попробуй войти заново",
            );
        }
    }

    let http = reqwest::Client::new();

    let token_resp = http
        .post("https://discord.com/api/oauth2/token")
        .form(&[
            ("client_id", state.oauth.client_id.as_str()),
            ("client_secret", state.oauth.client_secret.as_str()),
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", state.oauth.redirect_uri.as_str()),
        ])
        .send()
        .await;

    let token_resp = match token_resp {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            tracing::error!("Discord token exchange failed: {status} {body}");
            return error_json(StatusCode::BAD_GATEWAY, "Discord отклонил обмен кода на токен");
        }
        Err(e) => {
            tracing::error!("Discord token exchange request error: {e:?}");
            return error_json(StatusCode::BAD_GATEWAY, "Не удалось связаться с Discord");
        }
    };

    let token_data: DiscordTokenResponse = match token_resp.json().await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Discord token response parse error: {e:?}");
            return error_json(StatusCode::BAD_GATEWAY, "Discord вернул неожиданный ответ");
        }
    };

    let user_resp = http
        .get("https://discord.com/api/users/@me")
        .bearer_auth(&token_data.access_token)
        .send()
        .await;

    let discord_user: DiscordUser = match user_resp {
        Ok(r) if r.status().is_success() => match r.json().await {
            Ok(u) => u,
            Err(e) => {
                tracing::error!("Discord user response parse error: {e:?}");
                return error_json(
                    StatusCode::BAD_GATEWAY,
                    "Discord вернул неожиданный ответ о пользователе",
                );
            }
        },
        Ok(r) => {
            tracing::error!("Discord /users/@me failed: {}", r.status());
            return error_json(StatusCode::BAD_GATEWAY, "Не удалось получить данные пользователя Discord");
        }
        Err(e) => {
            tracing::error!("Discord /users/@me request error: {e:?}");
            return error_json(StatusCode::BAD_GATEWAY, "Не удалось связаться с Discord");
        }
    };

    // Проверяем допуск: пользователь уже в admins, либо это самый первый
    // вход и его id совпадает с initial_admin_discord_id из panel.toml
    let already_admin: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM admins WHERE discord_user_id = ?")
        .bind(discord_user.id.as_str())
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    if already_admin == 0 {
        let admins_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM admins")
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

        let is_bootstrap = admins_count == 0
            && !state.initial_admin_discord_id.is_empty()
            && *state.initial_admin_discord_id == discord_user.id;

        if is_bootstrap {
            let _ = sqlx::query(
                "INSERT INTO admins (discord_user_id, display_name, added_at) VALUES (?, ?, ?)",
            )
            .bind(discord_user.id.as_str())
            .bind(discord_user.username.as_str())
            .bind(now_ts())
            .execute(&state.db)
            .await;
            tracing::info!("Первый администратор панели: {} ({})", discord_user.username, discord_user.id);
        } else {
            return error_json(StatusCode::FORBIDDEN, "Этому Discord-аккаунту не разрешён вход в панель");
        }
    }

    // Создаём сессию
    let cookie = match create_session_cookie(&state, &discord_user.id).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    redirect_response("/", Some(cookie))
}

/// POST /api/auth/logout
pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(session_id) = read_cookie(&headers, SESSION_COOKIE) {
        let _ = sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(session_id.as_str())
            .execute(&state.db)
            .await;
    }
    let clear_cookie = format!("{SESSION_COOKIE}=; Path=/; Max-Age=0");
    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header(header::SET_COOKIE, clear_cookie)
        .body(Body::empty())
        .unwrap()
}

/// Middleware: требует валидную, не истёкшую сессию для защищённых
/// `/api/*` маршрутов. При успехе кладёт `AuthedUser` в extensions запроса.
pub async fn require_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Response {
    let Some(session_id) = read_cookie(&headers, SESSION_COOKIE) else {
        return error_json(StatusCode::UNAUTHORIZED, "Не авторизован");
    };

    let row: Option<(String, i64)> =
        sqlx::query_as("SELECT discord_user_id, expires_at FROM sessions WHERE id = ?")
            .bind(session_id.as_str())
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);

    match row {
        Some((discord_user_id, expires_at)) if expires_at > now_ts() => {
            req.extensions_mut().insert(AuthedUser { discord_user_id });
            next.run(req).await
        }
        _ => error_json(StatusCode::UNAUTHORIZED, "Сессия истекла или недействительна"),
    }
}