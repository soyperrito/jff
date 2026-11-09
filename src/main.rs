mod cli;
mod config;
mod editor;
mod error;
mod links;
mod model;
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

fn run(cli: Cli) -> Result<ExitCode> {
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
        return delete(&store, &reference);
    }

    match cli.command {
        Some(CliCommand::Term { title }) => open_existing(&store, &config, Kind::Term, &title),
        Some(CliCommand::Concept { title }) => {
            open_existing(&store, &config, Kind::Concept, &title)
        }
        Some(CliCommand::Create { term, concept }) => match (term, concept) {
            (Some(path), None) => create_from_file(&store, Kind::Term, &path),
            (None, Some(path)) => create_from_file(&store, Kind::Concept, &path),
            _ => bail!("jff create requires exactly one of -t/--term or -c/--concept"),
        },
        Some(CliCommand::List { kind, letter, json }) => {
            list(&store, kind.map(Kind::from), letter, json)
        }
        Some(CliCommand::Delete { reference }) => delete(&store, &reference),
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
            Ok(ExitCode::SUCCESS)
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

fn create_from_file(store: &Store, kind: Kind, source: &Path) -> Result<ExitCode> {
    let existed = source.exists();
    let entry = store.create_from_file(kind, source)?;

    println!("created {kind} \"{}\"", entry.title);
    println!("ref:  {}", entry.reference);
    println!("path: {}", entry.abs_path(store.root()).display());

    if existed {
        println!("from: {}", source.display());
    }

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

fn delete(store: &Store, reference: &str) -> Result<ExitCode> {
    let entry = store.delete(reference)?;
    println!(
        "deleted {} \"{}\" ({})",
        entry.kind, entry.title, entry.reference
    );
    Ok(ExitCode::SUCCESS)
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
