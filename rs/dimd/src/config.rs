//! Config: ~/.config/dim-agent/config.toml + .env secrets.
//! Parity with dim/config.py — same paths, same keys, same defaults.
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Cfg {
    pub audio_seconds: u32,
    pub model: String,
    pub answer_model: String,
    pub risk_threshold: f64,
    pub confidence_ambiguous: f64,
    pub allow_shell: bool,
    pub screenshots: bool,
    pub session_turns: usize,
    pub voice_out: bool,
    pub whisper_bin: PathBuf,
    pub whisper_model: PathBuf,
    pub router: String, // jev | chat | off
    pub raw: toml::Value,
}

#[derive(Deserialize, Default)]
struct FileCfg {
    audio: Option<HashMap<String, toml::Value>>,
    agent: Option<HashMap<String, toml::Value>>,
    brain: Option<HashMap<String, toml::Value>>,
}

fn cfg_dir() -> PathBuf {
    dirs_home().join(".config/dim-agent")
}
pub fn data_dir() -> PathBuf {
    dirs_home().join(".local/share/dim-agent")
}
fn runtime_dir() -> PathBuf {
    std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
        .join("dim-agent")
}
pub fn sock_file() -> PathBuf {
    runtime_dir().join("dimd.sock")
}
pub fn state_file() -> PathBuf {
    runtime_dir().join("state.json")
}
fn dirs_home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
}

fn get<'a>(map: &'a Option<HashMap<String, toml::Value>>, key: &str) -> Option<&'a toml::Value> {
    map.as_ref()?.get(key)
}
fn s(v: Option<&toml::Value>, default: &str) -> String {
    v.and_then(|v| v.as_str()).unwrap_or(default).to_string()
}

pub fn load() -> Cfg {
    let path = cfg_dir().join("config.toml");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let f: FileCfg = toml::from_str(&text).unwrap_or_default();
    let home = dirs_home();
    Cfg {
        audio_seconds: get(&f.audio, "seconds")
            .and_then(|v| v.as_integer())
            .unwrap_or(5) as u32,
        model: s(get(&f.agent, "model"), "typesafe/jev-1.13"),
        answer_model: s(
            get(&f.agent, "answer_model"),
            "meta-llama/llama-4-maverick",
        ),
        risk_threshold: get(&f.agent, "risk_threshold")
            .and_then(|v| v.as_float())
            .unwrap_or(1.5),
        confidence_ambiguous: get(&f.agent, "confidence_ambiguous")
            .and_then(|v| v.as_float())
            .unwrap_or(0.8),
        allow_shell: get(&f.agent, "allow_shell")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        screenshots: get(&f.agent, "screenshots")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        session_turns: get(&f.agent, "session_turns")
            .and_then(|v| v.as_integer())
            .unwrap_or(8) as usize,
        voice_out: get(&f.agent, "voice_out")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        whisper_bin: get(&f.agent, "whisper_bin")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("src/whisper.cpp/build/bin/whisper-cli")),
        whisper_model: get(&f.agent, "whisper_model")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                home.join("src/whisper.cpp/models/ggml-small.en.bin")
            }),
        router: s(get(&f.brain, "router"), "jev"),
        raw: toml::from_str(&text).unwrap_or(toml::Value::Table(Default::default())),
    }
}

pub fn api_key() -> Result<String, String> {
    // .env file then process env — same order as Python
    let env_file = cfg_dir().join(".env");
    if let Ok(text) = std::fs::read_to_string(&env_file) {
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("OPENROUTER_API_KEY=") {
                return Ok(v.trim().trim_matches('"').to_string());
            }
        }
    }
    std::env::var("OPENROUTER_API_KEY").map_err(|_| "no OPENROUTER_API_KEY".into())
}
