-- Настройки модуля «Анонимные сообщения» — per-guild, потому что канал и
-- роли физически привязаны к конкретному Discord-серверу.
CREATE TABLE anon_settings (
    guild_id          TEXT PRIMARY KEY,
    enabled           INTEGER NOT NULL DEFAULT 1,
    channel_id        TEXT,                       -- NULL, пока админ не настроил канал
    allowed_role_ids  TEXT NOT NULL DEFAULT '[]',  -- JSON-массив строк (id ролей)
    cooldown_seconds  INTEGER NOT NULL DEFAULT 30,
    updated_at        INTEGER NOT NULL
);

-- Постоянный псевдоним (номер) на пользователя в рамках одного сервера —
-- один и тот же номер при каждом его анонимном сообщении на этом сервере.
CREATE TABLE anon_identities (
    guild_id         TEXT NOT NULL,
    discord_user_id  TEXT NOT NULL,
    anon_number      INTEGER NOT NULL,
    created_at       INTEGER NOT NULL,
    PRIMARY KEY (guild_id, discord_user_id)
);

CREATE UNIQUE INDEX idx_anon_identities_number ON anon_identities (guild_id, anon_number);

-- Внутренний журнал каждого отправленного анонимного сообщения — не виден
-- обычным участникам сервера, нужен для модерации, жалоб и кулдауна.
CREATE TABLE anon_messages (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    guild_id            TEXT NOT NULL,
    discord_user_id     TEXT NOT NULL,
    anon_number         INTEGER NOT NULL,
    content             TEXT NOT NULL,
    channel_id          TEXT NOT NULL,
    discord_message_id  TEXT,
    created_at          INTEGER NOT NULL
);

CREATE INDEX idx_anon_messages_guild ON anon_messages (guild_id);
CREATE INDEX idx_anon_messages_user ON anon_messages (guild_id, discord_user_id);