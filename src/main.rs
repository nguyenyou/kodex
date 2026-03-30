#![deny(unused)]
#![warn(clippy::pedantic)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::module_name_repetitions,
    clippy::missing_errors_doc,
    clippy::too_many_lines
)]

use kodex::{index, ingest, query};

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

// ── Shared argument groups ──────────────────────────────────────────────────

/// Symbol kinds accepted by --kind.
#[derive(Clone, ValueEnum)]
enum SymbolKindArg {
    Class,
    Trait,
    Object,
    Method,
    Field,
    Type,
    Constructor,
}

impl SymbolKindArg {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Class => "class",
            Self::Trait => "trait",
            Self::Object => "object",
            Self::Method => "method",
            Self::Field => "field",
            Self::Type => "type",
            Self::Constructor => "constructor",
        }
    }
}

/// Common arguments for symbol query commands.
#[derive(Args)]
struct QueryArgs {
    /// Symbol name, FQN, or FQN suffix to search for
    query: String,
    /// Filter by symbol kind
    #[arg(long)]
    kind: Option<SymbolKindArg>,
    /// Filter by module name (substring match; dotted patterns match segments in order)
    #[arg(long)]
    module: Option<String>,
}

/// Comma-separated patterns to exclude (matches FQN, name, or owner).
#[derive(Args)]
struct ExcludeArgs {
    #[arg(long)]
    exclude: Option<String>,
    /// Auto-exclude noise (same as running `noise` and using its suggested --exclude)
    #[arg(long)]
    noise_filter: bool,
}

// ── CLI definition ──────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "kodex",
    version,
    about = "Compiler-precise Scala code intelligence"
)]
struct Cli {
    /// Path to kodex.idx (default: .scalex/kodex.idx, or `KODEX_IDX` env var)
    #[arg(long, global = true)]
    idx: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Codebase overview: module list with stats
    Overview,
    /// Build kodex.idx from a compiled project's SemanticDB output
    Index {
        /// Workspace root
        #[arg(long, default_value = ".")]
        root: String,
    },
    /// Search for symbol definitions
    Search {
        #[command(flatten)]
        q: QueryArgs,
        #[arg(long, default_value = "50")]
        limit: usize,
        #[command(flatten)]
        excl: ExcludeArgs,
    },
    /// Complete picture of a type or method (members + callers + callees + related)
    Info {
        /// Symbol FQN (from search results)
        fqn: String,
        #[command(flatten)]
        excl: ExcludeArgs,
    },
    /// Call tree: downstream (default) or upstream (--reverse)
    Calls {
        /// Symbol FQN (from search results)
        fqn: String,
        #[arg(long, default_value = "3")]
        depth: usize,
        /// Walk upstream (callers) instead of downstream (callees)
        #[arg(short, long)]
        reverse: bool,
        #[command(flatten)]
        excl: ExcludeArgs,
    },
    /// Where a symbol is referenced across the codebase
    Refs {
        /// Symbol FQN (from search results)
        fqn: String,
        /// Max file locations to show (0 = unlimited)
        #[arg(long, default_value = "100")]
        limit: usize,
    },
    /// Detect noise candidates for --exclude patterns
    Noise {
        /// Max candidates per category
        #[arg(long, default_value = "15")]
        limit: usize,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Overview => {
            let reader = open_index(cli.idx.as_deref())?;
            emit(query::commands::overview::cmd_overview(reader.index()));
            Ok(())
        }

        Command::Index { root } => {
            build_index_for_workspace(&root)?;
            Ok(())
        }

        Command::Search { q, limit, excl } => {
            let reader = open_index(cli.idx.as_deref())?;
            let exclude = resolve_exclude(&excl, reader.index());
            emit(query::commands::search::cmd_search(
                reader.index(),
                &q.query,
                limit,
                q.kind.as_ref().map(SymbolKindArg::as_str),
                q.module.as_deref(),
                &exclude,
            ));
            Ok(())
        }


        Command::Info { fqn, excl } => {
            let reader = open_index(cli.idx.as_deref())?;
            let exclude = resolve_exclude(&excl, reader.index());
            emit(query::commands::info::cmd_info(
                reader.index(),
                &fqn,
                &exclude,
            ));
            Ok(())
        }

        Command::Calls { fqn, depth, reverse, excl } => {
            let reader = open_index(cli.idx.as_deref())?;
            let exclude = resolve_exclude(&excl, reader.index());
            emit(query::commands::calls::cmd_calls(
                reader.index(),
                &fqn,
                depth,
                &exclude,
                reverse,
            ));
            Ok(())
        }

        Command::Refs { fqn, limit } => {
            let reader = open_index(cli.idx.as_deref())?;
            emit(query::commands::refs::cmd_refs(
                reader.index(),
                &fqn,
                limit,
            ));
            Ok(())
        }

        Command::Noise { limit } => {
            let reader = open_index(cli.idx.as_deref())?;
            emit(query::commands::noise::cmd_noise(reader.index(), limit));
            Ok(())
        }

    }
}

