use std::path::PathBuf;

use clap::{ArgGroup, Parser, Subcommand, ValueEnum};

use crate::model::Kind;

#[derive(Debug, Parser)]
#[command(
    name = "jff",
    version,
    about = "Just for Fun - Encyclopedic Dictionary",
    args_conflicts_with_subcommands = true,
    group = ArgGroup::new("action")
        .args(["term", "concept", "list", "delete"])
        .multiple(false)
)]
pub struct Cli {
    /// Open a term in the terminal viewer
    #[arg(short = 't', long = "term", value_name = "TITLE|REF")]
    pub term: Option<String>,

    /// Open a concept in the terminal viewer
    #[arg(short = 'c', long = "concept", value_name = "TITLE|REF")]
    pub concept: Option<String>,

    /// Show entries related to a term/concept through [more:...] links
    #[arg(short = 'l', long = "list", value_name = "TITLE|REF")]
    pub list: Option<String>,

    /// Delete an entry by ref
    #[arg(short = 'd', long = "delete", value_name = "REF")]
    pub delete: Option<String>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Open a term in the terminal viewer
    Term {
        #[arg(value_name = "TITLE|REF")]
        title: String,
    },

    /// Open a concept in the terminal viewer
    Concept {
        #[arg(value_name = "TITLE|REF")]
        title: String,
    },

    /// Import a Markdown file or create one inside the knowledge base
    Create {
        /// Import or create a term from FILE
        #[arg(
            short = 't',
            long = "term",
            value_name = "FILE",
            conflicts_with = "concept",
            required_unless_present = "concept"
        )]
        term: Option<PathBuf>,

        /// Import or create a concept from FILE
        #[arg(
            short = 'c',
            long = "concept",
            value_name = "FILE",
            conflicts_with = "term",
            required_unless_present = "term"
        )]
        concept: Option<PathBuf>,
    },

    /// List all entries
    List {
        #[arg(long, value_enum)]
        kind: Option<KindArg>,
        #[arg(long, value_name = "LETTER")]
        letter: Option<char>,
        #[arg(long)]
        json: bool,
    },

    /// Delete an entry by ref
    Delete {
        #[arg(value_name = "REF")]
        reference: String,
    },

    /// Search entries by title
    Find {
        #[arg(value_name = "QUERY")]
        query: String,
        #[arg(long, value_enum)]
        kind: Option<KindArg>,
        #[arg(long)]
        exact: bool,
        #[arg(long)]
        json: bool,
    },

    /// Show entries linked with [more:term] or [more:concept]
    Related {
        #[arg(value_name = "QUERY")]
        query: String,
        #[arg(long, value_enum)]
        kind: Option<KindArg>,
        #[arg(long)]
        json: bool,
    },

    /// Open an entry in an editor
    Edit {
        #[arg(value_name = "REF")]
        reference: String,
        #[arg(long, value_enum)]
        editor: Option<EditorArg>,
    },

    /// Inspect or initialize configuration
    Config {
        #[command(subcommand)]
        cmd: ConfigCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Print the path of config.json
    Path,
    /// Create config.json with default values
    Init,
    /// Print the effective configuration
    Show,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum KindArg {
    Term,
    Concept,
}

impl From<KindArg> for Kind {
    fn from(value: KindArg) -> Self {
        match value {
            KindArg::Term => Kind::Term,
            KindArg::Concept => Kind::Concept,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum EditorArg {
    Nano,
    #[value(alias = "neovim")]
    Vim,
    Code,
    Emacs,
    #[value(alias = "other")]
    Custom,
}

/// Translate the single-dash aliases from the original specification into
/// regular subcommands before clap parses the arguments.
pub fn normalize_legacy_args(args: Vec<String>) -> Vec<String> {
    if args.len() < 2 {
        return args;
    }

    let (command, flags): (&str, &[&str]) = match args[1].as_str() {
        "-set" => ("find", &["--kind", "term", "--exact"]),
        "-sec" => ("find", &["--kind", "concept", "--exact"]),
        "-st" => ("find", &["--kind", "term"]),
        "-sc" => ("find", &["--kind", "concept"]),
        "-srt" => ("related", &["--kind", "term"]),
        "-src" => ("related", &["--kind", "concept"]),
        "-dt" | "-dc" => ("delete", &[]),
        "-en" => ("edit", &["--editor", "nano"]),
        "-ev" => ("edit", &["--editor", "vim"]),
        "-ec" => ("edit", &["--editor", "code"]),
        "-ee" => ("edit", &["--editor", "emacs"]),
        "-eo" => ("edit", &["--editor", "custom"]),
        _ => return args,
    };

    let mut normalized = Vec::with_capacity(args.len() + 2);
    normalized.push(args[0].clone());
    normalized.push(command.to_string());
    normalized.extend(args[2..].iter().cloned());
    normalized.extend(flags.iter().map(|flag| flag.to_string()));
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_exact_search_aliases() {
        assert_eq!(
            normalize_legacy_args(vec!["jff".into(), "-set".into(), "water".into()]),
            ["jff", "find", "water", "--kind", "term", "--exact"]
        );
        assert_eq!(
            normalize_legacy_args(vec!["jff".into(), "-sec".into(), "water".into()]),
            ["jff", "find", "water", "--kind", "concept", "--exact"]
        );
    }

    #[test]
    fn translates_related_and_editor_aliases() {
        assert_eq!(
            normalize_legacy_args(vec!["jff".into(), "-srt".into(), "water".into()]),
            ["jff", "related", "water", "--kind", "term"]
        );
        assert_eq!(
            normalize_legacy_args(vec!["jff".into(), "-en".into(), "wt20261109123456".into()]),
            ["jff", "edit", "wt20261109123456", "--editor", "nano"]
        );
    }

    #[test]
    fn leaves_regular_arguments_alone() {
        let args = vec!["jff".into(), "-t".into(), "water".into()];
        assert_eq!(normalize_legacy_args(args.clone()), args);
    }
}
