use std::time::SystemTime;

fn timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let datetime = time::OffsetDateTime::from_unix_timestamp(secs as i64).unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    datetime.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| secs.to_string())
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
