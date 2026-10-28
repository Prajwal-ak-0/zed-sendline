//! Parse standalone CLI input and start the stdio language server.
use crate::{editor, runner, state::Storage};
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "sendline", version, about = "Run .http requests inside Zed")]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Send the request at the given one-based file line.
    Run {
        #[arg(long)]
        file: PathBuf,
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..))]
        line: u32,
        #[arg(long)]
        environment: Option<String>,
    },
    /// Serve request controls, environments, and diagnostics over standard LSP.
    Lsp {
        /// Require the version expected by the installed Zed extension.
        #[arg(long)]
        extension_version: Option<String>,
    },
}

pub async fn run() -> Result<bool> {
    let args = Args::parse();
    let storage = Storage::new()?;
    match args.command.unwrap_or(Command::Lsp {
        extension_version: None,
    }) {
        Command::Run {
            file,
            line,
            environment,
        } => runner::run(&file, line, environment.as_deref(), &storage).await,
        Command::Lsp { extension_version } => {
            if let Some(expected) = extension_version
                && expected != env!("CARGO_PKG_VERSION")
            {
                anyhow::bail!(
                    "The Zed extension requires Sendline {expected}. Update your custom binary or remove the executable override."
                );
            }
            editor::run(storage).await?;
            Ok(true)
        }
    }
}
