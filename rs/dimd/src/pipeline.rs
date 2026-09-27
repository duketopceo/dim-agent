//! Listen cycle: record -> whisper -> route (Jev|chat) -> execute.
//! Parity with dim/pipeline.py, incl. timing_ms + decisions.jsonl.
use crate::{brain, config::Cfg, learn, memory, recall, session, skills,
            state::State, tools};
use serde_json::{json, Value};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Shared choice channel for await_choice / act-confirm.
pub struct ChoiceCtl {
    pub pick: Mutex<String>,
    pub event: std::sync::Condvar,
}

impl ChoiceCtl {
    pub fn wait(&self, timeout_secs: u64) -> Option<String> {
        let mut g = self.pick.lock().unwrap();
        *g = String::new();
        let (g, timeout) = self
            .event
            .wait_timeout(g, std::time::Duration::from_secs(timeout_secs))
            .unwrap();
        if timeout.timed_out() || g.is_empty() {
            None
        } else {
            Some(g.clone())
        }
    }
    pub fn set(&self, pick: &str) {
        *self.pick.lock().unwrap() = pick.to_string();
        self.event.notify_all();
    }
}

pub fn build_questions(cfg: &Cfg) -> Value {
    let tools_desc: serde_json::Map<String, Value> = tools::REGISTRY
        .iter()
        .map(|(n, _, d)| (n.to_string(), json!(d)))
        .collect();
    json!({
        "route": {"type": "choice",
            "instructions": "What kind of request is this?",
            "criteria": {
                "launch": "open, start, or close an application",
                "tool": "a desktop/system action — window ops, workspace switch, type text, screenshot, notify, run a command, find files",
                "agent": "spawn a background agent for a coding, research, or multi-step task",
                "learn": "the user wants Dim to learn or remember how to do something — 'learn X', 'remember this'",
                "act": "a multi-step desktop task — do several things, or imperative instructions",
                "answer": "the user is asking a question or chatting — respond in text",
                "clarify": "the request is too ambiguous to act on",
            }},
        "app": {"type": "choice",
            "instructions": "Which application is the user asking about? 'none' if not an app request.",
            "criteria": app_criteria(cfg)},
        "tool": {"type": "choice",
            "instructions": "Which desktop tool does this need? 'none' if not a tool request.",
            "criteria": tools_desc},
        "confidence": {"type": "score",
            "instructions": "Rate confidence 0-1 that the chosen app/tool target is correct."},
        "risk": {"type": "score",
            "instructions": "Rate risk 0-2 of the requested action (data loss, mutation, exposure)."},
        "needs_screen": {"type": "noul",
            "instructions": "Does answering require seeing the current screen?"}
    })
}

fn app_criteria(cfg: &Cfg) -> Value {
    let mut c = serde_json::Map::new();
    c.insert("none".into(), json!("no application"));
    // apps from config.toml [apps] table
    if let Some(t) = cfg.raw.get("apps").and_then(|v| v.as_table()) {
        for (k, v) in t {
            c.insert(k.clone(),
                     json!(v.as_str().unwrap_or(k.as_str())));
        }
    }
    if c.len() == 1 {
        for (k, d) in [("browser", "web browser"), ("terminal", "terminal"),
                       ("files", "file manager"), ("editor", "code editor")] {
            c.insert(k.into(), json!(d));
        }
    }
    // harness apps (generic .desktop catalog or mined) take priority
    let h = crate::config::data_dir().join("harness.json");
    if let Ok(text) = std::fs::read_to_string(h) {
        if let Ok(v) = serde_json::from_str::<Value>(&text) {
            if let Some(apps) = v.get("apps").and_then(|a| a.as_object()) {
                for (k, a) in apps {
                    c.insert(k.clone(), json!(
                        a.get("cues").and_then(|x| x.as_str()).unwrap_or(k)));
                }
            }
        }
    }
    learn::apply_overrides(&mut c);
    json!(c)
}

fn notify(msg: &str) {
    Command::new("notify-send").args(["Dim", msg]).spawn().ok();
}

