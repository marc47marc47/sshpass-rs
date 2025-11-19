//! Windows 控制訊號與視窗事件處理
//!
//! 透過 console control handler 及輪詢，模擬 Unix 下的 signal 行為。

use super::SignalFlags;
use crate::error::{Result, SshpassError};
use std::sync::atomic::Ordering;
use std::sync::OnceLock;
use std::time::Duration;
use windows::Win32::Foundation::{BOOL, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::System::Console::{
    GetConsoleScreenBufferInfo, GetStdHandle, SetConsoleCtrlHandler, CONSOLE_SCREEN_BUFFER_INFO,
    CTRL_BREAK_EVENT, CTRL_CLOSE_EVENT, CTRL_C_EVENT, STD_OUTPUT_HANDLE,
};

static SIGNAL_STATE: OnceLock<SignalFlags> = OnceLock::new();
static RESIZE_THREAD: OnceLock<()> = OnceLock::new();

/// 註冊 console handler 並回傳旗標物件
pub fn setup_signal_handlers() -> Result<SignalFlags> {
    if let Some(flags) = SIGNAL_STATE.get() {
        return Ok(flags.clone());
    }

    unsafe {
        SetConsoleCtrlHandler(Some(console_handler), BOOL(1)).map_err(|err| {
            SshpassError::WindowsError(format!(
                "Failed to register console control handler: {}",
                err
            ))
        })?;
    }

    let flags = SignalFlags::new();
    SIGNAL_STATE
        .set(flags.clone())
        .map_err(|_| SshpassError::WindowsError("Signal handler already registered".into()))?;
    start_resize_monitor(flags.clone());
    Ok(flags)
}

/// 視窗大小改變時更新 PTY
pub fn handle_window_resize(pty: &crate::pty::Pty) -> Result<()> {
    if let Some((rows, cols)) = current_console_size() {
        pty.set_winsize(rows, cols)
    } else {
        Err(SshpassError::WindowsError(
            "Unable to query console size".to_string(),
        ))
    }
}

/// 將控制事件轉發到子行程
pub fn forward_signal_to_child(
    _signal: (),
    child: &mut crate::process::ChildProcess,
    verbose: bool,
) -> Result<()> {
    if verbose {
        eprintln!("SSHPASS: Forwarding console event to child process");
    }

    match child.pty_ref().write_all(&[0x03]) {
        Ok(()) => Ok(()),
        Err(err) => {
            if verbose {
                eprintln!("SSHPASS: Failed to send Ctrl+C via PTY: {}", err);
                eprintln!("SSHPASS: Falling back to forcefully terminating child");
            }
            child.kill()
        }
    }
}

unsafe extern "system" fn console_handler(ctrl_type: u32) -> BOOL {
    if let Some(flags) = SIGNAL_STATE.get() {
        match ctrl_type {
            CTRL_C_EVENT => {
                flags.sigint_received.store(true, Ordering::SeqCst);
                return BOOL(1);
            }
            CTRL_BREAK_EVENT => {
                flags.sigterm_received.store(true, Ordering::SeqCst);
                return BOOL(1);
            }
            CTRL_CLOSE_EVENT => {
                flags.sighup_received.store(true, Ordering::SeqCst);
                return BOOL(1);
            }
            _ => {}
        }
    }
    BOOL(0)
}

fn start_resize_monitor(flags: SignalFlags) {
    RESIZE_THREAD.get_or_init(|| {
        std::thread::spawn(move || {
            let mut last_size = current_console_size();
            loop {
                let current_size = current_console_size();
                if current_size.is_some() && current_size != last_size {
                    flags.sigwinch_received.store(true, Ordering::SeqCst);
                }
                last_size = current_size;
                std::thread::sleep(Duration::from_millis(250));
            }
        });
        ()
    });
}

fn current_console_size() -> Option<(u16, u16)> {
    unsafe {
        let handle = match GetStdHandle(STD_OUTPUT_HANDLE) {
            Ok(h) if h != HANDLE(0) && h != INVALID_HANDLE_VALUE => h,
            _ => return None,
        };
        let mut info = CONSOLE_SCREEN_BUFFER_INFO::default();
        if GetConsoleScreenBufferInfo(handle, &mut info).is_ok() {
            let cols = (info.srWindow.Right - info.srWindow.Left + 1) as u16;
            let rows = (info.srWindow.Bottom - info.srWindow.Top + 1) as u16;
            Some((rows, cols))
        } else {
            None
        }
    }
}
