mod config;
mod handler;
mod phrases;
mod state;
mod token_store;

use serenity::prelude::*;
use songbird::SerenityInit;
use state::{BaseDirKey, ConfigKey, PhrasesKey, PlaybackStates, ShardManagerKey};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::sync::Mutex;

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

    let base_dir = exe_dir();
    println!("Рабочая папка (рядом с exe): {}", base_dir.display());

    let config_path = base_dir.join("config.json");
    let token_path = base_dir.join("token.json");
    let phrases_path = base_dir.join("phrases.json");

    let config = config::load_or_create(&config_path.to_string_lossy());
    let token = token_store::load_or_prompt(&token_path.to_string_lossy());
    let phrases = phrases::load(&phrases_path.to_string_lossy());

    println!("Текстовых реакций загружено: {}", phrases.text_reactions.len());
    println!("Голосовых команд загружено: {}", phrases.voice_commands.len());
    println!(
        "Порог похожести фраз: {} (правь в config.json)",
        config.similarity_threshold
    );

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_VOICE_STATES
        | GatewayIntents::GUILDS;

    let mut client = Client::builder(&token, intents)
        .event_handler(handler::Handler)
        .register_songbird()
        .await
        .expect("Ошибка создания клиента. Проверь, что токен корректный.");

    {
        let mut data = client.data.write().await;
        data.insert::<ConfigKey>(Arc::new(config));
        data.insert::<PhrasesKey>(Arc::new(phrases));
        data.insert::<BaseDirKey>(Arc::new(base_dir));
        data.insert::<ShardManagerKey>(client.shard_manager.clone());
        data.insert::<PlaybackStates>(Arc::new(Mutex::new(HashMap::new())));
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

    println!("Подключаюсь к Discord...");
    if let Err(e) = client.start().await {
        tracing::error!("Клиент завершился с ошибкой: {e:?}");
        eprintln!("Ошибка: {e:?}");
    }
}