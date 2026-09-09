mod anon;
mod config;
mod db;
mod handler;
mod panel_config;
mod password;
mod phrases;
mod state;
mod token_store;
mod web;

use serenity::prelude::*;
use songbird::SerenityInit;
use state::{BaseDirKey, ConfigKey, DbKey, PhrasesKey, PlaybackStates, ShardManagerKey};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Instant};
use tokio::sync::{Mutex, RwLock};

/// Папка, где лежит сам исполняемый файл. Если по какой-то причине путь
/// узнать не удалось, используем текущую рабочую директорию как запасной
/// вариант (например, при `cargo run` это и так корень проекта).
fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[tokio::main]
async fn main() {
    // Если переменная окружения RUST_LOG не задана, включаем подробные логи
    // для songbird и serenity — именно в них видна настоящая причина, почему
    // не получается подключиться к голосовому каналу (нет прав, таймаут UDP,
    // разрыв соединения и т.д.). Задать свой уровень можно так:
    //   PowerShell: $env:RUST_LOG="debug"; cargo run --release
    //   CMD:        set RUST_LOG=debug && cargo run --release
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("discord_bot=debug,songbird=debug,serenity=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    println!("=== Discord бот: реакции по тегу + голосовые команды ===");

    let base_dir = Arc::new(exe_dir());
    println!("Рабочая папка (рядом с exe): {}", base_dir.display());

    let config_path = base_dir.join("config.json");
    let token_path = base_dir.join("token.json");
    let phrases_path = base_dir.join("phrases.json");
    let panel_config_path = base_dir.join("panel.toml");
    let db_path = base_dir.join("panel.db");

    let config = Arc::new(RwLock::new(config::load_or_create(&config_path.to_string_lossy())));
    let token = token_store::load_or_prompt(&token_path.to_string_lossy());
    let phrases = Arc::new(RwLock::new(phrases::load(&phrases_path.to_string_lossy())));
    let panel_cfg = panel_config::load_or_create(&panel_config_path.to_string_lossy());

    {
        let config_guard = config.read().await;
        let phrases_guard = phrases.read().await;
        println!("Текстовых реакций загружено: {}", phrases_guard.text_reactions.len());
        println!("Голосовых команд загружено: {}", phrases_guard.voice_commands.len());
        println!(
            "Порог похожести фраз: {} (правь в config.json)",
            config_guard.similarity_threshold
        );
    }

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_VOICE_STATES
        | GatewayIntents::GUILDS;

    let mut client = Client::builder(&token, intents)
        .event_handler(handler::Handler)
        .register_songbird()
        .await
        .expect("Ошибка создания клиента. Проверь, что токен корректный.");

    // БД инициализируем всегда (не только если включена панель) — она нужна
    // и /anon (роли, кулдаун, псевдонимы), и веб-панели, если та включена.
    let db_pool = match db::init(&db_path).await {
        Ok(pool) => Some(pool),
        Err(e) => {
            eprintln!(
                "[БД] Не удалось открыть {}: {e:?}. Веб-панель и /anon будут недоступны.",
                db_path.display()
            );
            None
        }
    };

    let playback_states = Arc::new(Mutex::new(HashMap::new()));

    {
        let mut data = client.data.write().await;
        data.insert::<ConfigKey>(config.clone());
        data.insert::<PhrasesKey>(phrases.clone());
        data.insert::<BaseDirKey>(base_dir.clone());
        data.insert::<ShardManagerKey>(client.shard_manager.clone());
        data.insert::<PlaybackStates>(playback_states.clone());
        data.insert::<DbKey>(db_pool.clone());
    }

    // Фоновая задача: каждые 30 секунд печатает в консоль текущий пинг
    // (задержку) соединения с Discord по каждому шарду.
    {
        let shard_manager = client.shard_manager.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                let runners = shard_manager.runners.lock().await;
                if runners.is_empty() {
                    println!("[ПИНГ] Шарды ещё не запущены.");
                    continue;
                }
                for (shard_id, runner) in runners.iter() {
                    match runner.latency {
                        Some(latency) => {
                            println!("[ПИНГ] Шард {shard_id}: {} мс", latency.as_millis())
                        }
                        None => println!("[ПИНГ] Шард {shard_id}: ещё не измерен (жди следующий heartbeat)"),
                    }
                }
            }
        });
    }

    // --- Веб-панель (опционально, см. panel.toml -> enabled) -----------------
    if !panel_cfg.enabled {
        println!("[ПАНЕЛЬ] Отключена в panel.toml (enabled = false).");
    } else if let Some(pool) = db_pool.clone() {
        let web_dir = base_dir.join("web");
        if !web_dir.exists() {
            eprintln!(
                "[ПАНЕЛЬ] Папка {} не найдена — веб-панель запущена не будет. \
                 Скопируй туда содержимое web/ из репозитория.",
                web_dir.display()
            );
        } else {
            let app_state = web::AppState {
                shard_manager: client.shard_manager.clone(),
                playback_states: playback_states.clone(),
                cache: client.cache.clone(),
                start_time: Instant::now(),
                db: pool,
                base_dir: base_dir.clone(),
                oauth: Arc::new(panel_cfg.discord_oauth.clone()),
                initial_admin_discord_id: Arc::new(panel_cfg.initial_admin_discord_id.clone()),
                auth_mode: panel_cfg.auth_mode,
                password_hash: Arc::new(panel_cfg.password_hash.clone()),
                oauth_states: Arc::new(Mutex::new(HashMap::new())),
                config: config.clone(),
                phrases: phrases.clone(),
                config_path: config_path.clone(),
                phrases_path: phrases_path.clone(),
            };
            let router = web::build_router(app_state, web_dir);
            let addr = format!("{}:{}", panel_cfg.host, panel_cfg.port);

            match tokio::net::TcpListener::bind(&addr).await {
                Ok(listener) => {
                    println!("[ПАНЕЛЬ] Веб-панель слушает на http://{addr}");
                    tokio::spawn(async move {
                        if let Err(e) = axum::serve(listener, router).await {
                            eprintln!("[ПАНЕЛЬ] Веб-сервер завершился с ошибкой: {e:?}");
                        }
                    });
                }
                Err(e) => {
                    eprintln!("[ПАНЕЛЬ] Не удалось занять адрес {addr}: {e}. Панель не запущена.");
                }
            }
        }
    } else {
        eprintln!("[ПАНЕЛЬ] БД недоступна — панель не запущена.");
    }

    println!("Подключаюсь к Discord...");
    if let Err(e) = client.start().await {
        tracing::error!("Клиент завершился с ошибкой: {e:?}");
        eprintln!("Ошибка: {e:?}");
    }
}