//! Curated memory — MEMORY.md + USER.md (parity with dim/memory.py).
//! Bounded files injected frozen into every brain call; `memory` tool
//! curates via add|replace|remove, substring-matched bullets.
use crate::config;
use std::path::PathBuf;

const MEMORY_BUDGET: usize = 800 * 4;
const USER_BUDGET: usize = 500 * 4;

fn file_for(target: &str) -> Option<(PathBuf, &'static str, usize)> {
    match target {
        "memory" => Some((config::data_dir().join("MEMORY.md"),
                          "# Memory\n", MEMORY_BUDGET)),
        "user" => Some((config::data_dir().join("USER.md"),
                        "# User\n", USER_BUDGET)),
        _ => None,
    }
}

pub fn edit(target: &str, op: &str, old: &str, new: &str) -> String {
    let Some((path, head, budget)) = file_for(target) else {
        return format!("FAIL (target must be 'memory' or 'user', got {target:?})");
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| head.into());
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    match op {
        "add" => {
            let fact = if new.is_empty() { old } else { new };
            if fact.is_empty() {
                return "FAIL (add needs a fact)".into();
            }
            let b = if fact.trim_start().starts_with('-') {
                fact.to_string()
            } else {
                format!("- {fact}")
            };
            lines.push(b);
        }
        "replace" | "remove" => {
            let idx: Vec<usize> = lines.iter().enumerate()
                .filter(|(_, l)| l.trim_start().starts_with('-')
                        && !old.is_empty() && l.contains(old))
                .map(|(i, _)| i).collect();
            if idx.is_empty() {
                return format!("FAIL (no line containing {old:?})");
            }
            if op == "replace" {
                let b = if new.trim_start().starts_with('-') {
                    new.to_string()
                } else {
                    format!("- {new}")
                };
                for i in idx { lines[i] = b.clone(); }
            } else {
                let drop: std::collections::HashSet<usize> =
                    idx.into_iter().collect();
                lines = lines.into_iter().enumerate()
                    .filter(|(i, _)| !drop.contains(i))
                    .map(|(_, l)| l).collect();
            }
        }
        _ => return format!("FAIL (op must be add|replace|remove, got {op:?})"),
    }
    let out = lines.join("\n").trim_end().to_string() + "\n";
    if out.len() > budget {
        return format!("FAIL (over budget: {} chars > {budget})",
                       out.len());
    }
    if let Some(p) = path.parent() { std::fs::create_dir_all(p).ok(); }
    match std::fs::write(&path, out) {
        Ok(_) => format!("OK ({target} {op})"),
        Err(e) => format!("FAIL ({e})"),
    }
}

/// Tool form: 'target|op|old|new'
pub fn run(arg: &str) -> String {
    let mut parts = arg.splitn(4, '|').map(str::trim);
    let (t, op) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let (old, new) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    edit(t, op, old, new)
}

/// Frozen context block injected into every brain call.
pub fn snapshot() -> String {
    let mut parts = Vec::new();
    for (path, label) in [
        (config::data_dir().join("MEMORY.md"), "memory"),
        (config::data_dir().join("USER.md"), "user"),
    ] {
        if let Ok(text) = std::fs::read_to_string(&path) {
            let body: Vec<&str> = text.lines()
                .filter(|l| !l.trim().is_empty() && !l.starts_with("# "))
                .collect();
            if !body.is_empty() {
                parts.push(format!("[{label}]\n{}", body.join("\n")));
            }
        }
    }
    parts.join("\n\n")
}