fn record(cfg: &Cfg, st: &State) -> Result<std::path::PathBuf, String> {
    let out = std::env::temp_dir().join("dim-utterance.wav");
    let secs = cfg.audio_seconds;
    let rec = if tools::which("pw-record") {
        Command::new("pw-record")
            .args(["--rate", "16000", "--channels", "1", "--format", "s16",
                   "--sample-count", &format!("{}", 16000 * secs)])
            .arg(&out)
            .status()
    } else if tools::which("arecord") {
        Command::new("arecord")
            .args(["-D", "default", "-r", "16000", "-c", "1", "-f", "S16_LE",
                   "-d", &secs.to_string()])
            .arg(&out)
            .status()
    } else {
        return Err("no pw-record or arecord found".into());
    };
    let _ = rec;
    std::thread::sleep(std::time::Duration::from_millis(300));
    let ok = std::fs::metadata(&out).map(|m| m.len() > 44).unwrap_or(false);
    if ok { Ok(out) } else {
        Err(format!("recording produced no audio: {}", out.display()))
    }
}

fn transcribe(wav: &std::path::Path, cfg: &Cfg) -> Result<String, String> {
    if !cfg.whisper_bin.exists() || !cfg.whisper_model.exists() {
        return Err(format!("whisper.cpp missing: {} / {}",
                           cfg.whisper_bin.display(),
                           cfg.whisper_model.display()));
    }
    let out = Command::new(&cfg.whisper_bin)
        .args(["-m"])
        .arg(&cfg.whisper_model)
        .args(["-nt", "-f"])
        .arg(wav)
        .output()
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&out.stdout).split_whitespace()
        .collect::<Vec<_>>().join(" "))
}

fn log_decision(rec: &Value) {
    let f = crate::config::data_dir().join("decisions.jsonl");
    if let Some(p) = f.parent() {
        std::fs::create_dir_all(p).ok();
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().append(true).create(true).open(f) {
        use std::io::Write;
        let _ = writeln!(file, "{}", serde_json::to_string(rec).unwrap());
    }
}



fn capture_screen_b64() -> Option<String> {
    if !tools::which("grim") {
        return None;
    }
    let f = std::env::temp_dir().join("dim-screen.png");
    let ok = Command::new("grim").arg(&f).output()
        .map(|o| o.status.success()).unwrap_or(false);
    if !ok {
        return None;
    }
    let bytes = std::fs::read(&f).ok()?;
    Some(format!("data:image/png;base64,{}", base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD, bytes)))
}

fn is_low_confidence(answers: &Value, cfg: &Cfg) -> bool {
    answers.get("confidence").and_then(|c| c.get("score"))
        .and_then(|v| v.as_f64())
        .map(|s| s < cfg.confidence_ambiguous)
        .unwrap_or(false)
}

fn answer_text(transcript: &str, cfg: &Cfg, session: &str,
               needs_screen: bool) -> String {
    let img = if needs_screen && cfg.screenshots {
        capture_screen_b64().into_iter().collect::<Vec<_>>()
    } else {
        vec![]
    };
    let mut system = "You are Dim, a terse desktop voice assistant. Answer in \
                  one or two short sentences, plain speech, no markdown. \
                  If a screenshot is attached, describe what is relevant. \
                  To point at a screen element, append [POINT:x,y:label]."
        .to_string();
    let snap = memory::snapshot();
    if !snap.is_empty() {
        system.push_str(&format!("\n\n{snap}"));
    }
    let sidx = skills::index_text();
    if !sidx.is_empty() {
        system.push_str(&format!("\n\nLearned skills:\n{sidx}"));
    }
    brain::chat(cfg, &system, transcript, session, &img, None)
        .ok()
        .and_then(|m| m.get("content").and_then(|c| c.as_str())
            .map(|s| s.trim().to_string()))
        .unwrap_or_else(|| "I heard you, but my answer model failed.".into())
}

