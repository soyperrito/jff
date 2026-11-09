use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use crate::config::{Config, Git};
use crate::error::{Error, Result};

pub fn sync(root: &Path, config: &Config, reason: &str) -> Result<()> {
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

    sync(root, config, reason)
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
                "rebase failed; run `git pull --rebase origin {branch}` in `{}`, resolve the conflict, then run `jff sync` again: {}",
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
    fn syncs_to_a_local_bare_remote() {
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

        sync(&root, &config, "test sync").expect("sync");

        let output = Command::new("git")
            .arg("--git-dir")
            .arg(&remote)
            .args(["rev-parse", "--verify", "refs/heads/main"])
            .output()
            .expect("rev-parse");
        assert!(output.status.success());

        let second = sync(&root, &config, "no changes");
        assert!(second.is_ok());
    }
}
