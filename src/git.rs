use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use chrono::Local;

use crate::config::{Config, Git};
use crate::error::{Error, Result};

pub fn backup(root: &Path, config: &Config, reason: &str) -> Result<()> {
    let Some(repo) = config.git.repo.as_deref() else {
        return Err(Error::Git(
            "git.repo is not configured in config.json".to_string(),
        ));
    };

    ensure_repo(root, repo)?;
    ensure_gitignore(root)?;
    commit(root, &config.git, reason)?;
    push(root, &config.git)?;

    Ok(())
}

pub fn autopush(root: &Path, config: &Config, reason: &str) -> Result<()> {
    if !config.git.autopush {
        return Ok(());
    }

    backup(root, config, reason)
}

/// Load `map.json`, `terms/` and `concepts/` from the configured remote.
///
/// Existing work is copied to `.cache/load-backup-<timestamp>/` before a
/// first-time clone replaces the local data. Returns the backup path when one
/// was created.
pub fn load(root: &Path, config: &Config) -> Result<Option<PathBuf>> {
    let Some(repo) = config.git.repo.as_deref() else {
        return Err(Error::Git(
            "git.repo is not configured in config.json".to_string(),
        ));
    };

    fs::create_dir_all(root)?;

    let backup = if root.join(".git").is_dir() {
        pull_existing(root, config, repo)?;
        None
    } else {
        load_clone(root, config, repo)?
    };

    fs::create_dir_all(root.join("terms"))?;
    fs::create_dir_all(root.join("concepts"))?;

    Ok(backup)
}

fn pull_existing(root: &Path, config: &Config, repo: &str) -> Result<()> {
    ensure_repo(root, repo)?;

    let status = git_output(
        root,
        None,
        &[
            "status",
            "--porcelain",
            "--untracked-files=no",
            "--",
            "map.json",
            "terms",
            "concepts",
        ],
    )?;

    if !status.status.success() {
        return Err(Error::Git(format!(
            "git status failed: {}",
            stderr(&status)
        )));
    }

    if !String::from_utf8_lossy(&status.stdout).trim().is_empty() {
        return Err(Error::Git(
            "tracked changes in map.json, terms/ or concepts/; run `jff backup` before `jff load`"
                .to_string(),
        ));
    }

    let branch = current_branch(root)?;
    let fetch = git_output(root, Some(&config.git), &["fetch", "origin"])?;

    if !fetch.status.success() {
        return Err(Error::Git(format!("git fetch failed: {}", stderr(&fetch))));
    }

    let remote_ref = format!("origin/{branch}");
    let remote = git_output(
        root,
        Some(&config.git),
        &["rev-parse", "--verify", &remote_ref],
    )?;

    if !remote.status.success() {
        return Err(Error::Git(format!(
            "remote branch `{branch}` does not exist"
        )));
    }

    let pull = git_output(
        root,
        Some(&config.git),
        &["pull", "--rebase", "--autostash", "origin", &branch],
    )?;

    if !pull.status.success() {
        let _ = git_output(root, None, &["rebase", "--abort"]);
        return Err(Error::Git(format!(
            "load failed; resolve the conflict manually: {}",
            stderr(&pull)
        )));
    }

    Ok(())
}

fn load_clone(root: &Path, config: &Config, repo: &str) -> Result<Option<PathBuf>> {
    let temporary = tempfile::tempdir()
        .map_err(|error| Error::Git(format!("cannot create temporary directory: {error}")))?;
    let clone = temporary.path().join("repo");
    let clone_text = clone
        .to_str()
        .ok_or_else(|| Error::Git("temporary path is not valid UTF-8".to_string()))?;
    let output = git_output(root, Some(&config.git), &["clone", repo, clone_text])?;

    if !output.status.success() {
        return Err(Error::Git(format!("git clone failed: {}", stderr(&output))));
    }

    if !clone.join("map.json").is_file() {
        return Err(Error::Git(
            "remote repository does not contain map.json".to_string(),
        ));
    }

    let backup = backup_local(root)?;

    for path in ["map.json", "terms", "concepts"] {
        remove_path(&root.join(path))?;
    }

    copy_contents(&clone, root)?;

    if let Some(backup) = &backup {
        restore_local(root, backup)?;
    }

    Ok(backup)
}

