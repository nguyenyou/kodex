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
    CaseClass,
    Trait,
    Object,
    Method,
    Field,
    Type,
    Constructor,
    Enum,
}

impl SymbolKindArg {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Class => "class",
            Self::CaseClass => "case-class",
            Self::Trait => "trait",
            Self::Object => "object",
            Self::Method => "method",
            Self::Field => "field",
            Self::Type => "type",
            Self::Constructor => "constructor",
            Self::Enum => "enum",
        }
    }
}

/// Common arguments for symbol query commands.
#[derive(Args)]
struct QueryArgs {
    /// Symbol name, FQN, or FQN suffix to search for (optional when --module is provided)
    query: Option<String>,
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
    /// Include noise (generated code, plumbing methods, etc.) — excluded by default
    #[arg(long)]
    include_noise: bool,
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
    /// Build kodex.idx from a compiled project's `SemanticDB` output
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
        /// Only show edges that cross module boundaries
        #[arg(long)]
        cross_module_only: bool,
        #[command(flatten)]
        excl: ExcludeArgs,
    },
    /// Call tree with info-level detail (signature + source) at each node
    Trace {
        /// Symbol FQN (from search results)
        fqn: String,
        #[arg(long, default_value = "3")]
        depth: usize,
        /// Walk upstream (callers) instead of downstream (callees)
        #[arg(short, long)]
        reverse: bool,
        /// Only show edges that cross module boundaries
        #[arg(long)]
        cross_module_only: bool,
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
        /// Write/regenerate .scalex/noise.conf
        #[arg(long)]
        init: bool,
    },
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            println!("Error: {e}");
            return ExitCode::SUCCESS;
        }
    };
    if let Err(e) = run(cli) {
        println!("Error: {e:#}");
    }
    ExitCode::SUCCESS
}

fn run(cli: Cli) -> anyhow::Result<()> {

    match cli.command {
        Command::Overview => {
            let (reader, _) = open_index(cli.idx.as_deref())?;
            emit(query::commands::overview::cmd_overview(reader.index()));
            Ok(())
        }

        Command::Index { root } => {
            build_index_for_workspace(&root)?;
            Ok(())
        }

        Command::Search { q, limit, excl } => {
            if q.query.is_none() && q.module.is_none() {
                anyhow::bail!("Either <QUERY> or --module must be provided");
            }
            let (reader, idx_path) = open_index(cli.idx.as_deref())?;
            let scalex_dir = idx_path.parent().unwrap_or(Path::new("."));
            let exclude = resolve_exclude(&excl, reader.index(), scalex_dir);
            emit(query::commands::search::cmd_search(
                reader.index(),
                q.query.as_deref(),
                limit,
                q.kind.as_ref().map(SymbolKindArg::as_str),
                q.module.as_deref(),
                &exclude,
                excl.include_noise,
            ));
            Ok(())
        }


        Command::Info { fqn, excl } => {
            let (reader, idx_path) = open_index(cli.idx.as_deref())?;
            let scalex_dir = idx_path.parent().unwrap_or(Path::new("."));
            let exclude = resolve_exclude(&excl, reader.index(), scalex_dir);
            emit(query::commands::info::cmd_info(
                reader.index(),
                &fqn,
                &exclude,
            ));
            Ok(())
        }

        Command::Calls { fqn, depth, reverse, cross_module_only, excl } => {
            let (reader, idx_path) = open_index(cli.idx.as_deref())?;
            let scalex_dir = idx_path.parent().unwrap_or(Path::new("."));
            let exclude = resolve_exclude(&excl, reader.index(), scalex_dir);
            emit(query::commands::calls::cmd_calls(
                reader.index(),
                &fqn,
                depth,
                &exclude,
                reverse,
                cross_module_only,
            ));
            Ok(())
        }

        Command::Trace { fqn, depth, reverse, cross_module_only, excl } => {
            let (reader, idx_path) = open_index(cli.idx.as_deref())?;
            let scalex_dir = idx_path.parent().unwrap_or(Path::new("."));
            let exclude = resolve_exclude(&excl, reader.index(), scalex_dir);
            emit(query::commands::trace::cmd_trace(
                reader.index(),
                &fqn,
                depth,
                &exclude,
                reverse,
                cross_module_only,
            ));
            Ok(())
        }

        Command::Refs { fqn, limit } => {
            let (reader, _) = open_index(cli.idx.as_deref())?;
            emit(query::commands::refs::cmd_refs(
                reader.index(),
                &fqn,
                limit,
            ));
            Ok(())
        }

        Command::Noise { limit, init } => {
            let (reader, idx_path) = open_index(cli.idx.as_deref())?;
            if init {
                let scalex_dir = idx_path.parent().unwrap_or(Path::new("."));
                let conf_path = query::commands::noise::write_noise_conf(scalex_dir, reader.index())?;
                eprintln!("Wrote {}", conf_path.display());
            }
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

    let mut documents = ingest::semanticdb::load_all(&discovery.files)?;

    // Rewrite URIs for cross-compiled shared sources (e.g., out/.../jsSharedSources.dest/ → shared/src/)
    if let Some(ref m) = metadata {
        if !m.uri_rewrites.is_empty() {
            let mut rewritten = 0usize;
            for doc in &mut documents {
                for (from, to) in &m.uri_rewrites {
                    if doc.uri.starts_with(from.as_str()) {
                        doc.uri = format!("{}{}", to, &doc.uri[from.len()..]);
                        rewritten += 1;
                        break;
                    }
                }
            }
            if rewritten > 0 {
                eprintln!("Rewrote {rewritten} shared-source URI(s) to canonical paths");
            }
        }
    }

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

    // Generate noise config from the freshly built index
    let reader = index::reader::IndexReader::open(&idx_path)?;
    let conf_path = query::commands::noise::write_noise_conf(&scalex_dir, reader.index())?;
    eprintln!("Noise config: {}", conf_path.display());

    let t_total = t0.elapsed();
    eprintln!("Total: {:.1}s", t_total.as_secs_f64());

    Ok(idx_path)
}

/// Open the index, or bail if it doesn't exist.
/// Returns the reader and the resolved index file path (for locating .scalex/ dir).
fn open_index(idx_path: Option<&str>) -> anyhow::Result<(index::reader::IndexReader, PathBuf)> {
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

    let reader = index::reader::IndexReader::open(&path)?;
    Ok((reader, path))
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

/// Resolve exclude patterns: reads `.scalex/noise.conf` if it exists, otherwise
/// falls back to auto-computed noise. `--include-noise` disables noise entirely.
fn resolve_exclude(
    excl: &ExcludeArgs,
    index: &kodex::model::ArchivedKodexIndex,
    scalex_dir: &Path,
) -> Vec<String> {
    if excl.include_noise {
        // User explicitly wants noise — only apply manual --exclude if given
        return parse_exclude(excl.exclude.as_ref());
    }

    // Read config file, fall back to auto-compute if missing
    let noise_patterns = query::commands::noise::read_noise_conf(scalex_dir)
        .unwrap_or_else(|| {
            let pattern = query::commands::noise::compute_noise_exclude(index);
            parse_exclude(Some(&pattern))
        });

    let mut patterns = parse_exclude(excl.exclude.as_ref());
    patterns.extend(noise_patterns);
    patterns
}
