use crate::{
    phrases::{self, VoiceCommand},
    state::{BaseDirKey, ConfigKey, GuildPlayback, PhrasesKey, PlaybackStates, ShardManagerKey},
};
use rand::seq::SliceRandom;
use regex::Regex;
use serenity::{
    all::{
        ButtonStyle, ComponentInteraction, CreateActionRow, CreateButton,
        CreateInteractionResponse, CreateInteractionResponseMessage, CreateMessage, Interaction,
    },
    async_trait,
    model::{
        channel::Message,
        gateway::Ready,
        id::{ChannelId, GuildId},
        voice::VoiceState,
    },
    prelude::*,
};
use songbird::tracks::PlayMode;
use std::path::{Path, PathBuf};

const BTN_PAUSE: &str = "voice_pause";
const BTN_RESUME: &str = "voice_resume";
const BTN_STOP: &str = "voice_stop";

pub struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _ctx: Context, ready: Ready) {
        tracing::info!("Бот запущен как {}", ready.user.name);
        println!("Бот запущен как {}", ready.user.name);
    }

    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            return;
        }

        let (config, phrases) = {
            let data = ctx.data.read().await;
            (
                data.get::<ConfigKey>().unwrap().clone(),
                data.get::<PhrasesKey>().unwrap().clone(),
            )
        };

        let content = msg.content.trim().to_string();

        // ---- управляющие команды (!pause, !resume, !stop, !join, !leave) ----
        // работают всегда, без необходимости тегать бота
        if let Some(cmd) = content.strip_prefix(config.command_prefix.as_str()) {
            if handle_control_command(&ctx, &msg, cmd.trim()).await {
                return;
            }
        }

        // ---- реакции и голосовые команды — только если бота тегнули ----
        if config.require_mention_for_reactions {
            match msg.mentions_me(&ctx).await {
                Ok(true) => {}
                _ => return,
            }
        }

        let clean_text = strip_mentions(&content);
        if clean_text.is_empty() {
            return;
        }

        if let Some(vc) = phrases::best_voice_command(&clean_text, &phrases, config.similarity_threshold)
        {
            play_or_resume(&ctx, &msg, vc).await;
            return;
        }

        if let Some(tr) =
            phrases::best_text_reaction(&clean_text, &phrases, config.similarity_threshold)
        {
            let reply = tr.responses.choose(&mut rand::thread_rng()).cloned();
            if let Some(reply) = reply {
                let _ = msg.channel_id.say(&ctx.http, reply).await;
            }
            return;
        }

        let reply = phrases.unknown_response.choose(&mut rand::thread_rng()).cloned();
        if let Some(reply) = reply {
            let _ = msg.channel_id.say(&ctx.http, reply).await;
        }
    }

    /// Обрабатывает клики по кнопкам Пауза/Продолжить/Стоп под сообщениями бота.
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Component(component) = interaction {
            handle_voice_button(&ctx, &component).await;
        }
    }
}

/// Убирает Discord-упоминания вида <@123> / <@!123> из текста сообщения.
fn strip_mentions(s: &str) -> String {
    let re = Regex::new(r"<@!?\d+>").unwrap();
    re.replace_all(s, "").trim().to_string()
}

fn find_user_voice_channel(
    cache: &serenity::cache::Cache,
    guild_id: GuildId,
    user_id: serenity::model::id::UserId,
) -> Option<ChannelId> {
    let guild = cache.guild(guild_id)?;
    let voice_state: &VoiceState = guild.voice_states.get(&user_id)?;
    voice_state.channel_id
}

/// Ряд кнопок управления воспроизведением, который прикрепляется к
/// сообщениям бота о голосовых командах — работает в том же чате, где
/// команда была вызвана.
fn voice_control_buttons() -> CreateActionRow {
    CreateActionRow::Buttons(vec![
        CreateButton::new(BTN_PAUSE)
            .label("⏸ Пауза")
            .style(ButtonStyle::Secondary),
        CreateButton::new(BTN_RESUME)
            .label("▶ Продолжить")
            .style(ButtonStyle::Success),
        CreateButton::new(BTN_STOP)
            .label("⏹ Стоп")
            .style(ButtonStyle::Danger),
    ])
}

