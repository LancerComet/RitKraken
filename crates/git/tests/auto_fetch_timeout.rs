mod support;

use std::{
    fs,
    io::{self, Read},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use ritkraken_git::{DEFAULT_AUTO_FETCH_TIMEOUT, Error, NewRemote, Repo};
use support::fixture;

/// Explicit acceptance check: a real Git HTTP helper connects but never receives a response.
#[test]
#[ignore = "runs the real 30-second automatic fetch timeout"]
fn automatic_fetch_abandons_a_stalled_remote_after_thirty_seconds() {
    assert_stalled_remote(DEFAULT_AUTO_FETCH_TIMEOUT, |repo| repo.auto_fetch());
}

#[test]
#[ignore = "runs the real configurable automatic fetch timeout"]
fn automatic_fetch_uses_a_custom_timeout() {
    let timeout = Duration::from_secs(2);
    assert_stalled_remote(timeout, |repo| repo.auto_fetch_with_timeout(timeout));
}

fn assert_stalled_remote(
    timeout: Duration,
    fetch: impl FnOnce(&Repo) -> ritkraken_git::Result<()>,
) {
    let (_directory, repo) = fixture();
    fs::write(repo.root().join("base.txt"), "base\n").unwrap();
    repo.stage(None).unwrap();
    repo.commit("Base", "").unwrap();
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let stopping = Arc::new(AtomicBool::new(false));
    let stop = stopping.clone();
    let (closed_tx, closed_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut socket, _)) => {
                    socket
                        .set_read_timeout(Some(Duration::from_millis(100)))
                        .unwrap();
                    let mut buffer = [0; 4096];
                    while !stop.load(Ordering::Relaxed) {
                        match socket.read(&mut buffer) {
                            Ok(0) => {
                                let _ = closed_tx.send(true);
                                return;
                            }
                            Ok(_) => {}
                            Err(error)
                                if matches!(
                                    error.kind(),
                                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                                ) => {}
                            Err(_) => {
                                let _ = closed_tx.send(true);
                                return;
                            }
                        }
                    }
                    return;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(error) => panic!("local server failed: {error}"),
            }
        }
    });
    repo.add_remote(
        &NewRemote::new("origin", &format!("http://{address}/stalled.git"), None).unwrap(),
    )
    .unwrap();
    let started = Instant::now();
    let result = fetch(&repo);
    let elapsed = started.elapsed();
    let connection_closed = closed_rx.recv_timeout(Duration::from_secs(5));
    stopping.store(true, Ordering::Relaxed);
    server.join().unwrap();
    assert!(
        matches!(result, Err(Error::Timeout(limit)) if limit == timeout),
        "{result:?}"
    );
    assert!(
        elapsed >= timeout && elapsed < timeout + Duration::from_secs(10),
        "{elapsed:?}"
    );
    assert!(
        connection_closed.is_ok(),
        "Git's remote helper left its connection open"
    );
    repo.snapshot(None).unwrap();
    repo.add_remote(&NewRemote::new("after-timeout", "../local", None).unwrap())
        .unwrap();
    eprintln!(
        "Automatic fetch stopped after {elapsed:?}; transport closed and repository is usable"
    );
}
