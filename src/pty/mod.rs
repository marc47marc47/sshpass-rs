//! PTY (Pseudo-Terminal) 平台抽象層
//!
//! 此模組提供跨平台的 PTY 操作介面。在 Unix 系統上使用傳統的 POSIX PTY，
//! 在 Windows 上使用 ConPTY (Console Pseudo-Console)。

use crate::error::Result;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::{get_terminal_winsize, Pty};

#[cfg(windows)]
mod windows_portable;
#[cfg(windows)]
pub use windows_portable::{Pty, PtyPair};

/// PTY 介面 trait
///
/// 定義跨平台 PTY 操作的共同介面
#[allow(dead_code)]
pub trait PtyInterface {
    /// 建立新的 PTY
    fn new() -> Result<Self>
    where
        Self: Sized;

    /// 取得 master 端的原始 handle/fd
    #[cfg(unix)]
    fn master_fd(&self) -> std::os::unix::io::RawFd;

    #[cfg(windows)]
    fn master_handle(&self) -> isize; // RawHandle

    /// 取得 slave 端的名稱或路徑
    fn slave_name(&self) -> &str;

    /// 從 PTY 讀取資料
    fn read(&self, buffer: &mut [u8]) -> Result<usize>;

    /// 寫入所有資料到 PTY
    fn write_all(&self, data: &[u8]) -> Result<()>;

    /// 設定視窗大小
    #[cfg(unix)]
    fn set_winsize(&self, winsize: &nix::pty::Winsize) -> Result<()>;

    #[cfg(windows)]
    fn set_winsize(&self, rows: u16, cols: u16) -> Result<()>;
}
