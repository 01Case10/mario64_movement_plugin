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
mod m64;
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
