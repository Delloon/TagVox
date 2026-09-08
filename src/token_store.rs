use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path};

#[derive(Serialize, Deserialize)]
struct TokenFile {
    token: String,
}

/// При первом запуске один раз спрашивает токен в консоли (ввод скрыт, как
/// пароль) и сохраняет его в token.json. При последующих запусках токен
/// читается из файла и заново не спрашивается.
pub fn load_or_prompt(path: &str) -> String {
    if Path::new(path).exists() {
        let data = fs::read_to_string(path).expect("не удалось прочитать token.json");
        let parsed: TokenFile =
            serde_json::from_str(&data).expect("token.json повреждён или содержит ошибку формата");
        if !parsed.token.trim().is_empty() {
            return parsed.token;
        }
    }

    print!("Токен не найден. Введи токен Discord-бота (ввод скрыт): ");
    std::io::stdout().flush().ok();
    let token = rpassword::read_password()
        .unwrap_or_default()
        .trim()
        .to_string();

    if token.is_empty() {
        panic!("Токен не может быть пустым.");
    }

    let data = serde_json::to_string_pretty(&TokenFile {
        token: token.clone(),
    })
    .unwrap();
    fs::write(path, data).expect("не удалось сохранить token.json");
    println!("Токен сохранён в {path}. В следующий раз спрашивать не буду.");

    token
}