fn backup_local(root: &Path) -> Result<Option<PathBuf>> {
    let paths = ["map.json", "terms", "concepts", "config.json", "themes"];
    let existing: Vec<&str> = paths
        .iter()
        .copied()
        .filter(|path| root.join(path).exists())
        .collect();

    if existing.is_empty() {
        return Ok(None);
    }

    let directory = root.join(".cache").join(format!(
        "load-backup-{}",
        Local::now().format("%Y%m%d%H%M%S")
    ));
    fs::create_dir_all(&directory)?;

    for path in existing {
        copy_path(&root.join(path), &directory.join(path))?;
    }

    Ok(Some(directory))
}

fn restore_local(root: &Path, backup: &Path) -> Result<()> {
    let config = backup.join("config.json");

    if config.is_file() {
        fs::copy(&config, root.join("config.json"))?;
    }

    let themes = backup.join("themes");

    if themes.is_dir() {
        let destination = root.join("themes");

        if destination.exists() {
            remove_path(&destination)?;
        }

        copy_path(&themes, &destination)?;
    }

    Ok(())
}

fn copy_contents(source: &Path, destination: &Path) -> Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        copy_path(&entry.path(), &destination.join(entry.file_name()))?;
    }

    Ok(())
}

fn copy_path(source: &Path, destination: &Path) -> Result<()> {
    if source.is_dir() {
        fs::create_dir_all(destination)?;

        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_path(&entry.path(), &destination.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::copy(source, destination)?;
    }

    Ok(())
}

fn remove_path(path: &Path) -> Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)?;
    } else if path.exists() {
        fs::remove_file(path)?;
    }

    Ok(())
}

fn ensure_repo(root: &Path, repo: &str) -> Result<()> {
    fs::create_dir_all(root)?;

    if !root.join(".git").exists() {
        run_git(root, None, &["init", "-b", "main"])?;
    }

    let current = git_output(root, None, &["remote", "get-url", "origin"])?;

    if current.status.success() {
        let current = String::from_utf8_lossy(&current.stdout).trim().to_string();

        if current != repo {
            run_git(root, None, &["remote", "set-url", "origin", repo])?;
        }
    } else {
        run_git(root, None, &["remote", "add", "origin", repo])?;
    }

    Ok(())
}

fn ensure_gitignore(root: &Path) -> Result<()> {
    let path = root.join(".gitignore");
    let mut contents = fs::read_to_string(&path).unwrap_or_default();
    let mut changed = false;

    for entry in [".lock", ".cache/", "*.tmp"] {
        if !contents.lines().any(|line| line.trim() == entry) {
            if !contents.is_empty() && !contents.ends_with('\n') {
                contents.push('\n');
            }
            contents.push_str(entry);
            contents.push('\n');
            changed = true;
        }
    }

    if changed || !path.exists() {
        fs::write(path, contents)?;
    }

    Ok(())
}

fn commit(root: &Path, git: &Git, reason: &str) -> Result<()> {
    run_git(root, None, &["add", "-A"])?;

    let diff = git_output(root, None, &["diff", "--cached", "--quiet"])?;

    if diff.status.success() {
        return Ok(());
    }

    if diff.status.code() != Some(1) {
        return Err(Error::Git(format!("git diff failed: {}", stderr(&diff))));
    }

    let name = git.user.clone().unwrap_or_else(|| "jff".to_string());
    let email = git
        .email
        .clone()
        .or_else(|| git.user.clone().filter(|user| user.contains('@')))
        .unwrap_or_else(|| "jff@localhost".to_string());
    let user_name = format!("user.name={name}");
    let user_email = format!("user.email={email}");
    let message = format!("jff: {reason}");

    run_git(
        root,
        None,
        &[
            "-c",
            &user_name,
            "-c",
            &user_email,
            "commit",
            "-m",
            &message,
        ],
    )?;

    Ok(())
}

fn push(root: &Path, git: &Git) -> Result<()> {
    let branch = current_branch(root)?;

    let fetch = git_output(root, Some(git), &["fetch", "origin"])?;

    if !fetch.status.success() {
        return Err(Error::Git(format!("git fetch failed: {}", stderr(&fetch))));
    }

    let remote_ref = format!("origin/{branch}");
    let remote_exists = git_output(root, Some(git), &["rev-parse", "--verify", &remote_ref])?;

    if remote_exists.status.success() {
        let pull = git_output(
            root,
            Some(git),
            &["pull", "--rebase", "--autostash", "origin", &branch],
        )?;

        if !pull.status.success() {
            let _ = git_output(root, None, &["rebase", "--abort"]);
            return Err(Error::Git(format!(
                "rebase failed; run `git pull --rebase origin {branch}` in `{}`, resolve the conflict, then run `jff backup` again: {}",
                root.display(),
                stderr(&pull)
            )));
        }
    }

    let output = git_output(root, Some(git), &["push", "-u", "origin", "HEAD"])?;

    if !output.status.success() {
        return Err(Error::Git(format!("git push failed: {}", stderr(&output))));
    }

    Ok(())
}

