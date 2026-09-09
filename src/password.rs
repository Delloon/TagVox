use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

/// Хеширует пароль для хранения в panel.toml. Сам пароль после этого нигде
/// не сохраняется — только необратимый хеш со случайной солью.
pub fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("не удалось захэшировать пароль")
        .to_string()
}

/// Проверяет пароль против сохранённого хеша. Возвращает false и на неверный
/// пароль, и на повреждённый/пустой хеш — вызывающий код не должен различать
/// эти случаи (чтобы не давать подсказок при переборе).
pub fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok()
}