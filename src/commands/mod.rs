pub(crate) mod completions;
pub mod diff;
pub mod drop;
pub(crate) mod enter;
pub mod fork;
pub mod ls;
pub(crate) mod merge;
pub mod nuke;
pub(crate) mod run;
pub(crate) mod switch;

use crate::cli::{Cli, Commands};
use crate::error::Result;

pub fn dispatch(cli: Cli) -> Result {
    match cli.command {
        Commands::Fork(args) => fork::exec(args),
        Commands::Drop(args) => drop::exec(args),
        Commands::Enter(args) => enter::exec(args),
        Commands::Run(args) => run::exec(args),
        Commands::Switch(args) => switch::exec(args),
        Commands::Ls(args) => ls::exec(args),
        Commands::Completions(args) => completions::exec(args.shell),
        Commands::Diff(args) => diff::exec(args),
        Commands::Merge(args) => merge::exec(args),
        Commands::Nuke(args) => nuke::exec(args),
        Commands::ProxyDaemon(args) => crate::proxy::run_proxy(args.port),
    }
}
