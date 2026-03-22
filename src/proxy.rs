use std::io;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::thread;

use crate::error::{GraftError, Result};
use crate::state::State;

pub fn run_proxy(listen_port: u16) -> Result {
    let listener = TcpListener::bind(("127.0.0.1", listen_port))
        .map_err(|e| GraftError::ProxyFailed(format!("failed to bind :{}: {}", listen_port, e)))?;

    eprintln!("proxy: listening on http://127.0.0.1:{}", listen_port);

    for stream in listener.incoming() {
        let client = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Re-read state on each connection to pick up hot-switched target ports
        thread::spawn(move || {
            let target_port = State::load()
                .ok()
                .and_then(|s| s.proxy)
                .and_then(|p| p.target_port)
                .unwrap_or(0);

            if target_port == 0 {
                return;
            }

            let Ok(upstream) = TcpStream::connect(("127.0.0.1", target_port)) else {
                return;
            };

            pipe(client, upstream);
        });
    }

    Ok(())
}

fn pipe(client: TcpStream, upstream: TcpStream) {
    let Ok(client_w) = client.try_clone() else {
        return;
    };
    let Ok(upstream_w) = upstream.try_clone() else {
        return;
    };

    let t1 = thread::spawn(move || {
        let mut src = client;
        let mut dst = upstream_w;
        io::copy(&mut src, &mut dst).ok();
        dst.shutdown(Shutdown::Write).ok();
    });

    let t2 = thread::spawn(move || {
        let mut src = upstream;
        let mut dst = client_w;
        io::copy(&mut src, &mut dst).ok();
        dst.shutdown(Shutdown::Write).ok();
    });

    t1.join().ok();
    t2.join().ok();
}
