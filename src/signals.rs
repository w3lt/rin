//! Clean exit on `docker stop`.

use tokio::signal::unix::{SignalKind, signal};

/// Resolves on SIGTERM or SIGINT.
pub async fn terminated() {
    let mut term = signal(SignalKind::terminate()).expect("can listen for SIGTERM");
    let mut int = signal(SignalKind::interrupt()).expect("can listen for SIGINT");
    tokio::select! {
        _ = term.recv() => {}
        _ = int.recv() => {}
    }
}
