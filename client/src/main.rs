//! Native HTTP client entry point. All protocol output stays in its owning adapter.
mod cli;
mod environment;
mod globals;
mod http_client;
mod lsp;
mod output;
mod parser;
mod runner;
mod script;
mod storage;
mod variables;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    match cli::run().await {
        Ok(true) => std::process::ExitCode::SUCCESS,
        Ok(false) => std::process::ExitCode::FAILURE,
        Err(error) => {
            eprintln!("Sendline: {error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}
