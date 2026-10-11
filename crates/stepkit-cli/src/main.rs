//! `stepkit` command line interface.
//!
//! Subcommands (see the implementation plan, appendix A):
//! * `validate`        - check animation clips against the timing manifest
//! * `trace diff`      - compare a golden trace against a core run
//! * `trace summarize` - per-action duration tables and curve fits
//! * `trace pack`      - convert CSV traces to a compact binary form
//! * `trace import-m64`- convert published TAS input movies to scenarios
//! * `anim generate`   - generate the reference mannequin and clips
//! * `anim retime`     - resample a clip to the manifest grid

mod anim_cmd;
mod fuzz;
mod m64;
mod oracle_cmd;
mod parity_diff;
mod trace_cmd;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "stepkit", version, about = "SM64-style movement toolkit")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Validate animation clips against the timing manifest.
    Validate {
        #[arg(long)]
        manifest: String,
        #[arg(long)]
        clips: Vec<String>,
    },
    /// Trace tooling: diff, summarize, pack, import-m64.
    Trace {
        #[command(subcommand)]
        command: TraceCommands,
    },
    /// Animation tooling: generate, retime.
    Anim {
        #[command(subcommand)]
        command: AnimCommands,
    },
    /// Oracle tooling: run a scenario through the native decomp oracle.
    Oracle {
        #[command(subcommand)]
        command: OracleCommands,
    },
    /// Random-input differential fuzzer (plan §7.4).
    Fuzz {
        /// Comma-separated fixed seeds (CI mode). Overrides --seed-base/--seed-count/--random.
        #[arg(long)]
        seeds: Option<String>,
        /// First seed of the deterministic sequence (default mode).
        #[arg(long, default_value_t = 0)]
        seed_base: u64,
        /// Number of seeds to run from seed_base (or of random seeds with --random).
        #[arg(long, default_value_t = 8)]
        seed_count: u32,
        /// Nightly mode: fresh random seeds, printed for reproducibility.
        #[arg(long, default_value_t = false)]
        random: bool,
        /// Frames per fuzz case.
        #[arg(long, default_value_t = 600)]
        frames: u32,
        /// Comma-separated geometry subset (see --list-geoms). Default: all.
        #[arg(long)]
        geoms: Option<String>,
        /// Skip automatic minimisation of diverging inputs.
        #[arg(long, default_value_t = false)]
        no_minimize: bool,
        /// List geometry presets and exit.
        #[arg(long, default_value_t = false)]
        list_geoms: bool,
        /// Directory for minimized repro scenarios and temp files.
        #[arg(long, default_value = "traces/fuzz")]
        out_dir: String,
        /// Only print failures.
        #[arg(long, default_value_t = false)]
        quiet: bool,
    },
}

#[derive(Subcommand)]
enum TraceCommands {
    Diff {
        #[arg(long)]
        scenario: Option<String>,
        #[arg(long)]
        golden: Option<String>,
        #[arg(long, default_value = "free")]
        mode: String,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        report: Option<String>,
    },
    Summarize {
        #[arg(long)]
        golden: String,
        #[arg(long)]
        out: String,
    },
    Pack {
        #[arg(long)]
        input: String,
        #[arg(long)]
        out: String,
    },
    ImportM64 {
        #[arg(long)]
        input: String,
        #[arg(long)]
        frames: Option<String>,
        #[arg(long)]
        out: String,
    },
    /// Run a scenario through the core and write the trace CSV + sidecar.
    Run {
        #[arg(long)]
        scenario: String,
        #[arg(long)]
        out: String,
    },
}

#[derive(Subcommand)]
enum AnimCommands {
    Generate {
        #[arg(long)]
        manifest: String,
        #[arg(long)]
        out: String,
    },
    Retime {
        #[arg(long)]
        clip: String,
        #[arg(long)]
        manifest: String,
        #[arg(long)]
        slot: String,
        #[arg(long)]
        out: String,
    },
    Validate {
        #[arg(long)]
        manifest: String,
        #[arg(long)]
        clips: String,
        #[arg(long)]
        bone_map: Option<String>,
    },
}

#[derive(Subcommand)]
enum OracleCommands {
    Run {
        #[arg(long)]
        scenario: String,
        #[arg(long)]
        out: String,
    },
    Diff {
        #[arg(long)]
        scenario: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        fields: Option<String>,
        #[arg(long, default_value = "free")]
        mode: String,
    },
}

fn run() -> Result<i32, String> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Validate { .. } => Err("stepkit validate: not yet implemented (phase 6)".into()),
        Commands::Trace { command } => match command {
            TraceCommands::Diff {
                scenario,
                golden,
                mode,
                all,
                report,
            } => {
                if all {
                    return Err("--all is not yet implemented; pass --scenario and --golden".into());
                }
                let passed = trace_cmd::diff(
                    scenario.as_deref(),
                    golden.as_deref(),
                    &mode,
                    report.as_deref(),
                )?;
                Ok(i32::from(!passed))
            }
            TraceCommands::Summarize { golden, out } => {
                trace_cmd::summarize(&golden, &out)?;
                Ok(0)
            }
            TraceCommands::Pack { input, out } => {
                trace_cmd::pack(&input, &out)?;
                Ok(0)
            }
            TraceCommands::ImportM64 { input, frames, out } => {
                trace_cmd::import_m64(&input, frames.as_deref(), &out)?;
                Ok(0)
            }
            TraceCommands::Run { scenario, out } => {
                trace_cmd::run(&scenario, &out)?;
                Ok(0)
            }
        },
        Commands::Anim { command } => match command {
            AnimCommands::Generate { manifest, out } => {
                anim_cmd::generate_reference(&manifest, &out)?;
                Ok(0)
            }
            AnimCommands::Retime { .. } => Err("stepkit anim retime: not yet implemented".into()),
            AnimCommands::Validate {
                manifest,
                clips,
                bone_map,
            } => {
                anim_cmd::validate(&manifest, &clips, bone_map.as_deref())?;
                Ok(0)
            }
        },
        Commands::Oracle { command } => match command {
            OracleCommands::Run { scenario, out } => {
                oracle_cmd::run(&scenario, &out)?;
                Ok(0)
            }
            OracleCommands::Diff {
                scenario,
                all,
                fields,
                mode,
            } => {
                if scenario.is_some() && all {
                    return Err("--scenario and --all are mutually exclusive".into());
                }
                if scenario.is_none() && !all {
                    return Err("either --scenario or --all is required".into());
                }
                let passed = parity_diff::diff(scenario.as_deref(), all, fields.as_deref(), &mode)?;
                Ok(i32::from(!passed))
            }
        },
        Commands::Fuzz {
            seeds,
            seed_base,
            seed_count,
            random,
            frames,
            geoms,
            no_minimize,
            list_geoms,
            out_dir,
            quiet,
        } => {
            let passed = fuzz::fuzz(
                seeds.as_deref(),
                seed_base,
                seed_count,
                random,
                frames,
                geoms.as_deref(),
                no_minimize,
                list_geoms,
                &out_dir,
                quiet,
            )?;
            Ok(i32::from(!passed))
        }
    }
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("stepkit: {e}");
            std::process::exit(2);
        }
    }
}
