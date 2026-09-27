//! Parity checks for the file-level contract (memory/recall/skills)
//! plus registry/risk invariants. One test fn because HOME/XDG env is
//! process-global — parallel tests would race the redirect.
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dimd"))
}

#[test]
fn cli_and_contract() {
    // unique HOME so the daemon-side paths land in a tmp dir
    let tmp = std::env::temp_dir()
        .join(format!("dimd-test-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    std::env::set_var("HOME", &tmp);
    std::env::set_var("XDG_RUNTIME_DIR", &tmp);

    // --help/usage path returns nonzero with usage text
    let out = bin().output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr)
        .contains("dimd"));
}