/// Build (or rebuild) the index for a workspace, returning the index file path.
fn build_index_for_workspace(root: &str) -> anyhow::Result<PathBuf> {
    let t0 = Instant::now();
    eprintln!("Indexing workspace: {root}");

    let root_path = Path::new(root);
    let provider = ingest::provider::detect_provider(root_path);

    let discovery = provider.discover(root_path)?;
    let t_disc = t0.elapsed();
    eprintln!(
        "Found {} .semanticdb files ({:.1}s)",
        discovery.files.len(),
        t_disc.as_secs_f64()
    );

    let metadata = provider.metadata(root_path, &discovery)?;
    if let Some(ref m) = metadata {
        eprintln!(
            "Build metadata: {} modules ({:.1}s)",
            m.modules.len(),
            t0.elapsed().as_secs_f64()
        );
    }

    let documents = ingest::semanticdb::load_all(&discovery.files)?;
    let t_parse = t0.elapsed();
    let total_symbols: usize = documents.iter().map(|d| d.symbols.len()).sum();
    let total_occs: usize = documents.iter().map(|d| d.occurrences.len()).sum();
    eprintln!(
        "Parsed {} documents, {} symbols, {} occurrences ({:.1}s)",
        documents.len(),
        total_symbols,
        total_occs,
        t_parse.as_secs_f64()
    );

    let built = ingest::merge::build_index(&documents, metadata.as_ref(), root);
    let t_merge = t0.elapsed();
    eprintln!(
        "Index built: {} symbols, {} files, {} modules ({:.1}s)",
        built.symbols.len(),
        built.files.len(),
        built.modules.len(),
        t_merge.as_secs_f64()
    );

    let scalex_dir = Path::new(root).join(".scalex");
    std::fs::create_dir_all(&scalex_dir)?;
    let idx_path = scalex_dir.join("kodex.idx");
    index::writer::write_index(&built, &idx_path)?;
    let t_total = t0.elapsed();
    eprintln!("Total: {:.1}s", t_total.as_secs_f64());

    Ok(idx_path)
}

/// Open the index, or bail if it doesn't exist.
fn open_index(idx_path: Option<&str>) -> anyhow::Result<index::reader::IndexReader> {
    let path = match idx_path {
        Some(p) => PathBuf::from(p),
        None => match std::env::var("KODEX_IDX") {
            Ok(p) => PathBuf::from(p),
            Err(_) => PathBuf::from(".scalex/kodex.idx"),
        },
    };

    if !path.exists() {
        anyhow::bail!(
            "Index not found: {}. Run `kodex index --root <path>` first.",
            path.display()
        );
    }

    index::reader::IndexReader::open(&path)
}

/// Print command output.
#[allow(clippy::needless_pass_by_value)]
fn emit(result: query::commands::CommandResult) {
    print!("{result}");
}

fn parse_exclude(exclude: Option<&String>) -> Vec<String> {
    exclude
        .map(|s| {
            s.split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Resolve exclude patterns: --exclude takes precedence, --noise-filter auto-computes.
fn resolve_exclude(excl: &ExcludeArgs, index: &kodex::model::ArchivedKodexIndex) -> Vec<String> {
    if excl.exclude.is_some() {
        parse_exclude(excl.exclude.as_ref())
    } else if excl.noise_filter {
        let pattern = query::commands::noise::compute_noise_exclude(index);
        parse_exclude(Some(&pattern))
    } else {
        vec![]
    }
}
