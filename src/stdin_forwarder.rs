//! stdin 轉發器 - 將用戶輸入轉發到 PTY
//!
//! 在 Windows 上使用獨立執行緒讀取 stdin 並轉發

use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

#[cfg(unix)]
use std::io::{self, Read};

#[cfg(windows)]
use std::io;

#[cfg(windows)]
use windows::Win32::Storage::FileSystem::ReadFile;
#[cfg(windows)]
use windows::Win32::System::Console::{
    GetConsoleMode, GetStdHandle, ReadConsoleInputW, SetConsoleMode, CONSOLE_MODE,
    ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT, INPUT_RECORD, KEY_EVENT,
    STD_INPUT_HANDLE,
};

/// stdin 輸入事件
pub enum StdinEvent {
    Data(Vec<u8>),
    Eof,
}

/// stdin 轉發器
pub struct StdinForwarder {
    receiver: Receiver<StdinEvent>,
    #[cfg(windows)]
    original_mode: Option<CONSOLE_MODE>,
}

/// 檢查 stdin 是否為 console (Windows)
#[cfg(windows)]
fn is_stdin_console() -> bool {
    unsafe {
        if let Ok(handle) = GetStdHandle(STD_INPUT_HANDLE) {
            let mut mode = CONSOLE_MODE(0);
            GetConsoleMode(handle, &mut mode).is_ok()
        } else {
            false
        }
    }
}

impl StdinForwarder {
    /// 創建新的 stdin 轉發器並啟動後台執行緒
    pub fn new(verbose: bool) -> io::Result<Self> {
        if verbose {
            eprintln!("SSHPASS: [DEBUG] StdinForwarder::new() called");
        }

        let (sender, receiver) = channel();

        // 在 Windows 上設定 raw mode
        #[cfg(windows)]
        let original_mode = Self::set_raw_mode(verbose)?;

        if verbose {
            eprintln!("SSHPASS: [DEBUG] Spawning stdin reader thread...");
        }

        // 啟動後台執行緒讀取 stdin（捕獲 verbose 變數）
        thread::spawn(move || {
            Self::read_stdin_loop(sender, verbose);
        });

        if verbose {
            eprintln!("SSHPASS: [DEBUG] StdinForwarder created successfully");
        }

        Ok(Self {
            receiver,
            #[cfg(windows)]
            original_mode,
        })
    }

    /// 嘗試接收 stdin 事件（非阻塞）
    pub fn try_recv(&self) -> Option<StdinEvent> {
        self.receiver.try_recv().ok()
    }

