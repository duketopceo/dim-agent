//! Deep recall — sqlite FTS5 store at recall.db (parity with
//! dim/recall.py). Write-through on turns + corrections; lexical FTS5
//! search needs no keys. rusqlite is bundled so recall.db is
//! byte-compatible with the Python core's.
use crate::config;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

fn db_path() -> PathBuf {
    config::data_dir().join("recall.db")
}

fn db(path: Option<&Path>) -> Option<Connection> {
    let p = path.map(PathBuf::from).unwrap_or_else(db_path);
    p.parent().map(|d| std::fs::create_dir_all(d).ok());
    let c = Connection::open(&p).ok()?;
    c.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS notes \
         USING fts5(kind, body, ts, tokenize='porter');").ok()?;
    Some(c)
}

pub fn add(kind: &str, body: &str) {
    if body.trim().is_empty() {
        return;
    }
    if let Some(c) = db(None) {
        c.execute(
            "INSERT INTO notes (kind, body, ts) VALUES (?1, ?2, ?3)",
            (kind, &body[..body.len().min(4000)],
             crate::state::now_iso())).ok();
    }
}

pub fn index_turn(transcript: &str, reply: &str, result: &str) {
    add("turn", &format!("user: {transcript}\ndim: {}",
                         if reply.is_empty() { result } else { reply }));
}

pub fn index_correction(heard: &str, picked: &str) {
    add("correction", &format!("heard {heard} -> picked {picked}"));
}

pub fn search(query: &str, k: usize) -> Vec<(String, String, String)> {
    let q: String = query.split_whitespace()
        .filter(|w| w.chars().all(|c| c.is_alphanumeric()))
        .collect::<Vec<_>>().join(" ");
    if q.is_empty() {
        return vec![];
    }
    let Some(c) = db(None) else { return vec![] };
    let mut st = match c.prepare(
        "SELECT kind, body, ts FROM notes WHERE notes MATCH ?1 \
         ORDER BY rank LIMIT ?2") {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    st.query_map((&q, k as i64), |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?,
            r.get::<_, String>(2)?))
    }).map(|rows| rows.flatten().collect()).unwrap_or_default()
}

/// Tool form: 'search <query>' or bare '<query>'
pub fn run(arg: &str) -> String {
    let q = arg.strip_prefix("search ").unwrap_or(arg);
    let hits = search(q, 5);
    if hits.is_empty() {
        return format!("no recall hits for {q:?}");
    }
    hits.iter()
        .map(|(k, b, t)| format!("[{} {}] {}", k, &t[..10.min(t.len())], b))
        .collect::<Vec<_>>().join("\n")
}

pub fn context_for(transcript: &str, k: usize) -> String {
    search(transcript, k).iter().map(|(_, b, _)| b.clone())
        .collect::<Vec<_>>().join("\n")
}
