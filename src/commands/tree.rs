use crate::error::GraftError;
use crate::state::State;
use crate::tree;

pub fn run() -> Result<(), GraftError> {
    let state = State::load()?;
    let workspaces: Vec<&_> = state.list_workspaces();
    let forest = tree::build_hierarchy(&workspaces);
    let output = tree::format_hierarchy(&forest);
    println!("{output}");
    Ok(())
}