    /// Windows: 設定 console 為 raw mode
    #[cfg(windows)]
    fn set_raw_mode(verbose: bool) -> io::Result<Option<CONSOLE_MODE>> {
        unsafe {
            let handle = GetStdHandle(STD_INPUT_HANDLE).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to get stdin handle: {}", e),
                )
            })?;

            let mut mode = CONSOLE_MODE(0);
            if GetConsoleMode(handle, &mut mode).is_err() {
                if verbose {
                    eprintln!("SSHPASS: [DEBUG] stdin is not a console (probably a pipe/file), skipping raw mode setup");
                }
                return Ok(None);
            }

            if verbose {
                eprintln!("SSHPASS: [DEBUG] stdin is a console, setting raw mode");
            }

            let original_mode = mode;

            // 移除 line input, echo, 和 processed input
            // 注意：不啟用 ENABLE_VIRTUAL_TERMINAL_INPUT，因為它會將按鍵轉換為 ANSI 序列
            mode.0 &= !(ENABLE_LINE_INPUT.0 | ENABLE_ECHO_INPUT.0 | ENABLE_PROCESSED_INPUT.0);
            // 不設置 ENABLE_VIRTUAL_TERMINAL_INPUT

            SetConsoleMode(handle, mode).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to set console mode: {}", e),
                )
            })?;

            if verbose {
                eprintln!(
                    "SSHPASS: [DEBUG] Console set to raw mode (mode={:#x})",
                    mode.0
                );
            }

            Ok(Some(original_mode))
        }
    }

    /// Unix: 設定終端為 raw mode
    #[cfg(unix)]
    fn set_raw_mode(_verbose: bool) -> io::Result<()> {
        // Unix 實作暫時省略，因為目前只需要 Windows
        Ok(())
    }

    /// 後台執行緒：持續讀取 stdin (Windows 版本)
    #[cfg(windows)]
    fn read_stdin_loop(sender: Sender<StdinEvent>, verbose: bool) {
        if verbose {
            eprintln!("SSHPASS: [DEBUG] Starting stdin read loop (Windows)");
        }

        // 檢查 stdin 是否為 console
        let is_console = is_stdin_console();
        if verbose {
            eprintln!("SSHPASS: [DEBUG] stdin is_console: {}", is_console);
        }

        if is_console {
            Self::read_console_loop(sender, verbose);
        } else {
            Self::read_pipe_loop(sender, verbose);
        }
    }

    /// 從 Console 讀取（使用 ReadConsoleInputW）
    #[cfg(windows)]
    fn read_console_loop(sender: Sender<StdinEvent>, verbose: bool) {
        if verbose {
            eprintln!("SSHPASS: [DEBUG] Using ReadConsoleInputW for console input");
        }

        unsafe {
            let handle = match GetStdHandle(STD_INPUT_HANDLE) {
                Ok(h) => h,
                Err(e) => {
                    if verbose {
                        eprintln!("SSHPASS: [DEBUG] Failed to get stdin handle: {}", e);
                    }
                    return;
                }
            };

            let mut input_buffer = [INPUT_RECORD::default(); 128];

            loop {
                let mut events_read = 0u32;

                match ReadConsoleInputW(handle, &mut input_buffer, &mut events_read) {
                    Ok(_) => {
                        for i in 0..events_read as usize {
                            let event = &input_buffer[i];

                            // 只處理鍵盤按下事件
                            if event.EventType == KEY_EVENT as u16 {
                                let key_event = unsafe { event.Event.KeyEvent };

                                // 只處理按鍵按下（不是釋放）
                                if key_event.bKeyDown.as_bool() {
                                    let char_code = unsafe { key_event.uChar.UnicodeChar };
                                    let vk_code = key_event.wVirtualKeyCode;

                                    // 過濾掉 vk_code == 0 的事件（這些通常是 ANSI 轉義序列）
                                    if vk_code == 0 {
                                        continue;
                                    }

                                    // 過濾掉非字符按鍵（方向鍵、功能鍵等）
                                    // VK codes: https://docs.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes

                                    // 跳過功能鍵、方向鍵等（0x21-0x2F, 0x70-0x87）
                                    if vk_code >= 0x21 && vk_code <= 0x2F {
                                        continue; // Page Up/Down, End, Home, 方向鍵等
                                    }
                                    if vk_code >= 0x70 && vk_code <= 0x87 {
                                        continue; // F1-F24
                                    }

                                    if char_code != 0 {
                                        // 將 UTF-16 字符轉換為 UTF-8
                                        let utf16_char = [char_code];
                                        if let Ok(s) = String::from_utf16(&utf16_char) {
                                            let mut bytes = s.into_bytes();

                                            // 將 Windows 的 \r (Enter) 轉換為 \n
                                            if bytes == vec![b'\r'] {
                                                bytes = vec![b'\n'];
                                            }

                                            if verbose {
                                                eprintln!("SSHPASS: [DEBUG] Console key: vk={:#04x}, char={:?}",
                                                    vk_code, String::from_utf8_lossy(&bytes));
                                            }

                                            // 立即發送每個字符，不累積
                                            if sender.send(StdinEvent::Data(bytes)).is_err() {
                                                if verbose {
                                                    eprintln!("SSHPASS: [DEBUG] Failed to send data - receiver closed");
                                                }
                                                return; // 接收端已關閉
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if verbose {
                            eprintln!("SSHPASS: [DEBUG] ReadConsoleInputW error: {}", e);
                        }
                        break;
                    }
                }
            }
        }

        if verbose {
            eprintln!("SSHPASS: [DEBUG] Console read loop terminated");
        }
    }

    /// 從管道讀取（使用 ReadFile）
    #[cfg(windows)]
    fn read_pipe_loop(sender: Sender<StdinEvent>, verbose: bool) {
        if verbose {
            eprintln!("SSHPASS: [DEBUG] Using ReadFile for pipe input");
        }

        unsafe {
            let handle = match GetStdHandle(STD_INPUT_HANDLE) {
                Ok(h) => h,
                Err(e) => {
                    if verbose {
                        eprintln!("SSHPASS: [DEBUG] Failed to get stdin handle: {}", e);
                    }
                    return;
                }
            };

            let mut buffer = vec![0u8; 256];
            loop {
                let mut bytes_read = 0u32;

                match ReadFile(handle, Some(&mut buffer), Some(&mut bytes_read), None) {
                    Ok(_) => {
                        if bytes_read == 0 {
                            // EOF
                            if verbose {
                                eprintln!("SSHPASS: [DEBUG] stdin EOF (pipe)");
                            }
                            let _ = sender.send(StdinEvent::Eof);
                            break;
                        }

                        if verbose {
                            eprintln!("SSHPASS: [DEBUG] stdin read {} bytes (pipe)", bytes_read);
                        }

                        let data = buffer[..bytes_read as usize].to_vec();
                        if sender.send(StdinEvent::Data(data)).is_err() {
                            break; // 接收端已關閉
                        }
                    }
                    Err(e) => {
                        if verbose {
                            eprintln!("SSHPASS: [DEBUG] stdin read error (pipe): {}", e);
                        }
                        let _ = sender.send(StdinEvent::Eof);
                        break;
                    }
                }
            }
        }

        if verbose {
            eprintln!("SSHPASS: [DEBUG] Pipe read loop terminated");
        }
    }

    /// 後台執行緒：持續讀取 stdin (Unix 版本)
    #[cfg(unix)]
    fn read_stdin_loop(sender: Sender<StdinEvent>, verbose: bool) {
        let mut stdin = io::stdin();
        let mut buffer = vec![0u8; 256];

        loop {
            match stdin.read(&mut buffer) {
                Ok(0) => {
                    // EOF
                    if verbose {
                        eprintln!("SSHPASS: [DEBUG] stdin EOF");
                    }
                    let _ = sender.send(StdinEvent::Eof);
                    break;
                }
                Ok(n) => {
                    if verbose {
                        eprintln!("SSHPASS: [DEBUG] stdin read {} bytes", n);
                    }
                    let data = buffer[..n].to_vec();
                    if sender.send(StdinEvent::Data(data)).is_err() {
                        break; // 接收端已關閉
                    }
                }
                Err(e) => {
                    if verbose {
                        eprintln!("SSHPASS: [DEBUG] stdin read error: {}", e);
                    }
                    break;
                }
            }
        }
    }
}

impl Drop for StdinForwarder {
    fn drop(&mut self) {
        // 恢復原始 console mode
        #[cfg(windows)]
        if let Some(original_mode) = self.original_mode {
            unsafe {
                if let Ok(handle) = GetStdHandle(STD_INPUT_HANDLE) {
                    let _ = SetConsoleMode(handle, original_mode);
                }
            }
        }
    }
}
