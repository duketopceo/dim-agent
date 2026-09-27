//! Self-authored skills — skills/*/SKILL.md (parity with dim/skills.py).
//! skill_manage / skill_view tools; index_text() injected into brain
//! calls; `tool:`+`tier:` frontmatter registers skill_<name> entries.
use crate::config;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

fn dir() -> PathBuf {
    config::data_dir().join("skills")
}

fn slug(name: &str) -> String {
    let s: String = name.to_lowercase().chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>()
        .join("-");
    let s: String = s.chars().take(48).collect();
    if s.is_empty() { "skill".into() } else { s }
}

fn frontmatter(text: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    if let Some(rest) = text.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---\n") {
            for line in rest[..end].lines() {
                if let Some((k, v)) = line.split_once(':') {
                    m.insert(k.trim().to_string(),
                             v.trim().trim_matches('"').to_string());
                }
            }
        }
    }
    m
}

pub fn index_text() -> String {
    let d = dir();
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&d) {
        for e in rd.flatten() {
            let f = e.path().join("SKILL.md");
            if let Ok(text) = std::fs::read_to_string(&f) {
                let fm = frontmatter(&text);
                let name = fm.get("name").cloned()
                    .unwrap_or_else(|| e.file_name().to_string_lossy().into());
                let desc = fm.get("description").cloned().unwrap_or_default();
                if !desc.is_empty() {
                    out.push(format!("- {name}: {desc}"));
                }
            }
        }
    }
    out.sort();
    out.join("\n")
}

pub fn view(name: &str) -> String {
    let f = dir().join(slug(name)).join("SKILL.md");
    std::fs::read_to_string(&f)
        .unwrap_or_else(|_| format!("FAIL (no skill {name:?})"))
}

/// Tool form: 'op|name|field|body' (field = description | filename)
pub fn run_manage(arg: &str) -> String {
    let mut parts = arg.splitn(4, '|').map(str::trim);
    let op = parts.next().unwrap_or("");
    let name = parts.next().unwrap_or("");
    let field = parts.next().unwrap_or("");
    let body = parts.next().unwrap_or("");
    let d = dir().join(slug(name));
    match op {
        "list" => {
            let mut names: Vec<String> = Vec::new();
            if let Ok(rd) = std::fs::read_dir(dir()) {
                for e in rd.flatten() {
                    if e.path().join("SKILL.md").is_file() {
                        names.push(e.file_name().to_string_lossy().into());
                    }
                }
            }
            if names.is_empty() { "OK (no skills)".into() }
            else { format!("OK {}", names.join(", ")) }
        }
        "create" => {
            if d.join("SKILL.md").exists() {
                return format!("FAIL (skill {:?} exists — use edit)", slug(name));
            }
            std::fs::create_dir_all(&d).ok();
            std::fs::write(d.join("SKILL.md"), format!(
                "---\nname: {}\ndescription: {}\n---\n# {}\n\n{}\n",
                slug(name), if field.is_empty() { "(undescribed)" } else { field },
                slug(name), if body.is_empty() { "TODO" } else { body })).ok();
            format!("OK (created {})", slug(name))
        }
        "edit" => {
            let f = d.join("SKILL.md");
            if !f.exists() {
                return format!("FAIL (no skill {:?} — use create)", slug(name));
            }
            if body.is_empty() {
                return "FAIL (edit needs the full new SKILL.md in body)".into();
            }
            std::fs::write(&f, format!("{}\n", body.trim_end())).ok();
            format!("OK (edited {})", slug(name))
        }
        "delete" => {
            if !d.exists() {
                return format!("FAIL (no skill {:?})", slug(name));
            }
            std::fs::remove_dir_all(&d).ok();
            format!("OK (deleted {})", slug(name))
        }
        "write_file" | "remove_file" => {
            if field.is_empty() || field.contains('/') || field.contains("..") {
                return "FAIL (needs a bare filename)".into();
            }
            let f = d.join(field);
            if op == "write_file" {
                std::fs::create_dir_all(&d).ok();
                std::fs::write(&f, body).ok();
                format!("OK (wrote {}/{field})", slug(name))
            } else if f.exists() {
                std::fs::remove_file(&f).ok();
                format!("OK (removed {}/{field})", slug(name))
            } else {
                format!("FAIL (no file {}/{field})", slug(name))
            }
        }
        _ => "FAIL (op must be create|edit|delete|write_file|remove_file|list)"
            .into(),
    }
}

/// skill_<name> dispatch: run the skill's declared `tool:` script.
/// Returns None when no such skill tool exists.
pub fn run_skill_tool(name: &str, arg: &str) -> Option<(String, &'static str)> {
    let skill = name.strip_prefix("skill_")?;
    let f = dir().join(skill).join("SKILL.md");
    let text = std::fs::read_to_string(&f).ok()?;
    let fm = frontmatter(&text);
    let script = fm.get("tool")?;
    if script.contains('/') || script.contains("..") {
        return None;
    }
    let sp = dir().join(skill).join(script);
    if !sp.is_file() {
        return None;
    }
    let tier = match fm.get("tier").map(|s| s.as_str()) {
        Some("safe") => "safe",
        Some("mutating") => "mutating",
        _ => "shell",
    };
    let out = Command::new("bash").arg(&sp)
        .args(arg.split_whitespace())
        .output();
    let msg = match out {
        Ok(o) => {
            let s = String::from_utf8_lossy(if o.stdout.is_empty() {
                &o.stderr
            } else {
                &o.stdout
            }).trim().chars().take(2000).collect::<String>();
            if s.is_empty() {
                format!("(exit {})", o.status.code().unwrap_or(-1))
            } else {
                s
            }
        }
        Err(e) => format!("ERROR ({e})"),
    };
    Some((msg, tier))
}

/// Risk tier for a skill_<name> tool (shell unless the skill declares
/// lower). Unknown skill tools stay shell — safest default.
pub fn skill_tier(name: &str) -> &'static str {
    if let Some(skill) = name.strip_prefix("skill_") {
        let f = dir().join(skill).join("SKILL.md");
        if let Ok(text) = std::fs::read_to_string(&f) {
            let fm = frontmatter(&text);
            return match fm.get("tier").map(|s| s.as_str()) {
                Some("safe") => "safe",
                Some("mutating") => "mutating",
                _ => "shell",
            };
        }
    }
    "shell"
}
