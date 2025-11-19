//! Windows PTY 實作（使用 portable-pty）
//!
//! 使用 portable-pty 提供可靠的 ConPTY 支持

use crate::error::{Result, SshpassError};
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

/// Windows 平台專用 PTY 包裝
pub struct Pty {
    master: Box<dyn portable_pty::MasterPty + Send>,
    pub(crate) reader: Arc<Mutex<Box<dyn Read + Send>>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
}

impl Pty {
    /// 建立新的 PTY 實例
    pub fn new() -> Result<Self> {
        eprintln!("SSHPASS: [DEBUG] Creating PTY using portable-pty...");

        let pty_system = NativePtySystem::default();

        let pty_size = PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system
            .openpty(pty_size)
            .map_err(|e| SshpassError::PtyCreationError(format!("Failed to create PTY: {}", e)))?;

        eprintln!("SSHPASS: [DEBUG] PTY created successfully");

        let reader = pair.master.try_clone_reader().map_err(|e| {
            SshpassError::PtyCreationError(format!("Failed to clone reader: {}", e))
        })?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| SshpassError::PtyCreationError(format!("Failed to take writer: {}", e)))?;

        Ok(Self {
            master: pair.master,
            reader: Arc::new(Mutex::new(reader)),
            writer: Arc::new(Mutex::new(writer)),
        })
    }

    /// 取得 slave PTY 供子進程使用
    pub(crate) fn take_slave(&mut self) -> Result<Box<dyn portable_pty::SlavePty + Send>> {
        // portable-pty 的 openpty 返回的是 pair，我們需要重新設計
        // 由於架構限制，我們需要在 spawn 時才創建 PTY
        Err(SshpassError::RuntimeError(
            "Slave PTY should be obtained during spawn".to_string(),
        ))
    }

    /// 非阻塞讀取 PTY 輸出
    pub fn read(&self, buffer: &mut [u8]) -> Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }

        let mut reader = self
            .reader
            .lock()
            .map_err(|_| SshpassError::WindowsError("Reader lock poisoned".into()))?;

        // portable-pty 的 reader 是阻塞的，我們需要設置為非阻塞
        // 使用 read 並捕獲 WouldBlock 錯誤
        match reader.read(buffer) {
            Ok(n) => Ok(n),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(0),
            Err(e) => {
                // EOF 或其他錯誤
                if e.kind() == std::io::ErrorKind::UnexpectedEof
                    || e.kind() == std::io::ErrorKind::BrokenPipe
                {
                    Ok(0)
                } else {
                    Err(SshpassError::WindowsError(format!(
                        "PTY read failed: {}",
                        e
                    )))
                }
            }
        }
    }

    /// 寫入所有資料至 PTY
    pub fn write_all(&self, data: &[u8]) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }

        let mut writer = self
            .writer
            .lock()
            .map_err(|_| SshpassError::WindowsError("Writer lock poisoned".into()))?;

        writer
            .write_all(data)
            .map_err(|e| SshpassError::WindowsError(format!("PTY write failed: {}", e)))?;

        writer
            .flush()
            .map_err(|e| SshpassError::WindowsError(format!("PTY flush failed: {}", e)))?;

        Ok(())
    }

    /// 調整終端視窗大小
    pub fn set_winsize(&self, rows: u16, cols: u16) -> Result<()> {
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        self.master
            .resize(size)
            .map_err(|e| SshpassError::WindowsError(format!("PTY resize failed: {}", e)))?;
        Ok(())
    }
}

/// 包含 PTY 和子進程的配對
pub struct PtyPair {
    pub pty: Pty,
    pub child: Box<dyn portable_pty::Child + Send + Sync>,
}

impl PtyPair {
    /// 創建 PTY 並啟動子進程
    pub fn spawn(command: &[String], verbose: bool) -> Result<Self> {
        if command.is_empty() {
            return Err(SshpassError::InvalidArguments(
                "No command specified".to_string(),
            ));
        }

        if verbose {
            eprintln!(
                "SSHPASS: [DEBUG] Command array has {} elements:",
                command.len()
            );
            for (i, arg) in command.iter().enumerate() {
                eprintln!("SSHPASS: [DEBUG]   [{}] = {:?}", i, arg);
            }
            eprintln!("SSHPASS: [DEBUG] Creating PTY using portable-pty...");
        }

        let pty_system = NativePtySystem::default();

        let pty_size = PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system
            .openpty(pty_size)
            .map_err(|e| SshpassError::PtyCreationError(format!("Failed to create PTY: {}", e)))?;

        if verbose {
            eprintln!("SSHPASS: Created Windows PTY (portable-pty)");
        }

        // 構建命令
        let mut cmd = CommandBuilder::new(&command[0]);
        for arg in &command[1..] {
            cmd.arg(arg);
        }

        if verbose {
            eprintln!("SSHPASS: [DEBUG] Spawning process...");
            eprintln!("SSHPASS: [DEBUG] Program: {:?}", command[0]);
            if command.len() > 1 {
                eprintln!("SSHPASS: [DEBUG] Args: {:?}", &command[1..]);
            }
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| SshpassError::ExecError(format!("Failed to spawn process: {}", e)))?;

        if verbose {
            eprintln!("SSHPASS: [DEBUG] Process spawned successfully");
            if let Some(pid) = child.process_id() {
                eprintln!("SSHPASS: Spawned child process with PID: {}", pid);
            }
        }

        let reader = pair.master.try_clone_reader().map_err(|e| {
            SshpassError::PtyCreationError(format!("Failed to clone reader: {}", e))
        })?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| SshpassError::PtyCreationError(format!("Failed to take writer: {}", e)))?;

        let pty = Pty {
            master: pair.master,
            reader: Arc::new(Mutex::new(reader)),
            writer: Arc::new(Mutex::new(writer)),
        };

        Ok(PtyPair { pty, child })
    }

    /// 嘗試非阻塞等待，若仍在執行則回傳 None
    pub fn try_wait(&mut self) -> Result<Option<i32>> {
        match self.child.try_wait() {
            Ok(Some(status)) => Ok(Some(status.exit_code() as i32)),
            Ok(None) => Ok(None),
            Err(e) => Err(SshpassError::WindowsError(format!(
                "Failed to wait for child: {}",
                e
            ))),
        }
    }

    /// 阻塞等待子行程結束
    pub fn wait(&mut self) -> Result<i32> {
        let status = self
            .child
            .wait()
            .map_err(|e| SshpassError::WindowsError(format!("Failed to wait for child: {}", e)))?;

        Ok(status.exit_code() as i32)
    }

    /// 強制終止子行程
    pub fn kill(&mut self) -> Result<()> {
        self.child
            .kill()
            .map_err(|e| SshpassError::WindowsError(format!("Failed to kill child: {}", e)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_pty_creation() {
        let pty = Pty::new();
        assert!(pty.is_ok());
    }
}
