mod server;
mod setup;
mod update;

use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Read};
use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use kendr_optimizer_contracts::{OptimizeRequest, RecoveryCapsule, UsageObservation};
use kendr_optimizer_core::Optimizer;
use serde::Serialize;
use serde::de::DeserializeOwned;

#[derive(Debug, Parser)]
#[command(name = "kendr-opt")]
#[command(about = "Provider-neutral token optimization without LLM routing")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Analyze an envelope in shadow mode and emit a hypothetical receipt.
    Analyze {
        #[arg(short, long, default_value = "-")]
        input: String,
        #[arg(short, long, default_value = "-")]
        output: String,
        #[arg(long)]
        compact: bool,
    },
    /// Optimize an envelope and emit content, receipt, and optional recovery data.
    Optimize {
        #[arg(short, long, default_value = "-")]
        input: String,
        #[arg(short, long, default_value = "-")]
        output: String,
        #[arg(long)]
        compact: bool,
    },
    /// Restore the complete original envelope from a recovery capsule.
    Restore {
        #[arg(short, long, default_value = "-")]
        input: String,
        #[arg(short, long, default_value = "-")]
        output: String,
        #[arg(long)]
        compact: bool,
    },
    /// Compare provider usage with an optional paired baseline.
    Observe {
        #[arg(short, long, default_value = "-")]
        input: String,
        #[arg(short, long, default_value = "-")]
        output: String,
        #[arg(long)]
        compact: bool,
    },
    /// List native engines and their declared risk levels.
    Engines {
        #[arg(short, long, default_value = "-")]
        output: String,
        #[arg(long)]
        compact: bool,
    },
    /// Run a local transform-only HTTP service. It never calls an LLM provider.
    Serve {
        #[arg(long, default_value = "127.0.0.1:7331")]
        bind: SocketAddr,
    },
    /// Install Kendr's repository-hosted adapter into an existing LLM harness.
    Setup {
        #[arg(value_enum)]
        harness: Option<setup::Harness>,
        /// List automatic and manual harness support without changing files.
        #[arg(long)]
        list: bool,
        /// Replace a same-name unmanaged adapter or an exclusive OpenClaw slot.
        #[arg(long)]
        force: bool,
    },
    /// Configure a harness, run the local optimizer, and launch the harness.
    Run {
        #[arg(value_enum)]
        harness: setup::Harness,
        /// Replace a conflicting adapter installation during setup.
        #[arg(long)]
        force: bool,
        /// Arguments passed to the harness after `--`.
        #[arg(last = true, allow_hyphen_values = true)]
        arguments: Vec<OsString>,
    },
    /// Check for or install a newer verified GitHub release.
    Update {
        /// Check for an update without downloading or replacing the executable.
        #[arg(long)]
        check: bool,
        /// Emit a versioned machine-readable result.
        #[arg(long)]
        json: bool,
        /// Release channel to follow.
        #[arg(long, value_enum)]
        channel: Option<update::Channel>,
        /// Allow updating an executable without an official install receipt.
        #[arg(long)]
        force: bool,
        /// Reinstall the same eligible version after full verification.
        #[arg(long)]
        reinstall: bool,
    },
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("kendr-opt: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    if matches!(
        &cli.command,
        Command::Setup { list: false, .. } | Command::Run { .. }
    ) {
        update::maybe_print_update_notice().await;
    }
    match cli.command {
        Command::Analyze {
            input,
            output,
            compact,
        } => {
            let optimizer = Optimizer::new();
            let request: OptimizeRequest = read_json(&input)?;
            let outcome = optimizer.analyze(&request)?;
            write_json(&output, &outcome, compact)?;
        }
        Command::Optimize {
            input,
            output,
            compact,
        } => {
            let optimizer = Optimizer::new();
            let request: OptimizeRequest = read_json(&input)?;
            let outcome = optimizer.optimize(&request)?;
            write_json(&output, &outcome, compact)?;
        }
        Command::Restore {
            input,
            output,
            compact,
        } => {
            let optimizer = Optimizer::new();
            let capsule: RecoveryCapsule = read_json(&input)?;
            let restored = optimizer.restore(&capsule)?;
            write_json(&output, &restored, compact)?;
        }
        Command::Observe {
            input,
            output,
            compact,
        } => {
            let optimizer = Optimizer::new();
            let observation: UsageObservation = read_json(&input)?;
            let savings = optimizer.observe(observation);
            write_json(&output, &savings, compact)?;
        }
        Command::Engines { output, compact } => {
            let optimizer = Optimizer::new();
            write_json(&output, &optimizer.engines(), compact)?;
        }
        Command::Serve { bind } => server::run(bind, Optimizer::new()).await?,
        Command::Setup {
            harness,
            list,
            force,
        } => {
            if list {
                println!("{}", setup::support_text());
            } else {
                for message in setup::setup(harness, force)? {
                    println!("{message}");
                }
            }
        }
        Command::Run {
            harness,
            force,
            arguments,
        } => setup::run(harness, &arguments, force)?,
        Command::Update {
            check,
            json,
            channel,
            force,
            reinstall,
        } => update::execute(check, json, channel, force, reinstall).await?,
    }

    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &str) -> Result<T, Box<dyn Error>> {
    let bytes = if path == "-" {
        let mut bytes = Vec::new();
        io::stdin().read_to_end(&mut bytes)?;
        bytes
    } else {
        fs::read(PathBuf::from(path))?
    };
    Ok(serde_json::from_slice(&bytes)?)
}

fn write_json<T: Serialize>(path: &str, value: &T, compact: bool) -> Result<(), Box<dyn Error>> {
    let mut bytes = if compact {
        serde_json::to_vec(value)?
    } else {
        serde_json::to_vec_pretty(value)?
    };
    bytes.push(b'\n');
    if path == "-" {
        use std::io::Write;
        io::stdout().write_all(&bytes)?;
    } else {
        fs::write(PathBuf::from(path), bytes)?;
    }
    Ok(())
}
