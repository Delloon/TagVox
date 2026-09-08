use std::{
    env, fs,
    path::{Path, PathBuf},
};

/// OUT_DIR обычно выглядит как `.../target/<profile>/build/<pkg>-<hash>/out`.
/// Поднимаемся на 3 уровня вверх, чтобы получить `.../target/<profile>` —
/// именно там лежит итоговый .exe после `cargo build`.
fn target_dir_from_out_dir(out_dir: &Path) -> Option<PathBuf> {
    out_dir.ancestors().nth(3).map(|p| p.to_path_buf())
}

/// Копирует файл, только если его ещё нет по месту назначения — чтобы не
/// затереть то, что пользователь уже отредактировал рядом с exe.
fn copy_if_missing(src: &Path, dst: &Path) {
    if dst.exists() {
        return;
    }
    if let Some(parent) = dst.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match fs::copy(src, dst) {
        Ok(_) => println!(
            "cargo:warning=[автокопирование] {} -> {}",
            src.display(),
            dst.display()
        ),
        Err(e) => println!(
            "cargo:warning=[автокопирование] не удалось скопировать {}: {e}",
            src.display()
        ),
    }
}

/// Рекурсивно копирует директорию, пропуская файлы, которые в месте
/// назначения уже существуют (не перезаписывает то, что пользователь сам
/// положил или изменил в папке рядом с exe).
fn copy_dir_if_missing(src: &Path, dst: &Path) {
    let Ok(entries) = fs::read_dir(src) else {
        return;
    };
    let _ = fs::create_dir_all(dst);
    for entry in entries.flatten() {
        let path = entry.path();
        let dest_path = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_if_missing(&path, &dest_path);
        } else {
            copy_if_missing(&path, &dest_path);
        }
    }
}

fn main() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR не задан"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR не задан"));

    let Some(target_dir) = target_dir_from_out_dir(&out_dir) else {
        println!(
            "cargo:warning=Не удалось определить папку target/<profile> из OUT_DIR, автокопирование пропущено."
        );
        return;
    };

    // phrases.json — обязательный файл настроек бота
    let phrases_src = manifest_dir.join("phrases.json");
    if phrases_src.exists() {
        copy_if_missing(&phrases_src, &target_dir.join("phrases.json"));
    }

    // config.json — необязателен (бот сам создаст его при первом запуске),
    // но если рядом с исходниками лежит свой готовый config.json — скопируем и его
    let config_src = manifest_dir.join("config.json");
    if config_src.exists() {
        copy_if_missing(&config_src, &target_dir.join("config.json"));
    }

    // Вся папка sounds/ со всем содержимым
    let sounds_src = manifest_dir.join("sounds");
    if sounds_src.exists() {
        copy_dir_if_missing(&sounds_src, &target_dir.join("sounds"));
    }

    // Перезапускать этот build-скрипт нужно только если сами исходники
    // настроек изменились — иначе обычная пересборка кода будет лишний раз
    // трогать файловую систему без необходимости.
    println!("cargo:rerun-if-changed=phrases.json");
    println!("cargo:rerun-if-changed=config.json");
    println!("cargo:rerun-if-changed=sounds");
}