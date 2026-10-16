//! A panic in a thread goes to the error log. A separate test file is a separate process: the panic hook is global.

use daw_bootstrap::{SessionConfig, build_session_in, install_panic_hook};

#[test]
fn a_panic_in_any_thread_is_written_to_the_error_log() {
    let state = std::env::temp_dir().join(format!("daw-panic-log-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let session = build_session_in(&SessionConfig::new(state.clone()));
    install_panic_hook(session.controls.log.clone());

    let result = std::thread::spawn(|| {
        panic!("broken on purpose");
    })
    .join();
    assert!(result.is_err());

    let entries = session.controls.log.recent(10);
    let entry = entries.iter().find(|e| e.source == "panic").expect("the panic is logged");
    assert!(entry.message.contains("broken on purpose"), "{}", entry.message);
    assert!(entry.message.contains("panic_log.rs"), "the panic place is in the message: {}", entry.message);
    assert!(entry.details.as_deref().is_some_and(|d| !d.is_empty()), "the backtrace is saved");
    std::fs::remove_dir_all(state).ok();
}
