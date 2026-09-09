use crate::state::{ConfigKey, DbKey};
use serenity::{
    builder::{
        CreateCommand, CreateCommandOption, CreateEmbed, CreateInteractionResponse,
        CreateInteractionResponseMessage, CreateMessage,
    },
    model::{
        application::{CommandInteraction, CommandOptionType, ResolvedValue},
        id::{ChannelId, GuildId},
    },
    prelude::Context,
};
use std::time::{SystemTime, UNIX_EPOCH};

pub const COMMAND_NAME: &str = "anon";

/// Определение слэш-команды /anon.
pub fn command_definition() -> CreateCommand {
    CreateCommand::new(COMMAND_NAME)
        .description("Отправить анонимное сообщение в настроенный канал")
        .add_option(
            CreateCommandOption::new(CommandOptionType::String, "message", "Текст сообщения")
                .required(true),
        )
}

/// Регистрирует /anon для одного сервера. Использует bulk-overwrite
/// (set_commands), поэтому безопасно вызывать повторно при каждом запуске —
/// дублей команды не появится.
pub async fn register_for_guild(ctx: &Context, guild_id: GuildId) {
    if let Err(e) = guild_id.set_commands(&ctx.http, vec![command_definition()]).await {
        tracing::error!("Не удалось зарегистрировать /anon для guild={guild_id}: {e:?}");
    }
}

fn now_ts() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

async fn reply_ephemeral(ctx: &Context, command: &CommandInteraction, text: impl Into<String>) {
    let response = CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new().content(text.into()).ephemeral(true),
    );
    if let Err(e) = command.create_response(&ctx.http, response).await {
        tracing::error!("Не удалось ответить на /anon: {e:?}");
    }
}

