use std::env;

use once_cell::sync::OnceCell;

static DEBUG: OnceCell<bool> = OnceCell::new();
static SECRET: OnceCell<String> = OnceCell::new();
static NOT_INIT: &str = "not initialized";

fn env_bool<T: AsRef<str>>(s: T) -> bool {
    let s = s.as_ref().trim();
    !(s.is_empty() || s == "0" || s.to_ascii_lowercase() == "false")
}

pub fn debug() -> bool {
    *DEBUG.get_or_init(|| env::var("DEBUG").map(env_bool).unwrap_or(false))
}

pub fn secret() -> &'static str {
    &*SECRET.get_or_init(|| env::var("SECRET").unwrap())
}
