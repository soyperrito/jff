use std::fs;
use std::path::Path;
use std::process::Output;

use assert_cmd::Command;
use tempfile::TempDir;

fn run(home: &Path, args: &[&str]) -> Output {
    Command::cargo_bin("jff")
        .expect("jff binary")
        .env("JFF_HOME", home)
        .args(args)
        .output()
        .expect("run jff")
}

fn ok(home: &Path, args: &[&str]) -> String {
    let output = run(home, args);

    assert!(
        output.status.success(),
        "expected success for {args:?}, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn reference_from(stdout: &str) -> String {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("ref:  "))
        .expect("ref in output")
        .trim()
        .to_string()
}

fn map(home: &Path) -> serde_json::Value {
    let contents = fs::read_to_string(home.join("map.json")).expect("map.json");
    serde_json::from_str(&contents).expect("valid map.json")
}

fn create_entry(home: &Path, kind: &str, title: &str) -> String {
    let inbox = home.join("inbox");
    fs::create_dir_all(&inbox).expect("inbox");
    let filename = title.to_lowercase().replace([' ', '/'], "_");
    let source = inbox.join(format!("{filename}.md"));
    fs::write(&source, format!("# {title}\n\nBody of {title}.\n")).expect("source file");
    let flag = if kind == "term" { "-t" } else { "-c" };
    let stdout = ok(
        home,
        &["create", flag, source.to_str().expect("source path")],
    );

    reference_from(&stdout)
}

#[test]
fn opens_an_existing_term_in_text_mode() {
    let home = TempDir::new().expect("temp dir");
    let reference = create_entry(home.path(), "term", "Water Pollution");

    let stdout = ok(home.path(), &["-t", "water pollution"]);

    assert!(stdout.contains("title: Water Pollution"));
    assert!(stdout.contains(&reference));
    assert!(stdout.contains("# Water Pollution"));
    assert_eq!(map(home.path())["entries"].as_array().unwrap().len(), 1);
}

#[test]
fn imports_concepts_in_their_own_tree() {
    let home = TempDir::new().expect("temp dir");
    let reference = create_entry(home.path(), "concept", "Water Pollution");

    assert!(home.path().join("concepts/w/water_pollution.md").is_file());

    let map = map(home.path());
    assert_eq!(map["entries"][0]["kind"], "concept");
    assert_eq!(map["entries"][0]["ref"], reference);
}

#[test]
fn refs_are_unique_within_the_same_second() {
    let home = TempDir::new().expect("temp dir");

    let water = create_entry(home.path(), "term", "Water");
    let wine = create_entry(home.path(), "term", "Wine");

    assert_ne!(water, wine);
    assert_eq!(water.len(), 16);
    assert_eq!(wine.len(), 16);
}

#[test]
fn legacy_exact_search_works_and_reports_missing_matches() {
    let home = TempDir::new().expect("temp dir");
    let reference = create_entry(home.path(), "term", "Water");

    let found = ok(home.path(), &["-set", "water"]);
    assert!(found.contains(&reference));

    let missing = run(home.path(), &["-set", "nothing"]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("no matches"));
}

#[test]
fn legacy_concept_search_is_case_and_accent_insensitive() {
    let home = TempDir::new().expect("temp dir");
    let reference = create_entry(home.path(), "concept", "Contaminación del agua");

    let found = ok(home.path(), &["-sec", "contaminacion del agua"]);
    assert!(found.contains(&reference));
}

#[test]
fn deletes_by_ref_and_removes_the_file() {
    let home = TempDir::new().expect("temp dir");
    let reference = create_entry(home.path(), "term", "Water");
    let path = home.path().join("terms/w/water.md");
    assert!(path.is_file());

    let stdout = ok(home.path(), &["-d", &reference]);
    assert!(stdout.contains(&reference));
    assert!(!path.exists());
    assert!(map(home.path())["entries"].as_array().unwrap().is_empty());
}

#[test]
fn follows_more_links_to_related_entries() {
    let home = TempDir::new().expect("temp dir");
    let term_reference = create_entry(home.path(), "term", "Water");
    let concept_reference = create_entry(home.path(), "concept", "Water Pollution");

    let term_path = home.path().join("terms/w/water.md");
    let mut contents = fs::read_to_string(&term_path).expect("term file");
    contents.push_str("\n[more:concept](../../concepts/w/water_pollution.md)\n");
    fs::write(&term_path, contents).expect("append link");

    let stdout = ok(home.path(), &["-srt", "water"]);

    assert!(stdout.contains(&term_reference));
    assert!(stdout.contains(&concept_reference));
    assert!(stdout.contains("Water Pollution"));
}

#[test]
fn reports_missing_related_links() {
    let home = TempDir::new().expect("temp dir");
    create_entry(home.path(), "term", "Water");

    let term_path = home.path().join("terms/w/water.md");
    let mut contents = fs::read_to_string(&term_path).expect("term file");
    contents.push_str("\n[more:term](../../terms/w/missing.md)\n");
    fs::write(&term_path, contents).expect("append link");

    let stdout = ok(home.path(), &["-srt", "water"]);
    assert!(stdout.contains("[missing]"));
    assert!(stdout.contains("missing.md"));
}

#[test]
fn related_list_prints_the_target_h1() {
    let home = TempDir::new().expect("temp dir");
    let term_reference = create_entry(home.path(), "term", "Water");
    let concept_reference = create_entry(home.path(), "concept", "Water Pollution");

    fs::write(
        home.path().join("concepts/w/water_pollution.md"),
        "# Displayed heading\n\nBody.\n",
    )
    .expect("rewrite concept");

    let term_path = home.path().join("terms/w/water.md");
    let mut contents = fs::read_to_string(&term_path).expect("term file");
    contents.push_str("\n[more:concept](../../concepts/w/water_pollution.md)\n");
    fs::write(&term_path, contents).expect("append link");

    let stdout = ok(home.path(), &["-srt", "water"]);

    assert!(stdout.contains(&term_reference));
    assert!(stdout.contains(&concept_reference));
    assert!(stdout.contains("Displayed heading"));
    assert!(!stdout.contains("Water Pollution"));
}

#[test]
fn related_accepts_short_more_aliases() {
    let home = TempDir::new().expect("temp dir");
    let water_reference = create_entry(home.path(), "term", "Water");
    let wine_reference = create_entry(home.path(), "term", "Wine");
    let concept_reference = create_entry(home.path(), "concept", "Water Pollution");

    let term_path = home.path().join("terms/w/water.md");
    let mut contents = fs::read_to_string(&term_path).expect("term file");
    contents.push_str(&format!(
        "\n[more:t]({wine_reference})\n[more:c]({concept_reference})\n"
    ));
    fs::write(&term_path, contents).expect("append links");

    let stdout = ok(home.path(), &["-srt", "water"]);

    assert!(stdout.contains(&water_reference));
    assert!(stdout.contains(&wine_reference));
    assert!(stdout.contains(&concept_reference));
    assert!(stdout.contains("Wine"));
    assert!(stdout.contains("Water Pollution"));
}

#[test]
fn non_interactive_list_falls_back_to_text() {
    let home = TempDir::new().expect("temp dir");
    let term_reference = create_entry(home.path(), "term", "Water");
    let concept_reference = create_entry(home.path(), "concept", "Water Pollution");

    let term_path = home.path().join("terms/w/water.md");
    let mut contents = fs::read_to_string(&term_path).expect("term file");
    contents.push_str("\n[more:concept](../../concepts/w/water_pollution.md)\n");
    fs::write(&term_path, contents).expect("append link");

    let stdout = ok(home.path(), &["-l", "water"]);

    assert!(stdout.contains(&term_reference));
    assert!(stdout.contains(&concept_reference));
    assert!(stdout.contains("Water Pollution"));
}

#[test]
fn opens_a_term_by_ref_in_text_mode() {
    let home = TempDir::new().expect("temp dir");
    let reference = create_entry(home.path(), "term", "Water");

    let stdout = ok(home.path(), &["-t", &reference]);

    assert!(stdout.contains("title: Water"));
    assert!(stdout.contains(&reference));
    assert!(stdout.contains("# Water"));
    assert!(stdout.contains("Body of Water."));
}

#[test]
fn opens_a_concept_by_ref_in_text_mode() {
    let home = TempDir::new().expect("temp dir");
    let reference = create_entry(home.path(), "concept", "Water Pollution");

    let stdout = ok(home.path(), &["-c", &reference]);

    assert!(stdout.contains("title: Water Pollution"));
    assert!(stdout.contains(&reference));
    assert!(stdout.contains("kind:  concept"));
}

#[test]
fn rejects_a_ref_with_the_wrong_kind() {
    let home = TempDir::new().expect("temp dir");
    let reference = create_entry(home.path(), "term", "Water");

    let output = run(home.path(), &["-c", &reference]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("not a concept"));
}

#[test]
fn list_accepts_a_ref_when_piped() {
    let home = TempDir::new().expect("temp dir");
    let term_reference = create_entry(home.path(), "term", "Water");
    let concept_reference = create_entry(home.path(), "concept", "Water Pollution");

    let term_path = home.path().join("terms/w/water.md");
    let mut contents = fs::read_to_string(&term_path).expect("term file");
    contents.push_str("\n[more:concept](../../concepts/w/water_pollution.md)\n");
    fs::write(&term_path, contents).expect("append link");

    let stdout = ok(home.path(), &["-l", &term_reference]);

    assert!(stdout.contains(&term_reference));
    assert!(stdout.contains(&concept_reference));
    assert!(stdout.contains("Water Pollution"));
}

#[test]
fn json_output_is_machine_readable() {
    let home = TempDir::new().expect("temp dir");
    let reference = create_entry(home.path(), "term", "Water");

    let stdout = ok(home.path(), &["find", "water", "--json"]);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

    assert_eq!(value[0]["ref"], reference);
    assert_eq!(value[0]["title"], "Water");
}

#[test]
fn opening_an_existing_term_does_not_duplicate_it() {
    let home = TempDir::new().expect("temp dir");
    let reference = create_entry(home.path(), "term", "Water");

    let stdout = ok(home.path(), &["-t", "  water  "]);

    assert!(stdout.contains(&reference));
    assert!(stdout.contains("title: Water"));
    assert_eq!(map(home.path())["entries"].as_array().unwrap().len(), 1);
}

#[test]
fn opens_an_existing_concept_in_text_mode() {
    let home = TempDir::new().expect("temp dir");
    let reference = create_entry(home.path(), "concept", "Water Pollution");

    let stdout = ok(home.path(), &["-c", "water pollution"]);

    assert!(stdout.contains(&reference));
    assert!(stdout.contains("kind:  concept"));
}

#[test]
fn opening_a_missing_term_fails_without_creating() {
    let home = TempDir::new().expect("temp dir");

    let output = run(home.path(), &["-t", "Water"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no term titled"));
    assert!(stderr.contains("jff create -t <file.md>"));
    assert!(!home.path().join("map.json").exists());
}

#[test]
fn config_init_is_idempotent() {
    let home = TempDir::new().expect("temp dir");

    let first = ok(home.path(), &["config", "init"]);
    assert!(first.contains("created"));
    assert!(home.path().join("config.json").is_file());

    let second = ok(home.path(), &["config", "init"]);
    assert!(second.contains("already exists"));

    let shown = ok(home.path(), &["config", "show"]);
    let value: serde_json::Value = serde_json::from_str(&shown).expect("valid config JSON");
    assert_eq!(value["editor"]["name"], "nano");
    assert_eq!(value["border-style"]["line"], "double");
}

#[test]
fn list_can_filter_by_kind() {
    let home = TempDir::new().expect("temp dir");
    create_entry(home.path(), "term", "Water");
    create_entry(home.path(), "concept", "Water Pollution");

    let terms = ok(home.path(), &["list", "--kind", "term"]);
    let concepts = ok(home.path(), &["list", "--kind", "concept"]);

    assert!(terms.contains("Water"));
    assert!(!terms.contains("Water Pollution"));
    assert!(!terms.contains("\tconcept\t"));
    assert!(concepts.contains("Water Pollution"));
    assert!(!concepts.contains("\tterm\t"));
}

#[test]
fn create_imports_an_existing_file_into_the_right_letter_folder() {
    let home = TempDir::new().expect("temp dir");
    let source = home.path().join("inbox/water.md");
    let contents = "# Water\n\nFresh water.\n";

    fs::create_dir_all(source.parent().unwrap()).expect("source dir");
    fs::write(&source, contents).expect("source file");

    let stdout = ok(
        home.path(),
        &["create", "-t", source.to_str().expect("path")],
    );

    assert!(stdout.contains("created term \"Water\""));
    assert!(stdout.contains("from:"));

    let destination = home.path().join("terms/w/water.md");
    assert_eq!(fs::read_to_string(&destination).expect("dest"), contents);
    assert!(source.is_file(), "import must copy, not move");

    let map = map(home.path());
    assert_eq!(map["entries"][0]["title"], "Water");
    assert_eq!(map["entries"][0]["path"], "terms/w/water.md");
}

#[test]
fn create_makes_a_new_file_when_the_input_is_missing() {
    let home = TempDir::new().expect("temp dir");
    let source = home.path().join("inbox/wind.md");

    let stdout = ok(
        home.path(),
        &["create", "-c", source.to_str().expect("path")],
    );

    assert!(stdout.contains("created concept \"wind\""));

    let destination = home.path().join("concepts/w/wind.md");
    assert!(destination.is_file());
    assert!(
        fs::read_to_string(destination)
            .expect("dest")
            .starts_with("# wind")
    );
    assert!(!source.exists());
}

#[test]
fn create_requires_exactly_one_kind_flag() {
    let home = TempDir::new().expect("temp dir");

    let without_kind = run(home.path(), &["create", "water.md"]);
    assert!(!without_kind.status.success());

    let with_both = run(home.path(), &["create", "-t", "a.md", "-c", "b.md"]);
    assert!(!with_both.status.success());
}

#[test]
fn generates_shell_completions() {
    let home = TempDir::new().expect("temp dir");

    let stdout = ok(home.path(), &["completions", "bash"]);

    assert!(stdout.contains("_jff"));
    assert!(stdout.contains("completions"));
}

#[test]
fn generates_a_man_page() {
    let home = TempDir::new().expect("temp dir");

    let stdout = ok(home.path(), &["man"]);

    assert!(stdout.contains(".TH jff"));
    assert!(stdout.contains("SYNOPSIS"));
}

#[test]
fn autopush_commits_to_a_local_remote() {
    let home = TempDir::new().expect("temp dir");
    let remote = home.path().join("remote.git");

    let git = std::process::Command::new("git")
        .args(["init", "--bare", remote.to_str().expect("remote path")])
        .current_dir(home.path())
        .output();

    let Ok(git) = git else {
        return;
    };

    assert!(
        git.status.success(),
        "{}",
        String::from_utf8_lossy(&git.stderr)
    );

    let config = serde_json::json!({
        "git": {
            "repo": remote,
            "autopush": true,
            "user": "Test User",
            "email": "test@example.com"
        }
    });
    fs::write(
        home.path().join("config.json"),
        serde_json::to_string_pretty(&config).expect("config JSON"),
    )
    .expect("config");

    let source = home.path().join("inbox/water.md");
    fs::create_dir_all(source.parent().expect("inbox")).expect("inbox");
    fs::write(&source, "# Water\n\nAutopush test.\n").expect("source");

    ok(
        home.path(),
        &["create", "-t", source.to_str().expect("path")],
    );

    let output = std::process::Command::new("git")
        .arg("--git-dir")
        .arg(&remote)
        .args(["rev-parse", "--verify", "refs/heads/main"])
        .output()
        .expect("rev-parse remote");

    assert!(output.status.success(), "autopush did not create a commit");
}

#[test]
fn sync_command_pushes_to_a_local_remote() {
    let home = TempDir::new().expect("temp dir");
    let remote = home.path().join("sync-remote.git");

    let git = std::process::Command::new("git")
        .args(["init", "--bare", remote.to_str().expect("remote path")])
        .current_dir(home.path())
        .output();

    let Ok(git) = git else {
        return;
    };

    assert!(
        git.status.success(),
        "{}",
        String::from_utf8_lossy(&git.stderr)
    );

    let config = serde_json::json!({
        "git": {
            "repo": remote,
            "autopush": false,
            "user": "Test User",
            "email": "test@example.com"
        }
    });
    fs::write(
        home.path().join("config.json"),
        serde_json::to_string_pretty(&config).expect("config JSON"),
    )
    .expect("config");

    let source = home.path().join("inbox/wind.md");
    fs::create_dir_all(source.parent().expect("inbox")).expect("inbox");
    fs::write(&source, "# Wind\n\nManual sync test.\n").expect("source");
    ok(
        home.path(),
        &["create", "-t", source.to_str().expect("path")],
    );

    let stdout = ok(home.path(), &["sync"]);
    assert!(stdout.contains("synced"));

    let output = std::process::Command::new("git")
        .arg("--git-dir")
        .arg(&remote)
        .args(["rev-parse", "--verify", "refs/heads/main"])
        .output()
        .expect("rev-parse remote");

    assert!(output.status.success(), "sync did not create a commit");
}

#[test]
fn broken_pipe_does_not_panic() {
    let home = TempDir::new().expect("temp dir");
    let source = home.path().join("inbox/long.md");
    let mut contents = String::from("# Long note\n\n");

    for index in 0..4000 {
        contents.push_str(&format!(
            "Line {index}: contenido para forzar un pipe roto.\n"
        ));
    }

    fs::create_dir_all(source.parent().unwrap()).expect("source dir");
    fs::write(&source, contents).expect("long source");
    ok(
        home.path(),
        &["create", "-t", source.to_str().expect("path")],
    );

    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_jff"))
        .env("JFF_HOME", home.path())
        .args(["-t", "Long note"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn jff");

    drop(child.stdout.take());

    let output = child.wait_with_output().expect("wait for jff");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_ne!(output.status.code(), Some(101));
    assert!(!stderr.contains("panicked"), "stderr: {stderr}");
}

#[test]
fn opens_entries_with_a_custom_editor() {
    let home = TempDir::new().expect("temp dir");
    let reference = create_entry(home.path(), "term", "Water");

    let editor = home.path().join("fake-editor.sh");
    fs::write(&editor, "#!/bin/sh\nexit 0\n").expect("write editor");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&editor).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&editor, permissions).unwrap();
    }

    let config = serde_json::json!({
        "editor": { "name": "fake", "path": editor }
    });
    fs::create_dir_all(home.path()).unwrap();
    fs::write(
        home.path().join("config.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .unwrap();

    let stdout = ok(home.path(), &["edit", &reference]);

    assert!(stdout.contains("edited term"));
}
