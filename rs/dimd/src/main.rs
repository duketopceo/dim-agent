//! dimd — Dim companion core daemon (Rust parity port).
//! Contract: docs/IPC_CONTRACT.md. CLI mirrors the Python `dimd`.
mod agents;
mod brain;
mod config;
mod harness;
mod ipc;
mod learn;
mod pipeline;
mod session;
mod state;
mod tools;

use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn main() {
    let cmd = std::env::args().nth(1).unwrap_or_default();
    let code = match cmd.as_str() {
        "daemon" => daemon(),
        "trigger" => client(&json!({"cmd": "listen"}), true),
        "status" => client(&json!({"cmd": "status"}), false),
        "stop" => client(&json!({"cmd": "stop"}), false),
        "choice" => {
            let pick = std::env::args().nth(2).unwrap_or_default();
            client(&json!({"cmd": "choice", "pick": pick}), false)
        }
        "task_status" => {
            let n = std::env::args().nth(2).unwrap_or_default();
            client(&json!({"cmd": "task_status", "name": n}), false)
        }
        "task_cancel" => {
            let n = std::env::args().nth(2).unwrap_or_default();
            client(&json!({"cmd": "task_cancel", "name": n}), false)
        }
        "learn" => {
            let p = learn::weekly(7);
            if p.is_empty() { println!("nothing to propose"); }
            else { println!("{p}"); }
            0
        }
        "harness" => match harness::build() {
            Ok(s) => { println!("{s}"); 0 }
            Err(e) => { eprintln!("{e}"); 1 }
        }
        _ => {
            eprintln!("usage: dimd [daemon|trigger|status|stop|choice|task_status|task_cancel]");
            2
        }
    };
    std::process::exit(code);
}

fn client(cmd: &Value, spawn_if_down: bool) -> i32 {
    let sock = config::sock_file();
    if !ipc::alive(&sock) {
        if !spawn_if_down {
            eprintln!("dimd not running");
            return 1;
        }
        // inline fallback: run the listen cycle in-process (Python parity)
        let cfg = config::load();
        let st = state::State::new(config::state_file());
        let ctl = pipeline::ChoiceCtl {
            pick: std::sync::Mutex::new(String::new()),
            event: std::sync::Condvar::new(),
        };
        let running = Arc::new(AtomicBool::new(true));
        return pipeline::run_listen(&cfg, &st, &ctl, running);
    }
    match ipc::send(&sock, cmd) {
        Ok(resp) => {
            if let Some(s) = resp.get("state") {
                println!("{}", serde_json::to_string_pretty(s).unwrap());
            } else if let Some(r) = resp.get("result") {
                println!("{}", r.as_str().unwrap_or(""));
            } else {
                println!("{}", serde_json::to_string(&resp).unwrap());
            }
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn daemon() -> i32 {
    let cfg = config::load();
    let st = Arc::new(state::State::new(config::state_file()));
    let running = Arc::new(AtomicBool::new(true));
    let busy = Arc::new(AtomicBool::new(false));
    let ctl = Arc::new(pipeline::ChoiceCtl {
        pick: std::sync::Mutex::new(String::new()),
        event: std::sync::Condvar::new(),
    });

    let st_h = st.clone();
    let running_h = running.clone();
    let busy_h = busy.clone();
    let ctl_h = ctl.clone();
    let cfg_h2 = Arc::new(std::sync::Mutex::new(cfg));
    let cfg_h = cfg_h2.clone();

    let handler = Arc::new(move |cmd: Value| -> Value {
        match cmd.get("cmd").and_then(|c| c.as_str()).unwrap_or("") {
            "status" => json!({"ok": true, "state": st_h.snapshot()}),
            "listen" => {
                if busy_h.swap(true, Ordering::SeqCst) {
                    return json!({"ok": false, "error": "busy"});
                }
                let st2 = st_h.clone();
                let cfg2 = cfg_h.clone();
                let ctl2 = ctl_h.clone();
                let running2 = running_h.clone();
                let busy2 = busy_h.clone();
                std::thread::spawn(move || {
                    let cfg_guard = cfg2.lock().unwrap().clone();
                    pipeline::run_listen(&cfg_guard, &st2, &ctl2, running2);
                    busy2.store(false, Ordering::SeqCst);
                });
                json!({"ok": true})
            }
            "choice" => {
                let pick = cmd.get("pick").and_then(|v| v.as_str()).unwrap_or("");
                ctl_h.set(pick);
                json!({"ok": true})
            }
            "task_status" => {
                let n = cmd.get("name").and_then(|v| v.as_str()).unwrap_or("");
                json!({"ok": true, "result": agents::status(n)})
            }
            "task_cancel" => {
                let n = cmd.get("name").and_then(|v| v.as_str()).unwrap_or("");
                json!({"ok": true, "result": agents::cancel(n)})
            }
            "learn" => {
                let p = learn::weekly(7);
                json!({"ok": true, "result": if p.is_empty() {
                    "nothing to propose".into() } else { p }})
            }
            "harness" => match harness::build() {
                Ok(s) => json!({"ok": true, "result": s}),
                Err(e) => json!({"ok": false, "error": e}),
            },
            "config" => {
                if let Some(set) = cmd.get("set").and_then(|v| v.as_object()) {
                    for (k, v) in set {
                        let (sec, key) = k.split_once('.').unwrap_or(("", ""));
                        if sec.is_empty() || key.is_empty() {
                            return json!({"ok": false,
                                "error": "config keys must be section.key"});
                        }
                        config::set_config(sec, key,
                            v.as_str().unwrap_or(&v.to_string()));
                    }
                    *cfg_h2.lock().unwrap() = config::load();
                }
                json!({"ok": true,
                       "config": cfg_h2.lock().unwrap().raw})
            }
            "stop" => {
                running_h.store(false, Ordering::SeqCst);
                json!({"ok": true})
            }
            c => json!({"ok": false, "error": format!("unknown cmd {c:?}")}),
        }
    });

    println!("dimd-rs listening on {}", config::sock_file().display());
    if let Err(e) = ipc::serve(config::sock_file(), handler, running.clone()) {
        eprintln!("serve error: {e}");
        return 1;
    }
    0
}
