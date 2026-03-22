use crate::error::GraftError;
use crate::state::State;

pub fn run(name: String) -> Result<(), GraftError> {
    let state = State::load()?;
    let ws = state.require_workspace(&name)?;

    // Print only the path, no trailing newline, no color — for scripting: $(graft path test)
    print!("{}", ws.merged.display());

    Ok(())
}