/// Отправляет сообщение с кнопками управления в указанный канал (тот, где
/// была вызвана голосовая команда).
async fn send_with_controls(ctx: &Context, channel_id: ChannelId, text: impl Into<String>) {
    let builder = CreateMessage::new()
        .content(text.into())
        .components(vec![voice_control_buttons()]);
    if let Err(e) = channel_id.send_message(&ctx.http, builder).await {
        tracing::error!("Не удалось отправить сообщение с кнопками: {e:?}");
    }
}

/// Клик по одной из кнопок Пауза/Продолжить/Стоп. Отвечаем коротким
/// эфемерным сообщением (видно только тому, кто нажал), чтобы не засорять
/// чат повторными "Пауза на ..." репликами.
async fn handle_voice_button(ctx: &Context, component: &ComponentInteraction) {
    let Some(guild_id) = component.guild_id else {
        return;
    };

    let text = match component.data.custom_id.as_str() {
        BTN_PAUSE => pause_playback_text(ctx, guild_id).await,
        BTN_RESUME => resume_playback_text(ctx, guild_id).await,
        BTN_STOP => stop_playback_text(ctx, guild_id).await,
        _ => return,
    };

    let response = CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new()
            .content(text)
            .ephemeral(true),
    );
    if let Err(e) = component.create_response(&ctx.http, response).await {
        tracing::error!("Не удалось ответить на нажатие кнопки: {e:?}");
    }
}

async fn handle_control_command(ctx: &Context, msg: &Message, cmd: &str) -> bool {
    let guild_id = match msg.guild_id {
        Some(g) => g,
        None => return false,
    };

    match cmd {
        "pause" => {
            let text = pause_playback_text(ctx, guild_id).await;
            send_with_controls(ctx, msg.channel_id, text).await;
            true
        }
        "resume" | "continue" => {
            let text = resume_playback_text(ctx, guild_id).await;
            send_with_controls(ctx, msg.channel_id, text).await;
            true
        }
        "stop" => {
            let text = stop_playback_text(ctx, guild_id).await;
            let _ = msg.reply(&ctx.http, text).await;
            true
        }
        "leave" => {
            leave_voice(ctx, guild_id, msg).await;
            true
        }
        "join" => {
            join_author_channel(ctx, guild_id, msg).await;
            true
        }
        "ping" => {
            show_ping(ctx, msg).await;
            true
        }
        _ => false,
    }
}

async fn show_ping(ctx: &Context, msg: &Message) {
    let shard_manager = {
        let data = ctx.data.read().await;
        data.get::<ShardManagerKey>().unwrap().clone()
    };
    let runners = shard_manager.runners.lock().await;

    let text = if let Some(runner) = runners.get(&ctx.shard_id) {
        match runner.latency {
            Some(latency) => format!("Пинг: {} мс.", latency.as_millis()),
            None => "Пинг ещё не измерен, попробуй через полминуты.".to_string(),
        }
    } else {
        "Не нашёл информацию о своём шарде.".to_string()
    };

    println!("[ПИНГ] Запрошен вручную: {text}");
    let _ = msg.reply(&ctx.http, text).await;
}

