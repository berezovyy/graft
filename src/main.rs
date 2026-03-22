use clap::Parser;
use owo_colors::OwoColorize;

use graft::cli::Cli;
use graft::commands;

fn main() {
    let cli = Cli::parse();
    if let Err(e) = commands::dispatch(cli) {
        eprintln!("{} {e}", "error:".red());
        std::process::exit(1);
    }
}
