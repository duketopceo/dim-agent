//! session.jsonl — persistent turns for follow-up context.
//! Parity with dim/session.py.
use serde_json::{json, Value};
use std::path::PathBuf;

fn file() -> PathBuf {
    crate::config::data_dir().join("session.jsonl")
}

pub fn append_turn(transcript: &str, route: &str, reply: &str, result: &str) {
    let f = file();
    if let Some(p) = f.parent() {
        std::fs::create_dir_all(p).ok();
    }
    if let Ok(mut fh) = std::fs::OpenOptions::new().append(true).create(true).open(&f) {
        use std::io::Write;
        let _ = writeln!(fh, "{}", json!({
            "ts": crate::state::now_iso(),
            "transcript": transcript, "route": route,
            "reply": reply, "result": result,
        }));
    }
}

pub fn tail(n: usize) -> Vec<Value> {
    let lines: Vec<Value> = std::fs::read_to_string(file())
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    lines.into_iter().rev().take(n).rev().collect()
}

pub fn as_text(turns: &[Value]) -> String {
    turns.iter()
        .map(|t| {
            format!("- {} [{}] {}",
                t.get("transcript").and_then(|v| v.as_str()).unwrap_or(""),
                t.get("route").and_then(|v| v.as_str()).unwrap_or(""),
                t.get("reply").or_else(|| t.get("result"))
                    .and_then(|v| v.as_str()).unwrap_or(""))
        })
        .collect::<Vec<_>>()
        .join("\n")
}
