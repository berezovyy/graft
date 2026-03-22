pub mod collapse;
pub mod diff;
pub mod drop;
pub mod enter;
pub mod fork;
pub mod ls;
pub mod merge;
pub mod nuke;
pub mod path;
pub mod snap;
pub mod tree;

use crate::cli::{Cli, Commands};
use crate::error::GraftError;

pub fn dispatch(cli: Cli) -> Result<(), GraftError> {
    match cli.command {
        Commands::Fork(args) => fork::run(args),
        Commands::Drop(args) => drop::run(args),
        Commands::Ls(_) => ls::run(),
        Commands::Path(args) => path::run(args.name),
        Commands::Diff(args) => diff::run(args),
        Commands::Enter(args) => enter::run(args),
        Commands::Merge(args) => merge::run(args),
        Commands::Snap(args) => snap::run(args.action),
        Commands::Tree(_) => tree::run(),
        Commands::Collapse(args) => collapse::run(args.name),
        Commands::Nuke(_) => nuke::run(),
    }
}