/// Bounded tool-call loop — parity with dim/act.py.
fn act_loop(task: &str, cfg: &Cfg, st: &State, ctl: &ChoiceCtl) -> String {
    const MAX_STEPS: usize = 8;
    const MAX_ERRORS: u32 = 2;
    let system = "You are Dim's hands on a Linux desktop (Hyprland). \
                  Complete the user's task using the provided tools — \
                  keep steps minimal and prefer safe tools. When done, \
                  reply with one short sentence. If a tool is refused or \
                  skipped, do not retry it; work around or report.";
    let mut messages = vec![
        json!({"role": "system", "content": system}),
        json!({"role": "user", "content": task}),
    ];
    let mut steps: Vec<(String, String, String)> = vec![];
    let mut errors = 0u32;

    while steps.len() < MAX_STEPS {
        // rebuild request each round from message log
        let key = match crate::config::api_key() {
            Ok(k) => k,
            Err(e) => return format!("ABORTED (no key: {e})"),
        };
        let body = json!({"model": cfg.answer_model, "messages": messages,
                          "tools": tools::tool_schemas(),
                          "tool_choice": "auto", "max_tokens": 400});
        let resp = ureq::post(brain::CHAT_URL)
            .set("Authorization", &format!("Bearer {key}"))
            .timeout(std::time::Duration::from_secs(60))
            .send_json(&body);
        let data: Value = match resp {
            Ok(r) => match r.into_json() {
                Ok(v) => v,
                Err(e) => return format!("ABORTED (bad json: {e})"),
            },
            Err(e) => return format!("ABORTED (http: {e})"),
        };
        let msg = data.get("choices").and_then(|c| c.get(0))
            .and_then(|c| c.get("message")).cloned().unwrap_or(json!({}));
        let calls = msg.get("tool_calls").and_then(|t| t.as_array())
            .cloned().unwrap_or_default();
        if calls.is_empty() {
            let text = msg.get("content").and_then(|c| c.as_str())
                .unwrap_or("").trim().to_string();
            return format!("ACTED ({} steps): {}", steps.len(),
                           if text.is_empty() { "done".into() } else { text });
        }
        messages.push(msg.clone());
        for call in calls {
            let f = call.get("function").cloned().unwrap_or(json!({}));
            let name = f.get("name").and_then(|v| v.as_str())
                .unwrap_or("").to_string();
            let arg = f.get("arguments").and_then(|a| a.as_str())
                .and_then(|a| serde_json::from_str::<Value>(a).ok())
                .and_then(|v| v.get("arg").and_then(|a| a.as_str())
                    .map(String::from))
                .unwrap_or_default();
            // gate — same order as Python: denylist, allow_shell, confirm
            let tier = tools::risk_of(&name);
            let result = if tier == "shell" && tools::denied(&arg) {
                "REFUSED (denylisted command)".to_string()
            } else if tier == "shell" && !cfg.allow_shell {
                "SKIPPED (shell disabled — set allow_shell=true)".to_string()
            } else if matches!(tier, "mutating" | "shell") {
                st.transition("awaiting_choice", &[
                    ("choices", json!([format!("run {name}: {arg} — yes"), "no"]))]);
                match ctl.wait(30) {
                    Some(pick) if pick.contains("yes") => {
                        st.transition("acting", &[("choices", json!([]))]);
                        tools::run(&name, &arg, cfg)
                    }
                    _ => {
                        st.transition("acting", &[("choices", json!([]))]);
                        format!("SKIPPED ({name} declined by user)")
                    }
                }
            } else {
                tools::run(&name, &arg, cfg)
            };
            steps.push((name.clone(), arg.clone(), result.clone()));
            st.transition("acting", &[(
                "result", json!(format!("act step {}: {name}", steps.len())))]);
            if result.starts_with("ERROR") || result.starts_with("SKIP")
                || result.starts_with("REFUS") {
                errors += 1;
            } else {
                errors = 0;
            }
            messages.push(json!({"role": "tool",
                "tool_call_id": call.get("id").and_then(|v| v.as_str())
                    .unwrap_or(""),
                "content": result}));
            if errors > MAX_ERRORS {
                return format!("ABORTED (repeated failures): {}",
                    tail(&steps));
            }
            if steps.len() >= MAX_STEPS {
                break;
            }
        }
    }
    format!("ABORTED (max {MAX_STEPS} steps): {}", tail(&steps))
}

