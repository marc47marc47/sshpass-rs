//! Windows 子行程管理（使用 portable-pty）

use crate::error::Result;
use crate::pty::{Pty, PtyPair};

/// 使用 PTY 包裝的 Windows 子行程
pub struct ChildProcess {
    pair: PtyPair,
    pub process_id: Option<u32>,
}

impl ChildProcess {
    /// 建立子行程並接上 PTY
    pub fn spawn(command: &[String], verbose: bool) -> Result<Self> {
        let pair = PtyPair::spawn(command, verbose)?;

        let process_id = pair.child.process_id();

        if verbose {
            if let Some(pid) = process_id {
                eprintln!("SSHPASS: Child process spawned with PID: {}", pid);
            }
        }

        Ok(Self { pair, process_id })
    }

    /// 嘗試非阻塞等待，若仍在執行則回傳 None
    pub fn try_wait(&mut self) -> Result<Option<i32>> {
        self.pair.try_wait()
    }

    /// 阻塞等待子行程結束
    pub fn wait(&mut self) -> Result<i32> {
        self.pair.wait()
    }

    /// 強制終止子行程
    pub fn kill(&mut self) -> Result<()> {
        self.pair.kill()
    }

    /// 獲取 PTY 的引用
    pub fn pty(&self) -> &Pty {
        &self.pair.pty
    }

    /// 獲取 PTY 的可變引用
    pub fn pty_mut(&mut self) -> &mut Pty {
        &mut self.pair.pty
    }
}

// 為了與 main.rs 兼容，添加一個字段訪問器
impl ChildProcess {
    pub fn pty_ref(&self) -> &Pty {
        &self.pair.pty
    }
}
