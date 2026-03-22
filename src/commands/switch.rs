use crate::cli::SwitchArgs;
use crate::error::{GraftError, Result};
use crate::state::State;
use crate::util::is_pid_alive;

pub fn exec(args: SwitchArgs) -> Result {
    State::with_state(|state| {
        let ws = state.require_workspace(&args.name)?;

        let proc = ws
            .process
            .as_ref()
            .ok_or_else(|| GraftError::NoProcessRunning(args.name.clone()))?;

        if !is_pid_alive(proc.pid) {
            return Err(GraftError::NoProcessRunning(args.name.clone()));
        }

        let port = proc.port;

        if let Some(ref mut proxy) = state.proxy {
            proxy.active_workspace = Some(args.name.clone());
            proxy.target_port = Some(port);
            println!("switched to '{}' (port {})", args.name, port);
        } else {
            return Err(GraftError::ProxyFailed(
                "no proxy running — start a process first with `graft run`".to_string(),
            ));
        }

        Ok(())
    })
}
