mod server;

use std::error::Error;
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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let optimizer = Optimizer::new();

    match cli.command {
        Command::Analyze {
            input,
            output,
            compact,
        } => {
            let request: OptimizeRequest = read_json(&input)?;
            let outcome = optimizer.analyze(&request)?;
            write_json(&output, &outcome, compact)?;
        }
        Command::Optimize {
            input,
            output,
            compact,
        } => {
            let request: OptimizeRequest = read_json(&input)?;
            let outcome = optimizer.optimize(&request)?;
            write_json(&output, &outcome, compact)?;
        }
        Command::Restore {
            input,
            output,
            compact,
        } => {
            let capsule: RecoveryCapsule = read_json(&input)?;
            let restored = optimizer.restore(&capsule)?;
            write_json(&output, &restored, compact)?;
        }
        Command::Observe {
            input,
            output,
            compact,
        } => {
            let observation: UsageObservation = read_json(&input)?;
            let savings = optimizer.observe(observation);
            write_json(&output, &savings, compact)?;
        }
        Command::Engines { output, compact } => {
            write_json(&output, &optimizer.engines(), compact)?;
        }
        Command::Serve { bind } => server::run(bind, optimizer).await?,
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
