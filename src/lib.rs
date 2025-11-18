// Platform-specific compilation guard
#[cfg(not(unix))]
compile_error!("sshpass requires a Unix-like operating system with PTY support. \
                Windows is not supported due to lack of POSIX PTY APIs.");

// Re-export modules for testing
#[cfg(unix)]
pub mod cli;
#[cfg(unix)]
pub mod error;
#[cfg(unix)]
pub mod monitor;
#[cfg(unix)]
pub mod password;
#[cfg(unix)]
pub mod process;
#[cfg(unix)]
pub mod pty;
#[cfg(unix)]
pub mod signal_handler;
