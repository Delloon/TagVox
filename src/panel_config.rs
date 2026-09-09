use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path::Path,
};

/// Способ входа в веб-панель.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    /// Вход через Discord OAuth. Требует настроенный redirect URI, который
    /// должен быть доступен из браузера того, кто входит в панель — если
    /// панель открывают не с той же машины, где крутится бот, `127.0.0.1`
    /// не подойдёт (нужен домен, LAN-адрес или туннель).
    Discord,
    /// Вход по общему паролю. Проще для локального/домашнего использования,
    /// не требует настройки OAuth-приложения и redirect URI вовсе.
    Password,
}

impl Default for AuthMode {
    fn default() -> Self {
        AuthMode::Discord
    }
}

/// Настройки веб-панели. Хранятся отдельно от config.json бота, потому что
/// это принципиально новая, независимая подсистема — панель может быть не
/// включена вовсе, и тогда этот файл никого не касается.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(default)]
    pub auth_mode: AuthMode,

    #[serde(default)]
    pub discord_oauth: DiscordOauthConfig,

    /// Хеш пароля (Argon2), используется только если auth_mode = "password".
    /// Сам пароль нигде не хранится, только необратимый хеш.
    #[serde(default)]
    pub password_hash: String,

    /// Ключ для подписи сессионных кук. Генерируется случайно при первом
    /// запуске — трогать руками не нужно.
    #[serde(default)]
    pub session_secret: String,

    /// Discord ID пользователя, которого нужно один раз добавить в admins
    /// при первом запуске (только для auth_mode = "discord"; чтобы было
    /// кому вообще войти в панель). После первого входа можно оставить
    /// пустым — админы уже в базе.
    #[serde(default)]
    pub initial_admin_discord_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiscordOauthConfig {
    /// Client ID того же Discord-приложения, что и у бота (Developer Portal
    /// -> General Information -> Application ID).
    #[serde(default)]
    pub client_id: String,

    /// Client Secret из Developer Portal -> OAuth2 -> Client Secret. Держать
    /// в секрете так же, как токен бота.
    #[serde(default)]
    pub client_secret: String,

    /// Должен точно совпадать со значением, добавленным в Developer Portal
    /// -> OAuth2 -> Redirects, например: http://localhost:8787/api/auth/discord/callback
    #[serde(default)]
    pub redirect_uri: String,
}

fn default_true() -> bool {
    true
}
fn default_host() -> String {
    "127.0.0.1".to_string()
}
fn default_port() -> u16 {
    8787
}

impl Default for PanelConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            host: default_host(),
            port: default_port(),
            auth_mode: AuthMode::default(),
            discord_oauth: DiscordOauthConfig::default(),
            password_hash: String::new(),
            session_secret: generate_secret(),
            initial_admin_discord_id: String::new(),
        }
    }
}

fn generate_secret() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..48)
        .map(|_| {
            let charset = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
            charset[rng.gen_range(0..charset.len())] as char
        })
        .collect()
}

fn prompt_line(question: &str) -> String {
    print!("{question}");
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    input.trim().to_string()
}

/// Загружает panel.toml, либо создаёт его при первом запуске. При создании
/// один раз в консоли спрашивает способ входа — так же, как token_store
/// один раз спрашивает токен бота.
pub fn load_or_create(path: &str) -> PanelConfig {
    if Path::new(path).exists() {
        let data = fs::read_to_string(path).expect("не удалось прочитать panel.toml");
        let mut cfg: PanelConfig = toml::from_str(&data).expect("panel.toml повреждён или содержит ошибку формата");
        // Если файл существует, но secret почему-то пуст (например, отредактирован руками) — досоздаём
        if cfg.session_secret.trim().is_empty() {
            cfg.session_secret = generate_secret();
            save(path, &cfg);
        }
        cfg
    } else {
        let mut cfg = PanelConfig::default();

        println!();
        println!("=== Настройка веб-панели (первый запуск) ===");
        println!("Как настроить вход в панель?");
        println!("  1) Через Discord OAuth — нужен свой redirect URI, доступный из");
        println!("     браузера того, кто входит (127.0.0.1 подходит, только если панель");
        println!("     открывают с того же компьютера, где работает бот).");
        println!("  2) По паролю — проще для локального/домашнего использования,");
        println!("     OAuth-приложение настраивать не нужно.");
        let choice = prompt_line("Выбери 1 или 2 (по умолчанию 1): ");

        if choice.trim() == "2" {
            cfg.auth_mode = AuthMode::Password;
            let password = loop {
                print!("Придумай пароль для входа в панель (ввод скрыт): ");
                io::stdout().flush().ok();
                let p1 = rpassword::read_password().unwrap_or_default();
                if p1.trim().is_empty() {
                    println!("Пароль не может быть пустым, попробуй ещё раз.");
                    continue;
                }
                print!("Повтори пароль: ");
                io::stdout().flush().ok();
                let p2 = rpassword::read_password().unwrap_or_default();
                if p1 != p2 {
                    println!("Пароли не совпадают, попробуй ещё раз.");
                    continue;
                }
                break p1;
            };
            cfg.password_hash = crate::password::hash_password(&password);
            println!("Пароль сохранён (в виде хеша, не в открытом виде).");
        } else {
            cfg.auth_mode = AuthMode::Discord;
            println!(
                "Выбран вход через Discord OAuth. Заполни discord_oauth.client_id/client_secret/redirect_uri \
                 и initial_admin_discord_id в panel.toml перед первым входом."
            );
        }

        save(path, &cfg);
        println!("Создан panel.toml. Веб-панель по умолчанию слушает http://{}:{}.", cfg.host, cfg.port);
        println!("Способ входа при необходимости можно сменить в panel.toml вручную (поле auth_mode).");
        println!();

        cfg
    }
}

fn save(path: &str, cfg: &PanelConfig) {
    let data = toml::to_string_pretty(cfg).expect("не удалось сериализовать panel.toml");
    fs::write(path, data).expect("не удалось сохранить panel.toml");
}