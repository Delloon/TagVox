-- Все таблицы с настройками несут nullable guild_id: NULL означает
-- "глобальная настройка на весь бот" (единственный сценарий, который
-- реально используется сейчас). Если в будущем понадобится своя
-- конфигурация на конкретный Discord-сервер — достаточно начать писать
-- туда не-NULL guild_id, менять схему не придётся.

CREATE TABLE bot_config (
    guild_id                       TEXT,
    command_prefix                 TEXT NOT NULL DEFAULT '!',
    similarity_threshold           REAL NOT NULL DEFAULT 0.78,
    require_mention_for_reactions  INTEGER NOT NULL DEFAULT 1,
    sounds_dir                     TEXT NOT NULL DEFAULT 'sounds',
    updated_at                     INTEGER NOT NULL,
    PRIMARY KEY (guild_id)
);

CREATE TABLE text_reactions (
    id          TEXT NOT NULL,
    guild_id    TEXT,
    triggers    TEXT NOT NULL,  -- JSON-массив строк
    responses   TEXT NOT NULL,  -- JSON-массив строк
    updated_at  INTEGER NOT NULL,
    PRIMARY KEY (id, guild_id)
);

CREATE TABLE voice_commands (
    id          TEXT NOT NULL,
    guild_id    TEXT,
    triggers    TEXT NOT NULL,  -- JSON-массив строк
    file        TEXT NOT NULL,
    updated_at  INTEGER NOT NULL,
    PRIMARY KEY (id, guild_id)
);

CREATE TABLE unknown_responses (
    guild_id    TEXT,
    responses   TEXT NOT NULL,  -- JSON-массив строк
    updated_at  INTEGER NOT NULL,
    PRIMARY KEY (guild_id)
);

-- Discord-пользователи, которым разрешён вход в панель (заполняется вручную
-- владельцем бота — см. panel.toml -> initial_admin_discord_id).
CREATE TABLE admins (
    discord_user_id TEXT PRIMARY KEY,
    display_name    TEXT,
    added_at        INTEGER NOT NULL
);

-- Сессии веб-панели (кука session_id -> discord_user_id).
CREATE TABLE sessions (
    id                TEXT PRIMARY KEY,
    discord_user_id   TEXT NOT NULL,
    created_at        INTEGER NOT NULL,
    expires_at        INTEGER NOT NULL
);

CREATE INDEX idx_sessions_expires_at ON sessions (expires_at);