use std::time::SystemTime;

fn timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    match time::OffsetDateTime::from_unix_timestamp(secs as i64) {
        Ok(datetime) => {
            const FORMAT: &[time::format_description::FormatItem<'_>] =
                time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
            datetime.format(&FORMAT).unwrap_or_else(|_| secs.to_string())
        }
        Err(_) => secs.to_string(),
    }
}

pub fn info(msg: &str) {
    println!("{} [INFO] {}", timestamp(), msg);
}

pub fn warn(msg: &str) {
    eprintln!("{} [WARN] {}", timestamp(), msg);
}

pub fn error(msg: &str) {
    eprintln!("{} [ERROR] {}", timestamp(), msg);
}
