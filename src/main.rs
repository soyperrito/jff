mod cli;
mod config;
mod editor;
mod error;
mod git;
mod links;
mod model;
mod opener;
mod refs;
mod store;
mod text;
mod tui;

use std::io::IsTerminal;
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::{CommandFactory, Parser};

use crate::cli::{Cli, Command as CliCommand, ConfigCommand};
use crate::model::{Entry, Kind};
use crate::store::Store;

fn main() -> ExitCode {
    install_broken_pipe_hook();

    let args = cli::normalize_legacy_args(std::env::args().collect());
    let cli = Cli::parse_from(args);

    match run(cli) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("jff: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// `println!` panics on EPIPE. Treat a closed downstream pipe as a normal end
/// of output so commands like `jff -t Water | head` do not print a panic.
fn install_broken_pipe_hook() {
    let default_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |info| {
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or_default();

        if message.contains("Broken pipe") || message.contains("os error 32") {
            std::process::exit(0);
        }

        default_hook(info);
    }));
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command.as_ref() {
        Some(CliCommand::Completions { shell }) => {
            let mut command = Cli::command();
            clap_complete::generate(*shell, &mut command, "jff", &mut std::io::stdout());
            return Ok(ExitCode::SUCCESS);
        }
        Some(CliCommand::Man) => {
            match clap_mangen::Man::new(Cli::command()).render(&mut std::io::stdout()) {
                Ok(()) => return Ok(ExitCode::SUCCESS),
                Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => {
                    return Ok(ExitCode::SUCCESS);
                }
                Err(error) => return Err(error.into()),
            }
        }
        _ => {}
    }

    let root = config::root()?;
    let store = Store::new(&root);
    let config = config::Config::load(&root)?;

    if let Some(title) = cli.term {
        return open_existing(&store, &config, Kind::Term, &title);
    }

    if let Some(title) = cli.concept {
        return open_existing(&store, &config, Kind::Concept, &title);
    }

    if let Some(query) = cli.list {
        return open_related(&store, &config, &query);
    }

    if let Some(reference) = cli.delete {
        return delete(&store, &config, &reference);
    }

    match cli.command {
        Some(CliCommand::Term { title }) => open_existing(&store, &config, Kind::Term, &title),
        Some(CliCommand::Concept { title }) => {
            open_existing(&store, &config, Kind::Concept, &title)
        }
        Some(CliCommand::Create { term, concept }) => match (term, concept) {
            (Some(path), None) => create_from_file(&store, &config, Kind::Term, &path),
            (None, Some(path)) => create_from_file(&store, &config, Kind::Concept, &path),
            _ => bail!("jff create requires exactly one of -t/--term or -c/--concept"),
        },
        Some(CliCommand::List { kind, letter, json }) => {
            list(&store, kind.map(Kind::from), letter, json)
        }
        Some(CliCommand::Delete { reference }) => delete(&store, &config, &reference),
        Some(CliCommand::Find {
            query,
            kind,
            exact,
            json,
        }) => find(&store, &query, kind.map(Kind::from), exact, json),
        Some(CliCommand::Related { query, kind, json }) => {
            related(&store, &query, kind.map(Kind::from), json)
        }
        Some(CliCommand::Edit { reference, editor }) => {
            let entry = store.get(&reference)?;
            editor::open(&store, &config, &entry, editor)?;
            println!("edited {} {}", entry.kind, entry.reference);
            autopush_or_warn(store.root(), &config, &format!("edit {}", entry.reference));
            Ok(ExitCode::SUCCESS)
        }
        Some(CliCommand::Backup) => backup_command(&store, &config),
        Some(CliCommand::Load) => load_command(&store, &config),
        Some(CliCommand::Index) => index_command(&store, &config),
        Some(CliCommand::Completions { .. }) | Some(CliCommand::Man) => {
            unreachable!("packaging commands are handled before loading the configuration")
        }
        Some(CliCommand::Config { cmd }) => config_command(&store, cmd),
        None => {
            Cli::command().print_help()?;
            println!();
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn open_existing(
    store: &Store,
    config: &config::Config,
    kind: Kind,
    query: &str,
) -> Result<ExitCode> {
    let entry = resolve_exact(store, query, kind)?;

    if interactive() {
        tui::run(store.clone(), config.clone(), entry)?;
        return Ok(ExitCode::SUCCESS);
    }

    print_entry_text(store, &entry)
}

/// Resolve a title or a ref for `-t` / `-c`, requiring the requested kind.
fn resolve_exact(store: &Store, query: &str, kind: Kind) -> Result<Entry> {
    let query = query.strip_prefix("jff:").unwrap_or(query).trim();

    if refs::is_valid(query) {
        let entry = store.get(query)?;

        if entry.kind != kind {
            bail!(
                "ref `{query}` is a {}, not a {}; use the matching -t/-c flag",
                entry.kind,
                kind
            );
        }

        return Ok(entry);
    }

    let mut entries = store.find_exact(Some(kind), query)?;

    match entries.len() {
        1 => Ok(entries.remove(0)),
        0 => {
            let flag = if kind == Kind::Term { "-t" } else { "-c" };
            bail!("no {kind} titled `{query}`; create it with `jff create {flag} <file.md>`");
        }
        _ => {
            let refs = entries
                .iter()
                .take(10)
                .map(|entry| entry.reference.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            bail!("`{query}` matches more than one {kind}: {refs}");
        }
    }
}

fn create_from_file(
    store: &Store,
    config: &config::Config,
    kind: Kind,
    source: &Path,
) -> Result<ExitCode> {
    let existed = source.exists();
    let entry = store.create_from_file(kind, source)?;

    println!("created {kind} \"{}\"", entry.title);
    println!("ref:  {}", entry.reference);
    println!("path: {}", entry.abs_path(store.root()).display());

    if existed {
        println!("from: {}", source.display());
    }

    autopush_or_warn(store.root(), config, &format!("create {}", entry.reference));

    Ok(ExitCode::SUCCESS)
}

fn open_related(store: &Store, config: &config::Config, query: &str) -> Result<ExitCode> {
    let entry = resolve_query(store, query, None)?;

    if interactive() {
        tui::run(store.clone(), config.clone(), entry)?;
        return Ok(ExitCode::SUCCESS);
    }

    print_related(store, &entry, false)
}

/// Resolve a title, fuzzy query or ref for the related list.
fn resolve_query(store: &Store, query: &str, kind: Option<Kind>) -> Result<Entry> {
    let query = query.strip_prefix("jff:").unwrap_or(query).trim();

    if refs::is_valid(query) {
        let entry = store.get(query)?;

        if let Some(kind) = kind
            && entry.kind != kind
        {
            bail!("ref `{query}` is a {}, not a {}", entry.kind, kind);
        }

        return Ok(entry);
    }

    Ok(store.resolve_one(query, kind)?)
}

fn print_entry_text(store: &Store, entry: &Entry) -> Result<ExitCode> {
    let contents = store.content(entry)?;

    println!("title: {}", entry.title);
    println!("ref:   {}", entry.reference);
    println!("kind:  {}", entry.kind);
    println!("path:  {}", entry.abs_path(store.root()).display());
    println!();
    print!("{contents}");

    if !contents.ends_with('\n') {
        println!();
    }

    Ok(ExitCode::SUCCESS)
}

fn list(store: &Store, kind: Option<Kind>, letter: Option<char>, json: bool) -> Result<ExitCode> {
    let entries = store.list(kind, letter)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else if entries.is_empty() {
        println!("no entries");
    } else {
        print_entries(&entries);
    }

    Ok(ExitCode::SUCCESS)
}

fn find(
    store: &Store,
    query: &str,
    kind: Option<Kind>,
    exact: bool,
    json: bool,
) -> Result<ExitCode> {
    let entries = if exact {
        store.find_exact(kind, query)?
    } else {
        store.search(kind, query)?
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else if entries.is_empty() {
        eprintln!("no matches for \"{query}\"");
    } else {
        print_entries(&entries);
    }

    if entries.is_empty() {
        Ok(ExitCode::from(1))
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

fn related(store: &Store, query: &str, kind: Option<Kind>, json: bool) -> Result<ExitCode> {
    let entry = resolve_query(store, query, kind)?;
    print_related(store, &entry, json)
}

fn print_related(store: &Store, entry: &Entry, json: bool) -> Result<ExitCode> {
    let related = store.related(&entry.reference)?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "entry": entry,
                "related": related,
            }))?
        );
    } else {
        println!("{} ({}, {})", entry.title, entry.kind, entry.reference);

        if related.is_empty() {
            println!("no related entries");
        }

        for item in &related {
            match &item.target {
                Some(target) => println!(
                    "  {}\t{}\t{}\t-> {}",
                    item.heading.as_deref().unwrap_or(&target.title),
                    target.reference,
                    target.kind,
                    target.path
                ),
                None => println!(
                    "  [missing]\t{}\t{}\t-> {}",
                    item.link.kind, item.link.label, item.link.dest
                ),
            }
        }
    }

    Ok(ExitCode::SUCCESS)
}

fn delete(store: &Store, config: &config::Config, reference: &str) -> Result<ExitCode> {
    let entry = store.delete(reference)?;
    println!(
        "deleted {} \"{}\" ({})",
        entry.kind, entry.title, entry.reference
    );
    autopush_or_warn(store.root(), config, &format!("delete {}", entry.reference));
    Ok(ExitCode::SUCCESS)
}

fn backup_command(store: &Store, config: &config::Config) -> Result<ExitCode> {
    git::backup(store.root(), config, "manual backup")?;
    println!("backed up {}", store.root().display());
    Ok(ExitCode::SUCCESS)
}

fn load_command(store: &Store, config: &config::Config) -> Result<ExitCode> {
    let backup = git::load(store.root(), config)?;
    println!("loaded {}", store.root().display());

    if let Some(backup) = backup {
        println!("previous data backed up to {}", backup.display());
    }

    Ok(ExitCode::SUCCESS)
}

fn index_command(store: &Store, config: &config::Config) -> Result<ExitCode> {
    let report = store.index()?;

    println!(
        "scanned {} file(s), indexed {}, skipped {}",
        report.scanned,
        report.added.len(),
        report.skipped.len()
    );

    for entry in &report.added {
        println!(
            "  + {} \"{}\" {} -> {}",
            entry.kind, entry.title, entry.reference, entry.path
        );
    }

    for skip in &report.skipped {
        eprintln!("  ! {}: {}", skip.path, skip.reason);
    }

    if !report.added.is_empty() {
        autopush_or_warn(store.root(), config, "index");
    }

    Ok(ExitCode::SUCCESS)
}

fn autopush_or_warn(root: &Path, config: &config::Config, reason: &str) {
    if let Err(error) = git::autopush(root, config, reason) {
        eprintln!("jff: git autopush failed: {error}");
    }
}

fn config_command(store: &Store, command: ConfigCommand) -> Result<ExitCode> {
    let root = store.root();

    match command {
        ConfigCommand::Path => println!("{}", config::config_path(root).display()),
        ConfigCommand::Init => {
            store.ensure_layout()?;
            let created = config::Config::write_default(root)?;
            let path = config::config_path(root);

            if created {
                println!("created {}", path.display());
            } else {
                println!("config already exists: {}", path.display());
            }
        }
        ConfigCommand::Show => {
            let config = config::Config::load(root)?;
            println!("{}", serde_json::to_string_pretty(&config)?);
        }
    }

    Ok(ExitCode::SUCCESS)
}

fn print_entries(entries: &[Entry]) {
    for entry in entries {
        println!("{}\t{}\t{}", entry.reference, entry.kind, entry.title);
    }
}

fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}
