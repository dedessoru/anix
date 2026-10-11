use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output},
};

fn invoke(workspace: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_anix"))
        .arg("--workspace")
        .arg(workspace)
        .args(args)
        .output()
        .unwrap()
}
fn successful(workspace: &Path, args: &[&str]) -> Value {
    let output = invoke(workspace, args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn canonical_help_and_legacy_workspace_migration() {
    let temp = tempfile::tempdir().unwrap();
    let w = temp.path();
    let help = invoke(w, &["--help"]);
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.contains("ANIX") && help.contains("anix"));
    assert!(!help.contains("Researcher's Notes"));
    let project = successful(w, &["project", "create", "--name", "Historical research"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    std::fs::rename(w.join(".anix"), w.join(".rn")).unwrap();
    assert_eq!(successful(w, &["project", "list"])[0]["id"], project);
    assert!(w.join(".anix/state.sqlite").exists() && !w.join(".rn").exists());
    assert_eq!(
        successful(w, &["--project", &project, "research", "status"])["control"]["owner_id"],
        "cli-user"
    );
}
#[test]
fn cli_uses_persistent_application_services() {
    let temp = tempfile::tempdir().unwrap();
    let w = temp.path();
    let created = successful(w, &["project", "create", "--name", "CLI study"]);
    let p = created["id"].as_str().unwrap();
    assert_eq!(successful(w, &["project", "list"])[0]["id"], p);
    let q = successful(
        w,
        &["--project", p, "question", "add", "--title", "Question?"],
    );
    let qid = q["id"].as_str().unwrap();
    let h = successful(
        w,
        &[
            "--project",
            p,
            "hypothesis",
            "create",
            "--question",
            qid,
            "--title",
            "H1",
            "--statement",
            "A improves B",
        ],
    );
    let hid = h["id"]["id"].as_str().unwrap();
    assert_eq!(
        successful(w, &["--project", p, "hypothesis", "show", hid])["statement"],
        "A improves B"
    );
    let result = successful(
        w,
        &[
            "--project",
            p,
            "agent",
            "demo-experiment",
            "--hypothesis",
            hid,
            "--expected-version",
            "1",
            "--title",
            "E1",
            "--objective",
            "Measure B",
            "--methodology",
            "Control comparison",
            "--protocol",
            "100 trials",
        ],
    );
    let gate = result["submission"]["Gate"].as_str().unwrap();
    assert_eq!(
        successful(w, &["--project", p, "gate", "list"])[0]["id"],
        gate
    );
    successful(w, &["--project", p, "gate", "approve", gate]);
    assert_eq!(
        successful(w, &["--project", p, "gate", "show", gate])["status"],
        "approved"
    );
    let state = successful(w, &["--project", p, "research-state"]);
    assert_eq!(state["experiments"].as_array().unwrap().len(), 1);
    assert!(state["active_runs"].as_array().unwrap().is_empty());
    assert!(
        !invoke(w, &["question", "add", "--title", "missing project"])
            .status
            .success()
    );
    assert!(
        !invoke(w, &["--project", p, "hypothesis", "show", "invalid-id"])
            .status
            .success()
    );
    assert!(!invoke(w, &["unknown-command"]).status.success());
}

#[test]
fn human_control_cli_is_json_and_persistent() {
    let temp = tempfile::tempdir().unwrap();
    let w = temp.path();
    let p = successful(w, &["project", "create", "--name", "Control CLI"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    successful(w, &["--project", &p, "research", "autonomy", "manual"]);
    assert_eq!(
        successful(w, &["--project", &p, "research", "status"])["control"]["autonomy"]["mode"],
        "manual"
    );
    successful(
        w,
        &[
            "--project",
            &p,
            "research",
            "budget-set",
            "--max-steps",
            "150",
        ],
    );
    assert_eq!(
        successful(w, &["--project", &p, "research", "budget-show"])["max_steps"],
        150
    );
    successful(w, &["--project", &p, "research", "pause"]);
    assert_eq!(
        successful(w, &["--project", &p, "research", "status"])["control"]["status"],
        "paused"
    );
}