/// Подключается к голосовому каналу и в случае неудачи печатает максимум
/// подробностей: и в трейсинг-логи, и прямо в консоль, и (коротко) в ответ
/// в Discord — чтобы не пришлось лезть в логи ради каждой мелочи.
async fn join_voice_channel(
    ctx: &Context,
    guild_id: GuildId,
    channel_id: ChannelId,
    msg: &Message,
) -> bool {
    tracing::debug!("Пробую подключиться: guild={guild_id} channel={channel_id}");
    println!("[ГОЛОС] Пробую подключиться: guild_id={guild_id} channel_id={channel_id}");

    let manager = songbird::get(ctx).await.expect("songbird не инициализирован").clone();
    match manager.join(guild_id, channel_id).await {
        Ok(_) => {
            tracing::info!("Подключился к голосовому каналу guild={guild_id} channel={channel_id}");
            println!("[ГОЛОС] Подключился: guild_id={guild_id} channel_id={channel_id}");
            true
        }
        Err(e) => {
            tracing::error!(
                "Ошибка подключения к голосу (guild={guild_id}, channel={channel_id}): {e:?}"
            );
            eprintln!(
                "[ГОЛОС] Не удалось подключиться. guild_id={guild_id} channel_id={channel_id}\n\
                 Техническая причина: {e:?}\n\
                 Частые причины:\n\
                 - у бота нет прав Connect / Speak в этом канале или на сервере;\n\
                 - UDP-трафик блокирует антивирус/файрвол/VPN на этом ПК (голос идёт по UDP, не TCP);\n\
                 - канал — Stage-канал, для сцен нужен отдельный запрос на выступление;\n\
                 - сервер только что стартовал и ещё не успел закэшировать голосовые данные — попробуй ещё раз через пару секунд;\n\
                 - региональный voice-сервер Discord временно недоступен."
            );
            let short_reason = {
                let s = format!("{e:?}");
                if s.chars().count() > 300 {
                    format!("{}...", s.chars().take(300).collect::<String>())
                } else {
                    s
                }
            };
            let _ = msg
                .reply(
                    &ctx.http,
                    format!(
                        "Не смог подключиться к каналу.\nПричина: `{short_reason}`\nПолный лог — в консоли бота."
                    ),
                )
                .await;
            false
        }
    }
}

async fn join_author_channel(ctx: &Context, guild_id: GuildId, msg: &Message) {
    let channel_id = find_user_voice_channel(&ctx.cache, guild_id, msg.author.id);
    let channel_id = match channel_id {
        Some(id) => id,
        None => {
            let _ = msg.reply(&ctx.http, "Зайди сначала в голосовой канал.").await;
            return;
        }
    };
    if join_voice_channel(ctx, guild_id, channel_id, msg).await {
        let _ = msg.reply(&ctx.http, "Подключился к голосовому каналу.").await;
    }
}

async fn leave_voice(ctx: &Context, guild_id: GuildId, msg: &Message) {
    let manager = songbird::get(ctx).await.expect("songbird не инициализирован").clone();

    let states = {
        let data = ctx.data.read().await;
        data.get::<PlaybackStates>().unwrap().clone()
    };
    states.lock().await.remove(&guild_id);

    if let Err(e) = manager.remove(guild_id).await {
        tracing::error!("voice leave error: {e:?}");
        let _ = msg.reply(&ctx.http, "Я и так не в канале.").await;
    } else {
        let _ = msg.reply(&ctx.http, "Отключился.").await;
    }
}