/// Обрабатывает вызов /anon. Ничего не делает, если команда называется иначе
/// (на случай появления других слэш-команд в будущем).
pub async fn handle(ctx: &Context, command: &CommandInteraction) {
    if command.data.name != COMMAND_NAME {
        return;
    }

    let Some(guild_id) = command.guild_id else {
        reply_ephemeral(ctx, command, "Эта команда работает только на сервере.").await;
        return;
    };

    let message = command.data.options().into_iter().find_map(|opt| match (opt.name, opt.value) {
        ("message", ResolvedValue::String(s)) => Some(s.to_string()),
        _ => None,
    });

    let Some(message) = message else {
        reply_ephemeral(ctx, command, "Не удалось прочитать текст сообщения.").await;
        return;
    };
    let message = message.trim().to_string();
    if message.is_empty() {
        reply_ephemeral(ctx, command, "Сообщение не может быть пустым.").await;
        return;
    }
    if message.chars().count() > 2000 {
        reply_ephemeral(ctx, command, "Слишком длинное сообщение (максимум 2000 символов).").await;
        return;
    }

    // Глобальный тумблер модуля (панель -> «Модули»)
    let module_enabled = {
        let data = ctx.data.read().await;
        let config_lock = data.get::<ConfigKey>().unwrap().clone();
        drop(data);

        let config = config_lock.read().await;
        config.modules.anonymous_messages
    };
    if !module_enabled {
        reply_ephemeral(ctx, command, "Анонимные сообщения сейчас отключены администратором бота.").await;
        return;
    }

    let db = {
        let data = ctx.data.read().await;
        data.get::<DbKey>().unwrap().clone()
    };
    let Some(db) = db else {
        reply_ephemeral(ctx, command, "Функция недоступна: база данных не инициализирована.").await;
        return;
    };

    let guild_id_str = guild_id.to_string();

    let settings: Option<(i64, Option<String>, String, i64)> = sqlx::query_as(
        "SELECT enabled, channel_id, allowed_role_ids, cooldown_seconds FROM anon_settings WHERE guild_id = ?",
    )
    .bind(&guild_id_str)
    .fetch_optional(&db)
    .await
    .unwrap_or(None);

    let Some((enabled, channel_id, allowed_role_ids_json, cooldown_seconds)) = settings else {
        reply_ephemeral(
            ctx,
            command,
            "Администратор ещё не настроил анонимные сообщения на этом сервере (нужно сделать это в веб-панели).",
        )
        .await;
        return;
    };

    if enabled == 0 {
        reply_ephemeral(ctx, command, "Анонимные сообщения выключены на этом сервере.").await;
        return;
    }

    let Some(channel_id) = channel_id else {
        reply_ephemeral(ctx, command, "Администратор ещё не выбрал канал для анонимных сообщений.").await;
        return;
    };

    // Проверка роли — доступ только тем, кого явно разрешил администратор
    let allowed_role_ids: Vec<String> = serde_json::from_str(&allowed_role_ids_json).unwrap_or_default();
    let has_role = command
        .member
        .as_ref()
        .map(|m| m.roles.iter().any(|r| allowed_role_ids.contains(&r.to_string())))
        .unwrap_or(false);

    if !has_role {
        reply_ephemeral(ctx, command, "У тебя нет прав на использование анонимных сообщений на этом сервере.").await;
        return;
    }

    let author_id_str = command.user.id.to_string();

    // Кулдаун — настраивается в панели (cooldown_seconds)
    let last_sent: Option<i64> = sqlx::query_scalar(
        "SELECT MAX(created_at) FROM anon_messages WHERE guild_id = ? AND discord_user_id = ?",
    )
    .bind(&guild_id_str)
    .bind(&author_id_str)
    .fetch_one(&db)
    .await
    .unwrap_or(None);

    if let Some(last) = last_sent {
        let elapsed = now_ts() - last;
        if elapsed < cooldown_seconds {
            let wait = cooldown_seconds - elapsed;
            reply_ephemeral(ctx, command, format!("Подожди ещё {wait} сек. перед следующим анонимным сообщением.")).await;
            return;
        }
    }

    // Постоянный номер-псевдоним на пользователя в рамках этого сервера
    let existing_number: Option<i64> = sqlx::query_scalar(
        "SELECT anon_number FROM anon_identities WHERE guild_id = ? AND discord_user_id = ?",
    )
    .bind(&guild_id_str)
    .bind(&author_id_str)
    .fetch_optional(&db)
    .await
    .unwrap_or(None);

    let anon_number = match existing_number {
        Some(n) => n,
        None => {
            let next: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(anon_number), 0) + 1 FROM anon_identities WHERE guild_id = ?",
            )
            .bind(&guild_id_str)
            .fetch_one(&db)
            .await
            .unwrap_or(1);

            let _ = sqlx::query(
                "INSERT INTO anon_identities (guild_id, discord_user_id, anon_number, created_at) VALUES (?, ?, ?, ?)",
            )
            .bind(&guild_id_str)
            .bind(&author_id_str)
            .bind(next)
            .bind(now_ts())
            .execute(&db)
            .await;

            next
        }
    };

    let target_channel: ChannelId = match channel_id.parse::<u64>() {
        Ok(id) => ChannelId::new(id),
        Err(_) => {
            reply_ephemeral(ctx, command, "Некорректно настроен канал — обратись к администратору.").await;
            return;
        }
    };

    let embed = CreateEmbed::new()
        .title(format!("🕵️ Аноним #{anon_number}"))
        .description(&message)
        .color(0xE8A33D);

    match target_channel.send_message(&ctx.http, CreateMessage::new().embed(embed)).await {
        Ok(sent_msg) => {
            let _ = sqlx::query(
                "INSERT INTO anon_messages (guild_id, discord_user_id, anon_number, content, channel_id, discord_message_id, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&guild_id_str)
            .bind(&author_id_str)
            .bind(anon_number)
            .bind(&message)
            .bind(&channel_id)
            .bind(sent_msg.id.to_string())
            .bind(now_ts())
            .execute(&db)
            .await;

            reply_ephemeral(ctx, command, format!("Отправлено анонимно в <#{channel_id}>.")).await;
        }
        Err(e) => {
            tracing::error!("Не удалось отправить анонимное сообщение: {e:?}");
            reply_ephemeral(ctx, command, "Не удалось отправить сообщение — проверь права бота в канале.").await;
        }
    }
}