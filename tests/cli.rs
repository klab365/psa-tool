use std::process::{Command, Output};
use tempfile::TempDir;

fn psa(home: &TempDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_psa"))
        .args(args)
        .env("HOME", home.path())
        .output()
        .expect("PSA CLI starts")
}

fn stdout(output: Output) -> String {
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("UTF-8 output")
}

#[test]
fn local_entry_workflow_uses_shared_home_directory() {
    let home = tempfile::tempdir().expect("temporary HOME");

    let output = psa(
        &home,
        &[
            "config",
            "set",
            "environmentUrl",
            "https://example.crm4.dynamics.com",
        ],
    );
    assert!(stdout(output).contains("environmentUrl = https://example.crm4.dynamics.com"));

    let output = psa(
        &home,
        &[
            "add",
            "--date",
            "2026-09-01",
            "--hours",
            "7.5",
            "--description",
            "Integrationstest",
            "--project-name",
            "Projekt A",
            "--task-name",
            "Task A",
        ],
    );
    assert!(stdout(output).contains("Eintrag #1 erfasst."));

    let output = psa(&home, &["list"]);
    let output = stdout(output);
    assert!(output.contains("#1"));
    assert!(output.contains("2026-09-01"));
    assert!(output.contains("7.5h"));
    assert!(output.contains("Projekt A"));
    assert!(output.contains("Task A"));
    assert!(output.contains("offen"));

    let output = psa(&home, &["week", "2026-09-01"]);
    let output = stdout(output);
    assert!(output.contains("Woche 2026-08-31 – 2026-09-06"));
    assert!(output.contains("Summe: 7.5h"));

    // No credentials are configured: this proves dry-run does not contact
    // Dataverse and does not modify the pending entry.
    let sync_output = stdout(psa(&home, &["sync", "--dry-run"]));
    assert!(sync_output.contains("[dry-run] CREATE 2026-09-01 | Projekt A | 7.5h"));
    assert!(sync_output.contains("Dry-run fertig: 1 erstellen"));
    assert!(stdout(psa(&home, &["list"])).contains("offen"));

    let output = psa(&home, &["remove", "1"]);
    assert!(stdout(output).contains("Eintrag #1 zum Löschen vorgemerkt."));
    assert!(!stdout(psa(&home, &["list"])).contains("#1 "));
}

#[test]
fn config_show_uses_defaults_without_creating_a_config_file() {
    let home = tempfile::tempdir().expect("temporary HOME");
    let output = stdout(psa(&home, &["config", "show"]));

    assert!(output.contains("\"environmentUrl\": \"\""));
    assert!(output.contains("\"timeEntryEntitySet\": \"msdyn_timeentries\""));
    assert!(!home.path().join(".psa-tool/config.json").exists());
}

#[test]
fn doctor_reports_configuration_and_login_issues() {
    let home = tempfile::tempdir().expect("temporary HOME");
    let output = psa(&home, &["doctor"]);
    assert!(!output.status.success());
    let out = String::from_utf8(output.stdout).expect("UTF-8 output");
    assert!(out.contains("✘ environmentUrl gesetzt"));
    assert!(out.contains("✘ resourceId gesetzt"));
    assert!(out.contains("✘ Angemeldet"));
}
