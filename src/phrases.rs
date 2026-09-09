use serde::{Deserialize, Serialize};
use std::fs;

/// Текстовая реакция: набор похожих фраз-триггеров и набор ответов, из
/// которых бот выбирает случайный.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextReaction {
    pub id: String,
    pub triggers: Vec<String>,
    pub responses: Vec<String>,
}

/// Голосовая команда: набор похожих фраз-триггеров и локальный путь к
/// звуковому файлу, который нужно проиграть.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceCommand {
    pub id: String,
    pub triggers: Vec<String>,
    pub file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Phrases {
    #[serde(default)]
    pub text_reactions: Vec<TextReaction>,
    #[serde(default)]
    pub voice_commands: Vec<VoiceCommand>,
    #[serde(default = "default_unknown")]
    pub unknown_response: Vec<String>,
}

fn default_unknown() -> Vec<String> {
    vec!["Не понял, что ты имеешь в виду.".to_string()]
}

/// Загружает phrases.json. Файл обязателен — количество text_reactions и
/// voice_commands не ограничено, можно добавлять сколько угодно записей.
pub fn load(path: &str) -> Phrases {
    let data = fs::read_to_string(path).unwrap_or_else(|_| {
        panic!("Не найден {path}. Создай его по образцу из README.md / примера в репозитории.")
    });
    serde_json::from_str(&data).expect("phrases.json содержит ошибку формата")
}

/// Приводит строку к нижнему регистру и убирает пунктуацию/лишние пробелы,
/// чтобы сравнение фраз было устойчивее.
fn normalize(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c.is_whitespace() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Похожесть двух строк: 0.0 (совсем разные) .. 1.0 (идентичные).
/// Комбинирует Jaro-Winkler с бонусом, если одна строка целиком содержится в
/// другой — это позволяет ловить триггер внутри более длинной фразы.
fn score(text: &str, trigger: &str) -> f64 {
    let a = normalize(text);
    let b = normalize(trigger);
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let contains_bonus = if a.contains(&b) || b.contains(&a) {
        0.97
    } else {
        0.0
    };
    strsim::jaro_winkler(&a, &b).max(contains_bonus)
}

/// Находит текстовую реакцию, чей триггер больше всего похож на сообщение
/// пользователя, если похожесть не ниже порога.
pub fn best_text_reaction<'a>(
    text: &str,
    phrases: &'a Phrases,
    threshold: f64,
) -> Option<&'a TextReaction> {
    let mut best: Option<(&TextReaction, f64)> = None;
    for entry in &phrases.text_reactions {
        for trigger in &entry.triggers {
            let s = score(text, trigger);
            if best.as_ref().map_or(true, |(_, bs)| s > *bs) {
                best = Some((entry, s));
            }
        }
    }
    best.filter(|(_, s)| *s >= threshold).map(|(e, _)| e)
}

/// То же самое, но среди голосовых команд.
pub fn best_voice_command<'a>(
    text: &str,
    phrases: &'a Phrases,
    threshold: f64,
) -> Option<&'a VoiceCommand> {
    let mut best: Option<(&VoiceCommand, f64)> = None;
    for entry in &phrases.voice_commands {
        for trigger in &entry.triggers {
            let s = score(text, trigger);
            if best.as_ref().map_or(true, |(_, bs)| s > *bs) {
                best = Some((entry, s));
            }
        }
    }
    best.filter(|(_, s)| *s >= threshold).map(|(e, _)| e)
}