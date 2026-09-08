use crate::{config::Config, phrases::Phrases};
use serenity::{model::id::GuildId, prelude::TypeMapKey};
use songbird::tracks::TrackHandle;
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::sync::Mutex;

pub struct ConfigKey;
impl TypeMapKey for ConfigKey {
    type Value = Arc<Config>;
}

/// Папка, где лежит сам исполняемый файл. Все относительные пути к
/// config.json / phrases.json / sounds/ считаются от неё, а не от текущей
/// рабочей директории — так JSON-файлы можно спокойно редактировать рядом
/// со скомпилированным .exe, независимо от того, откуда его запускают.
pub struct BaseDirKey;
impl TypeMapKey for BaseDirKey {
    type Value = Arc<PathBuf>;
}

pub struct PhrasesKey;
impl TypeMapKey for PhrasesKey {
    type Value = Arc<Phrases>;
}

/// Доступ к менеджеру шардов — через него можно узнать текущий пинг
/// (задержку) соединения бота с Discord.
pub struct ShardManagerKey;
impl TypeMapKey for ShardManagerKey {
    type Value = Arc<serenity::gateway::ShardManager>;
}

/// Состояние воспроизведения для одного сервера: текущий трек и id голосовой
/// команды, к которой он привязан (чтобы понимать, что именно возобновляем).
#[derive(Default)]
pub struct GuildPlayback {
    pub handle: Option<TrackHandle>,
    pub command_id: Option<String>,
}

pub struct PlaybackStates;
impl TypeMapKey for PlaybackStates {
    type Value = Arc<Mutex<HashMap<GuildId, GuildPlayback>>>;
}