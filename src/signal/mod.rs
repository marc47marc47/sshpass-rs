//! Signal/Console Event 平台抽象層
//!
//! 此模組提供跨平台的信號和主控台事件處理介面。
//! 在 Unix 系統上使用 POSIX 信號，在 Windows 上使用主控台事件。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// 信號旗標結構
///
/// 用於跨執行緒通訊信號狀態（跨平台共用）
#[derive(Clone)]
pub struct SignalFlags {
    pub sigwinch_received: Arc<AtomicBool>,
    pub sigterm_received: Arc<AtomicBool>,
    pub sigint_received: Arc<AtomicBool>,
    pub sighup_received: Arc<AtomicBool>,
    pub sigtstp_received: Arc<AtomicBool>,
}

impl SignalFlags {
    /// Create a new set of signal flags
    pub fn new() -> Self {
        Self {
            sigwinch_received: Arc::new(AtomicBool::new(false)),
            sigterm_received: Arc::new(AtomicBool::new(false)),
            sigint_received: Arc::new(AtomicBool::new(false)),
            sighup_received: Arc::new(AtomicBool::new(false)),
            sigtstp_received: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Check if SIGWINCH (window resize) was received and clear the flag
    pub fn check_and_clear_sigwinch(&self) -> bool {
        self.sigwinch_received.swap(false, Ordering::SeqCst)
    }

    /// Check if SIGTERM was received
    pub fn check_sigterm(&self) -> bool {
        self.sigterm_received.load(Ordering::SeqCst)
    }

    /// Check if SIGINT was received
    pub fn check_sigint(&self) -> bool {
        self.sigint_received.load(Ordering::SeqCst)
    }

    /// Check if SIGHUP was received
    pub fn check_sighup(&self) -> bool {
        self.sighup_received.load(Ordering::SeqCst)
    }

    /// Check if SIGTSTP was received and clear the flag
    pub fn check_and_clear_sigtstp(&self) -> bool {
        self.sigtstp_received.swap(false, Ordering::SeqCst)
    }

    /// Check if any termination signal was received
    #[allow(dead_code)]
    pub fn should_terminate(&self) -> bool {
        self.check_sigterm() || self.check_sigint() || self.check_sighup()
    }

    /// Get the termination signal if any (Unix only)
    #[cfg(unix)]
    pub fn get_term_signal(&self) -> Option<nix::sys::signal::Signal> {
        use nix::sys::signal::{SIGHUP, SIGINT, SIGTERM};
        if self.check_sigterm() {
            Some(SIGTERM)
        } else if self.check_sigint() {
            Some(SIGINT)
        } else if self.check_sighup() {
            Some(SIGHUP)
        } else {
            None
        }
    }

    /// Get the termination signal type (Windows)
    #[cfg(windows)]
    pub fn get_term_signal(&self) -> Option<()> {
        if self.should_terminate() {
            Some(())
        } else {
            None
        }
    }
}

impl Default for SignalFlags {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::{forward_signal_to_child, handle_window_resize, setup_signal_handlers};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{forward_signal_to_child, handle_window_resize, setup_signal_handlers};
