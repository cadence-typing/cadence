//! #76: `config.toml` parsing, diagnostics, precedence and rewrites.
use std::path::Path;

use crate::config::{Config, KEYS, Setting, Source, path, set};

fn parse(text: &str) -> Result<Config, Vec<crate::config::ConfigError>> {
    Config::parse(text, Path::new("/cfg/config.toml"))
}

fn ok(table: &str, key: &str, value: &str) -> bool {
    parse(&format!("[{table}]\n{key} = {value}\n")).is_ok()
}

#[test]
fn every_key_accepts_its_edges_and_refuses_one_past() {
    for minutes in ["5", "10", "15", "20", "30"] {
        assert!(ok("session", "minutes", minutes));
    }
    for minutes in ["4", "6", "31", "0", "\"15\""] {
        assert!(!ok("session", "minutes", minutes), "{minutes}");
    }
    assert!(ok("practice", "accuracy_target", "90"));
    assert!(ok("practice", "accuracy_target", "100"));
    assert!(!ok("practice", "accuracy_target", "89"));
    assert!(!ok("practice", "accuracy_target", "101"));
    for (table, key) in [
        ("keyboard", "numpad"),
        ("practice", "live_pace"),
        ("display", "high_contrast"),
    ] {
        assert!(ok(table, key, "true") && ok(table, key, "false"));
        assert!(!ok(table, key, "1") && !ok(table, key, "\"yes\""));
    }
    for (table, key, good, bad) in [
        ("session", "emphasis", "\"accuracy\"", "\"fast\""),
        ("keyboard", "layout", "\"uk\"", "\"dvorak\""),
        ("keyboard", "decimal_separator", "\",\"", "\";\""),
        ("content", "breadth", "\"broad\"", "\"wide\""),
        ("display", "text_size", "\"large\"", "\"huge\""),
        ("display", "key_guide", "\"off\"", "\"never\""),
        ("display", "theme", "\"light\"", "\"neon\""),
    ] {
        assert!(ok(table, key, good), "{key} = {good}");
        assert!(!ok(table, key, bad), "{key} = {bad}");
    }
    assert!(ok("display", "theme", "\"ocean.toml\""));
    assert!(!ok("display", "theme", "\"../ocean.toml\""));
    assert!(ok("session", "tracks", r#"["words", "prose"]"#));
    assert!(!ok("session", "tracks", "[]"));
    assert!(!ok("session", "tracks", r#"["words", "words"]"#));
    assert!(!ok("session", "tracks", r#"["typing"]"#));
    assert!(ok("content", "packs", r#"["standard", "rust-std"]"#));
    assert!(!ok("content", "packs", r#"["Rust"]"#));
    // Every key's default is valid.
    let defaults = Config::default();
    assert!(KEYS.iter().all(|k| defaults.get(k.name).is_some()));
}

#[test]
fn an_unknown_key_or_wrong_type_names_file_line_key_and_range() {
    let errors = parse("# my settings\n[session]\nminutes = 15\ncolour = \"blue\"\n").unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, 4);
    assert_eq!(errors[0].key, "session.colour");
    assert_eq!(
        errors[0].to_string(),
        "/cfg/config.toml:4: session.colour: unknown key"
    );
    let errors = parse("[session]\nminutes = \"fifteen\"\n").unwrap_err();
    assert_eq!(errors[0].line, 2);
    assert!(
        errors[0].message.contains("one of 5, 10, 15, 20, 30"),
        "{}",
        errors[0]
    );
    let errors = parse("[session\n").unwrap_err();
    assert!(errors[0].message.contains("not valid TOML"));
}

#[test]
fn a_partial_file_defaults_the_rest_and_flags_win() {
    let mut config = parse("[display]\ntheme = \"light\"\n").unwrap();
    assert_eq!(config.text("display.theme"), Some("light"));
    assert_eq!(config.source("display.theme"), Some(Source::File));
    assert_eq!(config.integer("session.minutes"), Some(15));
    assert_eq!(config.source("session.minutes"), Some(Source::Default));
    config.flag("display.theme", Setting::Text("terminal".into()));
    assert_eq!(config.text("display.theme"), Some("terminal"));
    assert_eq!(config.source("display.theme"), Some(Source::Flag));
}

#[test]
fn no_file_is_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::load(&dir.path().join("config.toml")).unwrap();
    assert_eq!(config, Config::default());
}

#[test]
fn a_rewrite_keeps_comments_and_changes_one_value() {
    let text = "# Cadence settings\n[session]\n# how long\nminutes = 15   # usual\n\n[display]\ntheme = \"dark\"\n";
    let rewritten = set(text, "session.minutes", "20").unwrap();
    assert_eq!(rewritten, text.replace("minutes = 15", "minutes = 20"));
    assert!(set(text, "session.minutes", "21").is_err());
    assert!(set(text, "session.nope", "1").is_err());
    let added = set(text, "display.high_contrast", "true").unwrap();
    assert!(
        added.starts_with("# Cadence settings\n[session]\n# how long\nminutes = 15   # usual\n")
    );
    assert!(
        parse(&added)
            .unwrap()
            .boolean("display.high_contrast")
            .unwrap()
    );
}

#[test]
fn the_path_follows_xdg_and_ignores_relative_values() {
    use std::ffi::OsStr;
    assert_eq!(
        path(Some(OsStr::new("/x/conf")), Some(OsStr::new("/home/a"))),
        Some(Path::new("/x/conf/cadence/config.toml").to_path_buf())
    );
    assert_eq!(
        path(Some(OsStr::new("relative")), Some(OsStr::new("/home/a"))),
        Some(Path::new("/home/a/.config/cadence/config.toml").to_path_buf())
    );
    assert_eq!(path(None, None), None);
}