fn current_branch(root: &Path) -> Result<String> {
    let output = run_git(root, None, &["symbolic-ref", "--short", "HEAD"])?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_git(root: &Path, git: Option<&Git>, args: &[&str]) -> Result<Output> {
    let output = git_output(root, git, args)?;

    if !output.status.success() {
        return Err(Error::Git(format!(
            "git {} failed: {}",
            args.first().copied().unwrap_or("command"),
            stderr(&output)
        )));
    }

    Ok(output)
}

fn git_output(root: &Path, git: Option<&Git>, args: &[&str]) -> Result<Output> {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true")
        .env("GIT_MERGE_AUTOEDIT", "no");

    if let Some(git) = git
        && let Ok(token) = std::env::var(&git.token_env)
        && !token.is_empty()
    {
        command.arg("-c").arg("credential.helper=");
        command.arg("-c").arg(format!(
            "credential.helper=!f() {{ echo username=x-access-token; echo password=${}; }}; f",
            git.token_env
        ));
        command.env(&git.token_env, token);
    }

    command.args(args);

    command
        .output()
        .map_err(|error| Error::Git(format!("could not run git; is it installed? {error}")))
}

fn stderr(output: &Output) -> String {
    let error = String::from_utf8_lossy(&output.stderr);
    let error = error.trim();

    if error.is_empty() {
        format!("exit status {}", output.status)
    } else {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use tempfile::TempDir;

    fn git_command(path: &Path, args: &[&str]) -> Output {
        Command::new("git")
            .current_dir(path)
            .args(args)
            .output()
            .expect("git")
    }

    #[test]
    fn backs_up_to_a_local_bare_remote() {
        let directory = TempDir::new().expect("temp dir");
        let root = directory.path().join("store");
        let remote = directory.path().join("remote.git");
        fs::create_dir_all(&root).expect("store");
        fs::write(root.join("note.md"), "# Note\n").expect("note");
        let output = git_command(
            directory.path(),
            &["init", "--bare", remote.to_str().unwrap()],
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let mut config = Config::default();
        config.git.repo = Some(remote.to_string_lossy().into_owned());
        config.git.user = Some("Test User".to_string());
        config.git.email = Some("test@example.com".to_string());

        backup(&root, &config, "test backup").expect("backup");

        let output = Command::new("git")
            .arg("--git-dir")
            .arg(&remote)
            .args(["rev-parse", "--verify", "refs/heads/main"])
            .output()
            .expect("rev-parse");
        assert!(output.status.success());

        let second = backup(&root, &config, "no changes");
        assert!(second.is_ok());
    }

    #[test]
    fn loads_map_terms_and_concepts_from_remote() {
        let directory = TempDir::new().expect("temp dir");
        let seed = directory.path().join("seed");
        let loaded = directory.path().join("loaded");
        let remote = directory.path().join("remote.git");

        fs::create_dir_all(seed.join("terms/w")).expect("seed terms");
        fs::create_dir_all(seed.join("concepts/w")).expect("seed concepts");
        fs::write(seed.join("terms/w/water.md"), "# Water\n\nBody.\n").expect("term");
        fs::write(
            seed.join("concepts/w/water_pollution.md"),
            "# Water Pollution\n\nConcept.\n",
        )
        .expect("concept");
        fs::write(
            seed.join("map.json"),
            r#"{"version":1,"next_id":2,"entries":[]}"#,
        )
        .expect("map");

        let output = git_command(
            directory.path(),
            &["init", "--bare", remote.to_str().unwrap()],
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let mut config = Config::default();
        config.git.repo = Some(remote.to_string_lossy().into_owned());
        config.git.user = Some("Test User".to_string());
        config.git.email = Some("test@example.com".to_string());

        backup(&seed, &config, "seed").expect("seed backup");
        let backup_path = load(&loaded, &config).expect("load");

        assert!(backup_path.is_none());
        assert!(loaded.join("map.json").is_file());
        assert!(loaded.join("terms/w/water.md").is_file());
        assert!(loaded.join("concepts/w/water_pollution.md").is_file());
        assert!(loaded.join(".git").is_dir());
    }
}
