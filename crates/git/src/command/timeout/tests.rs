use std::{
    env, fs,
    fs::OpenOptions,
    io::{self, Write},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use super::{OUTPUT_LIMIT, Running, run_write_with_timeout};
use crate::Error;

const MODE: &str = "RITKRAKEN_TIMEOUT_TEST_MODE";
const LOCK: &str = "RITKRAKEN_TIMEOUT_TEST_LOCK";

fn helper_command(mode: &str) -> Command {
    let mut command = Command::new(env::current_exe().unwrap());
    command
        .args(["--exact", "command::timeout::tests::helper", "--nocapture"])
        .env(MODE, mode)
        .stdin(Stdio::null());
    command
}

#[test]
fn helper() {
    let Ok(mode) = env::var(MODE) else { return };
    match mode.as_str() {
        "success" => println!("fetch complete"),
        "failure" => {
            eprintln!("transport rejected the request");
            std::process::exit(23);
        }
        "noisy" => {
            for _ in 0..512 {
                io::stderr().write_all(&[b'x'; 4096]).unwrap();
            }
            std::process::exit(24);
        }
        "tree" => {
            helper_command("descendant")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap()
                .wait()
                .unwrap();
        }
        "descendant" => {
            let path = env::var_os(LOCK).unwrap();
            let file = OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .truncate(false)
                .open(&path)
                .unwrap();
            file.lock().unwrap();
            fs::write(Path::new(&path).with_extension("ready"), b"ready").unwrap();
            thread::sleep(Duration::from_secs(60));
            fs::write(
                Path::new(&path).with_extension("survived"),
                b"not cancelled",
            )
            .unwrap();
        }
        "blocked" => thread::sleep(Duration::from_secs(60)),
        _ => panic!("unknown helper mode"),
    }
}

#[test]
fn completed_commands_keep_their_exit_status_and_diagnostics() {
    run_write_with_timeout(helper_command("success"), Duration::from_secs(60)).unwrap();
    let error =
        run_write_with_timeout(helper_command("failure"), Duration::from_secs(60)).unwrap_err();
    let Error::Git { status, message } = error else {
        panic!("{error:?}")
    };
    assert_eq!(status.code(), Some(23));
    assert!(message.contains("transport rejected the request"));
}

#[test]
fn stalled_commands_timeout_and_subsequent_commands_can_run() {
    let limit = Duration::from_millis(50);
    let error = run_write_with_timeout(helper_command("blocked"), limit).unwrap_err();
    assert!(
        matches!(error, Error::Timeout(timeout) if timeout == limit),
        "{error:?}"
    );
    run_write_with_timeout(helper_command("success"), Duration::from_secs(60)).unwrap();
}

#[test]
fn verbose_helpers_cannot_block_the_waiter_or_allocate_unbounded_diagnostics() {
    let error =
        run_write_with_timeout(helper_command("noisy"), Duration::from_secs(60)).unwrap_err();
    let Error::Git { status, message } = error else {
        panic!("{error:?}")
    };
    assert_eq!(status.code(), Some(24));
    assert_eq!(message.len(), OUTPUT_LIMIT as usize);
}

#[test]
fn stopping_a_command_also_stops_its_running_transport_descendant() {
    let directory = tempfile::tempdir().unwrap();
    let lock = directory.path().join("transport.lock");
    let ready = lock.with_extension("ready");
    let mut command = helper_command("tree");
    command
        .env(LOCK, &lock)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut running = Running::spawn(&mut command).unwrap();
    // Synchronize with the descendant; this is not a process-startup speed test.
    let watchdog = Instant::now() + Duration::from_secs(30);
    while !ready.exists() {
        assert!(
            running.child.try_wait().unwrap().is_none(),
            "helper exited before locking"
        );
        assert!(Instant::now() < watchdog, "helper did not signal readiness");
        thread::sleep(Duration::from_millis(10));
    }
    running.stop().unwrap();
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock)
        .unwrap();
    // Wait for the OS to release the descendant's handle rather than asserting
    // that termination finishes within a scheduler-dependent number of ticks.
    file.lock().unwrap();
    assert!(
        !lock.with_extension("survived").exists(),
        "a transport descendant survived cancellation"
    );
}
