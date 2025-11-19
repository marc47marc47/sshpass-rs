//! Process (進程) 平台抽象層
//!
//! 此模組提供跨平台的進程管理介面。在 Unix 系統上使用 fork/exec，
//! 在 Windows 上使用 CreateProcess。

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::ChildProcess;

#[cfg(windows)]
mod windows_portable;
#[cfg(windows)]
pub use windows_portable::ChildProcess;
