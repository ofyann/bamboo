use chrono::Local;

fn timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn info(msg: &str) {
    println!("{} \u{001b}[32m[INFO]\u{001b}[0m {}", timestamp(), msg);
}

pub fn warn(msg: &str) {
    eprintln!("{} \u{001b}[33m[WARN]\u{001b}[0m {}", timestamp(), msg);
}

pub fn error(msg: &str) {
    eprintln!("{} \u{001b}[31m[ERROR]\u{001b}[0m {}", timestamp(), msg);
}
