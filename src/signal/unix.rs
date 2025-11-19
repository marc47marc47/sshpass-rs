//! Unix 信號處理實作
//!
//! 使用 POSIX 信號處理機制

use super::SignalFlags;
use crate::error::Result;
use nix::sys::signal::{Signal, SIGINT, SIGTSTP};
use std::sync::Arc;

/// Set up signal handlers for the application
///
/// This function registers signal handlers that set atomic flags when
/// signals are received. The main loop can check these flags to respond
/// to signals appropriately.
pub fn setup_signal_handlers() -> Result<SignalFlags> {
    use signal_hook::consts::signal::*;
    use signal_hook::flag;

    let flags = SignalFlags::new();

    // Register SIGWINCH (window size change)
    flag::register(SIGWINCH, Arc::clone(&flags.sigwinch_received)).map_err(|e| {
        crate::error::SshpassError::RuntimeError(format!(
            "Failed to register SIGWINCH handler: {}",
            e
        ))
    })?;

    // Register SIGTERM
    flag::register(SIGTERM, Arc::clone(&flags.sigterm_received)).map_err(|e| {
        crate::error::SshpassError::RuntimeError(format!(
            "Failed to register SIGTERM handler: {}",
            e
        ))
    })?;

    // Register SIGINT (Ctrl-C)
    flag::register(SIGINT, Arc::clone(&flags.sigint_received)).map_err(|e| {
        crate::error::SshpassError::RuntimeError(format!(
            "Failed to register SIGINT handler: {}",
            e
        ))
    })?;

    // Register SIGHUP
    flag::register(SIGHUP, Arc::clone(&flags.sighup_received)).map_err(|e| {
        crate::error::SshpassError::RuntimeError(format!(
            "Failed to register SIGHUP handler: {}",
            e
        ))
    })?;

    // Register SIGTSTP (Ctrl-Z)
    flag::register(SIGTSTP, Arc::clone(&flags.sigtstp_received)).map_err(|e| {
        crate::error::SshpassError::RuntimeError(format!(
            "Failed to register SIGTSTP handler: {}",
            e
        ))
    })?;

    Ok(flags)
}

/// Handle window resize signal by updating PTY window size
pub fn handle_window_resize(pty: &crate::pty::Pty) -> Result<()> {
    if let Some(winsize) = crate::pty::get_terminal_winsize() {
        pty.set_winsize(&winsize)?;
    }
    Ok(())
}

/// Forward a signal to the child process
///
/// SIGINT and SIGTSTP are sent as control characters to the PTY,
/// other signals are sent directly to the process.
pub fn forward_signal_to_child(
    signal: Signal,
    child: &crate::process::ChildProcess,
    verbose: bool,
) -> Result<()> {
    match signal {
        SIGINT => {
            // Send Ctrl-C (0x03) to the PTY
            if verbose {
                eprintln!("SSHPASS: Forwarding SIGINT as Ctrl-C");
            }
            child.pty.write_all(&[0x03])?;
        }
        SIGTSTP => {
            // Send Ctrl-Z (0x1a) to the PTY
            if verbose {
                eprintln!("SSHPASS: Forwarding SIGTSTP as Ctrl-Z");
            }
            child.pty.write_all(&[0x1a])?;
        }
        _ => {
            // Send signal directly to the child process
            if verbose {
                eprintln!("SSHPASS: Forwarding signal {:?} to child", signal);
            }
            child.kill(signal)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nix::sys::signal::SIGTERM;

    #[test]
    fn test_signal_flags_creation() {
        let flags = SignalFlags::new();
        assert!(!flags.check_sigterm());
        assert!(!flags.check_sigint());
        assert!(!flags.check_sighup());
        assert!(!flags.should_terminate());
    }

    #[test]
    fn test_signal_flags_check_and_clear() {
        use std::sync::atomic::Ordering;
        let flags = SignalFlags::new();

        // Set a flag
        flags.sigwinch_received.store(true, Ordering::SeqCst);
        assert!(flags.check_and_clear_sigwinch());
        // Should be cleared now
        assert!(!flags.check_and_clear_sigwinch());
    }

    #[test]
    fn test_should_terminate() {
        use std::sync::atomic::Ordering;
        let flags = SignalFlags::new();

        flags.sigterm_received.store(true, Ordering::SeqCst);
        assert!(flags.should_terminate());
        assert_eq!(flags.get_term_signal(), Some(SIGTERM));
    }
}
