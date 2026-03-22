use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

use crate::cli::RunArgs;
use crate::error::{GraftError, Result};
use crate::state::{ProxyConfig, State};
use crate::util::{graft_home, is_pid_alive, kill_process};
use crate::workspace::RunningProcess;

const PORT_RANGE_START: u16 = 5501;
const PORT_RANGE_END: u16 = 5600;

fn spawn_detached(cmd: &mut Command) -> std::io::Result<std::process::Child> {
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        })
    };
    cmd.spawn()
}

fn open_log_pair(
    dir: &Path,
    out_name: &str,
    err_name: &str,
    wrap_err: impl Fn(String) -> GraftError,
) -> Result<(std::fs::File, std::fs::File)> {
    let stdout = std::fs::File::create(dir.join(out_name))
        .map_err(|e| wrap_err(format!("log file: {}", e)))?;
    let stderr = std::fs::File::create(dir.join(err_name))
        .map_err(|e| wrap_err(format!("log file: {}", e)))?;
    Ok((stdout, stderr))
}

fn is_port_available(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn allocate_port(state: &State) -> Result<u16> {
    let used_ports: Vec<u16> = state
        .workspaces
        .values()
        .filter_map(|ws| ws.process.as_ref())
        .map(|p| p.port)
        .collect();

    for port in PORT_RANGE_START..=PORT_RANGE_END {
        if !used_ports.contains(&port) && is_port_available(port) {
            return Ok(port);
        }
    }
    Err(GraftError::PortRangeExhausted)
}

fn ensure_proxy(state: &mut State, listen_port: u16) -> Result {
    if let Some(ref proxy) = state.proxy {
        if let Some(pid) = proxy.proxy_pid {
            if is_pid_alive(pid) {
                return Ok(());
            }
        }
    }

    let exe = std::env::current_exe()
        .map_err(|e| GraftError::ProxyFailed(format!("cannot find self: {}", e)))?;

    let log_dir = graft_home();
    if let Err(e) = std::fs::create_dir_all(&log_dir) {
        eprintln!("warning: failed to create log directory {}: {e}", log_dir.display());
    }

    let (stdout, stderr) = open_log_pair(&log_dir, "proxy.log", "proxy.err", GraftError::ProxyFailed)?;

    let child = spawn_detached(
        Command::new(exe)
            .args(["proxy-daemon", "--port", &listen_port.to_string()])
            .stdout(stdout)
            .stderr(stderr),
    )
    .map_err(|e| GraftError::ProxyFailed(format!("spawn proxy: {}", e)))?;

    state.proxy = Some(ProxyConfig {
        listen_port,
        active_workspace: None,
        target_port: None,
        proxy_pid: Some(child.id()),
    });

    std::thread::sleep(std::time::Duration::from_millis(100));

    Ok(())
}

pub fn exec(args: RunArgs) -> Result {
    if args.stop {
        return stop(&args.name);
    }

    if args.command.is_empty() {
        return Err(GraftError::InvalidArgument(
            "command required: graft run <workspace> -- <command>".to_string(),
        ));
    }

    State::with_state(|state| {
        let ws = state.require_workspace(&args.name)?;

        if let Some(ref proc) = ws.process {
            if is_pid_alive(proc.pid) {
                return Err(GraftError::ProcessAlreadyRunning(args.name.clone()));
            }
        }

        let app_port = args.port;
        let proxy_listen_port = allocate_port(state)?;
        let cmd = args.command.join(" ");
        let merged = state.require_workspace(&args.name)?.merged.clone();

        let log_dir = graft_home().join(&args.name);
        let (stdout, stderr) = open_log_pair(&log_dir, "stdout.log", "stderr.log", GraftError::ProcessFailed)?;

        let child = spawn_detached(
            Command::new("sh")
                .args(["-c", &cmd])
                .current_dir(&merged)
                .env("PORT", app_port.to_string())
                .env("GRAFT_WORKSPACE", &args.name)
                .stdout(stdout)
                .stderr(stderr),
        )
        .map_err(|e| GraftError::ProcessFailed(format!("spawn: {}", e)))?;

        let pid = child.id();

        let ws = state.require_workspace_mut(&args.name)?;
        ws.process = Some(RunningProcess {
            pid,
            command: cmd.clone(),
            port: app_port,
        });

        ensure_proxy(state, proxy_listen_port)?;

        if let Some(ref mut proxy) = state.proxy {
            proxy.active_workspace = Some(args.name.clone());
            proxy.target_port = Some(app_port);
        }

        let proxy_port = state.proxy.as_ref().map_or(proxy_listen_port, |p| p.listen_port);
        println!("running '{}' on port {} (PID {})", cmd, app_port, pid);
        println!("proxy: http://localhost:{}", proxy_port);

        Ok(())
    })
}

fn stop(name: &str) -> Result {
    State::with_state(|state| {
        let ws = state.require_workspace_mut(name)?;

        let proc = ws
            .process
            .take()
            .ok_or_else(|| GraftError::NoProcessRunning(name.to_string()))?;

        if is_pid_alive(proc.pid) {
            kill_process(proc.pid);
        }

        println!("stopped '{}' (PID {})", proc.command, proc.pid);
        Ok(())
    })
}