fn format_time(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// Ставит текущий трек сервера на паузу (позиция запоминается драйвером
/// songbird автоматически) и возвращает текст с результатом.
async fn pause_playback_text(ctx: &Context, guild_id: GuildId) -> String {
    let states = {
        let data = ctx.data.read().await;
        data.get::<PlaybackStates>().unwrap().clone()
    };
    let states = states.lock().await;
    if let Some(entry) = states.get(&guild_id) {
        if let Some(handle) = &entry.handle {
            let _ = handle.pause();
            if let Ok(info) = handle.get_info().await {
                return format!("⏸ Пауза на {}.", format_time(info.position.as_secs()));
            }
            return "⏸ Пауза.".to_string();
        }
    }
    "Сейчас ничего не играет.".to_string()
}

/// Возобновляет воспроизведение с сохранённой позиции.
async fn resume_playback_text(ctx: &Context, guild_id: GuildId) -> String {
    let states = {
        let data = ctx.data.read().await;
        data.get::<PlaybackStates>().unwrap().clone()
    };
    let states = states.lock().await;
    if let Some(entry) = states.get(&guild_id) {
        if let Some(handle) = &entry.handle {
            let _ = handle.play();
            if let Ok(info) = handle.get_info().await {
                return format!("▶ Продолжаю с {}.", format_time(info.position.as_secs()));
            }
            return "▶ Продолжаю.".to_string();
        }
    }
    "Сейчас ничего не на паузе.".to_string()
}

/// Полностью останавливает трек и сбрасывает сохранённую позицию.
async fn stop_playback_text(ctx: &Context, guild_id: GuildId) -> String {
    let states = {
        let data = ctx.data.read().await;
        data.get::<PlaybackStates>().unwrap().clone()
    };
    let mut states = states.lock().await;
    if let Some(entry) = states.get_mut(&guild_id) {
        if let Some(handle) = entry.handle.take() {
            let _ = handle.stop();
        }
        entry.command_id = None;
    }
    "⏹ Остановлено, позиция сброшена.".to_string()
}

/// Запускает голосовую команду: если тот же трек уже загружен и стоит на
/// паузе — продолжает с сохранённого места, иначе подключается к каналу
/// автора и запускает файл заново. Сообщение с кнопками управления
/// отправляется в тот же канал, где была вызвана команда.
async fn play_or_resume(ctx: &Context, msg: &Message, vc: &VoiceCommand) {
    let guild_id = match msg.guild_id {
        Some(g) => g,
        None => {
            let _ = msg.reply(&ctx.http, "Эта команда работает только на сервере.").await;
            return;
        }
    };

    let file_path: PathBuf = {
        let data = ctx.data.read().await;
        let config = data.get::<ConfigKey>().unwrap().clone();
        let base_dir = data.get::<BaseDirKey>().unwrap().clone();
        let raw = Path::new(&vc.file);
        if raw.is_absolute() {
            // Полный путь (например C:\music\track.mp3 или /home/user/track.mp3) — используем как есть
            raw.to_path_buf()
        } else if vc.file.contains('/') || vc.file.contains('\\') {
            // Путь со слэшами, но не абсолютный — считаем его от папки с exe
            base_dir.join(raw)
        } else {
            // Просто имя файла — ищем в папке sounds_dir рядом с exe
            base_dir.join(&config.sounds_dir).join(raw)
        }
    };

    if !file_path.exists() {
        let _ = msg
            .reply(&ctx.http, format!("Файл `{}` не найден на диске.", file_path.display()))
            .await;
        return;
    }

    let states = {
        let data = ctx.data.read().await;
        data.get::<PlaybackStates>().unwrap().clone()
    };
    let mut states = states.lock().await;
    let entry = states.entry(guild_id).or_insert_with(GuildPlayback::default);

    // Тот же трек уже был запущен на этом сервере — пробуем возобновить
    if entry.command_id.as_deref() == Some(vc.id.as_str()) {
        if let Some(handle) = &entry.handle {
            if let Ok(info) = handle.get_info().await {
                match info.playing {
                    PlayMode::Pause => {
                        let _ = handle.play();
                        let text = format!(
                            "▶ Продолжаю «{}» с {}.",
                            vc.id,
                            format_time(info.position.as_secs())
                        );
                        drop(states);
                        send_with_controls(ctx, msg.channel_id, text).await;
                        return;
                    }
                    PlayMode::Play => {
                        drop(states);
                        send_with_controls(ctx, msg.channel_id, "Уже играет.").await;
                        return;
                    }
                    _ => {} // Stop/End/Errored -> запустим заново ниже
                }
            }
        }
    }

    // Подключаемся к каналу автора, если ещё не в канале
    let manager = songbird::get(ctx).await.expect("songbird не инициализирован").clone();
    if manager.get(guild_id).is_none() {
        let channel_id = find_user_voice_channel(&ctx.cache, guild_id, msg.author.id);
        let channel_id = match channel_id {
            Some(id) => id,
            None => {
                let _ = msg.reply(&ctx.http, "Зайди сначала в голосовой канал.").await;
                return;
            }
        };
        if !join_voice_channel(ctx, guild_id, channel_id, msg).await {
            return;
        }
    }

    if let Some(call) = manager.get(guild_id) {
        let mut call = call.lock().await;
        let source = songbird::input::File::new(file_path.clone());
        let handle = call.play_input(source.into());
        entry.handle = Some(handle);
        entry.command_id = Some(vc.id.clone());
        drop(call);
        drop(states);
        send_with_controls(ctx, msg.channel_id, format!("▶ Воспроизвожу «{}».", vc.id)).await;
    }
}