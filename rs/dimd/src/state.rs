//! state.json — atomic publish per IPC_CONTRACT.md.
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::sync::Mutex;

pub struct State {
    file: PathBuf,
    inner: Mutex<Map<String, Value>>,
    pub started_at: String,
}

impl State {
    pub fn new(file: PathBuf) -> Self {
        let st = State {
            file,
            inner: Mutex::new(Map::new()),
            started_at: now(),
        };
        st.transition("idle", &[]);
        st
    }

    pub fn transition(&self, status: &str, fields: &[(&str, Value)]) {
        let mut g = self.inner.lock().unwrap();
        g.insert("status".into(), json!(status));
        for (k, v) in fields {
            g.insert((*k).into(), v.clone());
        }
        for k in ["transcript", "answer", "result", "error"] {
            g.entry(k).or_insert(json!(""));
        }
        g.entry("choices").or_insert(json!([]));
        g.entry("level").or_insert(json!(0.0));
        g.entry("tasks").or_insert(json!({}));
        g.insert("started_at".into(), json!(self.started_at));
        let snap = g.clone();
        drop(g);
        write_atomic(&self.file, &snap);
    }

    pub fn snapshot(&self) -> Value {
        json!(self.inner.lock().unwrap().clone())
    }

    pub fn status(&self) -> String {
        self.inner
            .lock()
            .unwrap()
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("idle")
            .to_string()
    }
}

pub fn now_iso() -> String {
    now()
}

fn now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    // ISO-8601 without chrono: seconds → civil date
    iso8601(secs)
}

fn iso8601(secs: u64) -> String {
    let days = secs / 86400;
    let rem = secs % 86400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (y, mo, d) = civil(days as i64);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}
fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn write_atomic(file: &PathBuf, v: &Map<String, Value>) {
    if let Some(p) = file.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    let tmp = file.with_extension("tmp");
    if std::fs::write(&tmp, serde_json::to_string(v).unwrap()).is_ok() {
        let _ = std::fs::rename(&tmp, file);
    }
}
