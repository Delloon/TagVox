use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

/// Настройки бота. Хранятся в config.json и создаются автоматически при первом
/// запуске со значениями по умолчанию, если файла ещё нет.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Префикс для управляющих команд (!pause, !resume, !stop, !join, !leave)
    #[serde(default = "default_prefix")]
    pub command_prefix: String,

    /// Порог похожести фразы (0.0..1.0). Чем выше — тем точнее должна совпадать
    /// фраза пользователя с одним из триггеров в phrases.json.
    #[serde(default = "default_threshold")]
    pub similarity_threshold: f64,

    /// Если true — бот реагирует на фразы/голосовые команды только когда его
    /// тегнули (@упомянули) в сообщении. Управляющие команды (!pause и т.д.)
    /// работают в любом случае.
    #[serde(default = "default_true")]
    pub require_mention_for_reactions: bool,

    /// Папка, относительно которой ищутся звуковые файлы, если в phrases.json
    /// путь указан без слэшей (просто имя файла).
    #[serde(default = "default_sounds_dir")]
    pub sounds_dir: String,
}

fn default_prefix() -> String {
    "!".to_string()
}
fn default_threshold() -> f64 {
    0.78
}
fn default_true() -> bool {
    true
}
fn default_sounds_dir() -> String {
    "sounds".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            command_prefix: default_prefix(),
            similarity_threshold: default_threshold(),
            require_mention_for_reactions: true,
            sounds_dir: default_sounds_dir(),
        }
    }
}

/// Загружает config.json, либо создаёт его со значениями по умолчанию, если
/// файла ещё не существует.
pub fn load_or_create(path: &str) -> Config {
    if Path::new(path).exists() {
        let data = fs::read_to_string(path).expect("не удалось прочитать config.json");
        serde_json::from_str(&data).expect("config.json повреждён или содержит ошибку формата")
    } else {
        let cfg = Config::default();
        let data = serde_json::to_string_pretty(&cfg).unwrap();
        fs::write(path, data).expect("не удалось создать config.json");
        println!("Создан config.json со значениями по умолчанию. Можешь отредактировать его под себя.");
        cfg
    }
}