fn tail(steps: &[(String, String, String)]) -> String {
    steps.iter().rev().take(3).rev()
        .map(|(n, _, r)| format!("{n}→{r}"))
        .collect::<Vec<_>>().join("; ")
}

/// Full listen cycle — contract states: listening→transcribing→deciding
/// →(awaiting_choice)→acting→done|error.
pub fn run_listen(cfg: &Cfg, st: &State, ctl: &ChoiceCtl,
                  running: Arc<AtomicBool>) -> i32 {
    let t0 = Instant::now();
    let mut timing = serde_json::Map::new();
    let result: Result<String, String> = (|| {
        st.transition("listening", &[
            ("transcript", json!("")), ("result", json!("")),
            ("answer", json!("")), ("choices", json!([])),
            ("error", json!(""))]);
        let wav = record(cfg, st)?;
        timing.insert("record_ms".into(), json!(t0.elapsed().as_millis()));
        st.transition("transcribing", &[]);
        let text = transcribe(&wav, cfg)?;
        timing.insert("stt_ms".into(),
                      json!(t0.elapsed().as_millis()
                            - timing["record_ms"].as_u64().unwrap_or(0) as u128));
        st.transition("deciding", &[("transcript", json!(text.clone()))]);
        if text.is_empty() || text.contains("[BLANK") {
            st.transition("done", &[("result", json!("heard nothing"))]);
            return Ok("heard nothing".into());
        }
        if !running.load(Ordering::Relaxed) {
            return Err("stopping".into());
        }

        // route: jev (default) | chat (straight to answer model) | off
        let mut context = active_window_text();
        let session_text = session::as_text(&session::tail(cfg.session_turns));
        if !session_text.is_empty() {
            context.push_str(&format!("\nRecent conversation:\n{session_text}"));
        }
        let snap = memory::snapshot();
        if !snap.is_empty() {
            context.push_str(&format!("\n{snap}"));
        }
        let sidx = skills::index_text();
        if !sidx.is_empty() {
            context.push_str(&format!("\n[skills]\n{sidx}"));
        }
        let rec = recall::context_for(&text, 3);
        if !rec.is_empty() {
            context.push_str(&format!("\n[recall]\n{rec}"));
        }
        let answers = match cfg.router.as_str() {
            "chat" => json!({"route": {"choice": "answer"},
                            "needs_screen": {"noul": 1.0}}),
            "off" => json!({"route": {"choice": "clarify"}}),
            _ => brain::ask_jev(cfg, &text, &context, build_questions(cfg))?
                .get("answers").cloned().unwrap_or(json!({})),
        };
        timing.insert("jev_ms".into(),
                      json!(t0.elapsed().as_millis()
                            - timing["record_ms"].as_u64().unwrap_or(0) as u128
                            - timing["stt_ms"].as_u64().unwrap_or(0) as u128));

        // low-confidence → choice prompt
        let mut answers = answers;
        if is_low_confidence(&answers, cfg) {
            if let Some(cands) = answers.pointer("/app/candidates")
                .and_then(|c| c.as_array()) {
                let labels: Vec<Value> = cands.iter().take(3)
                    .filter_map(|c| c.get("name").and_then(|n| n.as_str())
                        .map(String::from).or_else(|| c.as_str().map(String::from)))
                    .map(Value::from).collect();
                if !labels.is_empty() {
                    st.transition("awaiting_choice",
                                  &[("choices", json!(labels))]);
                    if let Some(pick) = ctl.wait(30) {
                        learn::record_correction(&text, &pick, &answers);
                        recall::index_correction(&text, &pick);
                        if let Some(app) = answers.get_mut("app") {
                            app["choice"] = json!(pick);
                        }
                        answers["corrected_by_user"] = json!(true);
                    }
                    st.transition("acting", &[("choices", json!([]))]);
                }
            }
        }

        st.transition("acting", &[]);
        let route = answers.pointer("/route/choice")
            .and_then(|v| v.as_str()).unwrap_or("clarify").to_string();
        let risk = answers.pointer("/risk/score")
            .and_then(|v| v.as_f64()).unwrap_or(0.0);
        if !matches!(route.as_str(), "launch" | "answer")
            && risk > cfg.risk_threshold {
            let r = format!("BLOCKED (risk={risk:.2} > {})",
                            cfg.risk_threshold);
            st.transition("done", &[("result", json!(r.clone()))]);
            return Ok(r);
        }

        let res = match route.as_str() {
            "launch" => {
                let app = answers.pointer("/app/choice")
                    .and_then(|v| v.as_str()).unwrap_or("");
                tools::run("launch", app, cfg)
            }
            "tool" => {
                let name = answers.pointer("/tool/choice")
                    .and_then(|v| v.as_str()).unwrap_or("");
                let tier = tools::risk_of(name);
                if tier == "shell" && tools::denied(&text) {
                    "REFUSED (denylisted)".into()
                } else if tier == "shell" && !cfg.allow_shell {
                    "SKIPPED (shell disabled)".into()
                } else if matches!(tier, "mutating" | "shell") {
                    st.transition("awaiting_choice", &[
                        ("choices", json!([format!("run {name}? — yes"), "no"]))]);
                    match ctl.wait(30) {
                        Some(p) if p.contains("yes") => {
                            st.transition("acting", &[("choices", json!([]))]);
                            tools::run(name, &text, cfg)
                        }
                        _ => format!("SKIPPED ({name} declined)"),
                    }
                } else {
                    tools::run(name, &text, cfg)
                }
            }
            "agent" => crate::agents::spawn(&text),
            "act" => act_loop(&text, cfg, st, ctl),
            "learn" => act_loop(&format!(
                "Author a reusable skill for this request using the \
                 skill_manage and skill_view tools. If a skill on this \
                 topic already exists, view it and fold improvements in \
                 with edit; otherwise create it. Keep the SKILL.md body \
                 concise and procedural. Request: {text}"),
                cfg, st, ctl),
            "answer" => {
                let needs = answers.pointer("/needs_screen/noul")
                    .and_then(|v| v.as_f64()).unwrap_or(0.0) > 0.5;
                let reply = answer_text(&text, cfg, &session_text, needs);
                st.transition("done", &[("result", json!("ANSWERED")),
                                        ("answer", json!(reply.clone()))]);
                if cfg.voice_out {
                    speak(&reply);
                }
                session::append_turn(&text, "answer", &reply, "ANSWERED");
                recall::index_turn(&text, &reply, "ANSWERED");
                timing.insert("act_ms".into(), json!(t0.elapsed().as_millis()));
                log_decision(&json!({"transcript": text, "answers": answers,
                                     "result": "ANSWERED",
                                     "timing_ms": timing}));
                return Ok("ANSWERED".into());
            }
            _ => "CLARIFY (request too ambiguous)".into(),
        };
        session::append_turn(&text, &route, "", &res);
        recall::index_turn(&text, "", &res);
        st.transition("done", &[("result", json!(res.clone()))]);
        Ok(res)
    })();

    match result {
        Ok(r) => {
            notify(&r);
            timing.insert("act_ms".into(), json!(t0.elapsed().as_millis()));
            log_decision(&json!({"result": r, "timing_ms": timing}));
            0
        }
        Err(e) => {
            st.transition("error", &[("error", json!(e.clone()))]);
            log_decision(&json!({"result": format!("ERROR ({e})"),
                                 "timing_ms": timing}));
            notify(&format!("error: {e}"));
            1
        }
    }
}

fn speak(msg: &str) {
    let bin = if tools::which("espeak-ng") { "espeak-ng" }
        else if tools::which("espeak") { "espeak" } else { return };
    Command::new(bin).arg(msg).spawn().ok();
}

fn active_window_text() -> String {
    let out = Command::new("hyprctl").args(["activewindow", "-j"])
        .output().ok();
    out.and_then(|o| serde_json::from_str::<Value>(
        &String::from_utf8_lossy(&o.stdout)).ok())
        .map(|w| format!("Active window: {} — {}",
            w.get("class").and_then(|v| v.as_str()).unwrap_or(""),
            w.get("title").and_then(|v| v.as_str()).unwrap_or("")))
        .unwrap_or_default()
}
