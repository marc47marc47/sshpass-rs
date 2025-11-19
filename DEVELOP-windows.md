# Windows 實作開發規劃

## 概述

本文件說明在 Windows 平台上實作 sshpass 功能的開發策略。主要挑戰在於 Windows 不支援傳統的 Unix PTY（虛擬終端）機制，但提供了可實現類似功能的替代 API。

## 目前限制

當前實作（`src/` 目錄）僅支援 Unix 系統，原因如下：
- 使用 Unix PTY API (`nix::pty`)
- 使用 Unix 進程管理 (`fork()`, `exec()`)
- Unix 特定的信號處理
- 檔案描述符操作

## Windows PTY 替代方案

### 1. ConPTY (Console Pseudo-Console) - **建議選項**

**可用性**：Windows 10 1809（2018 年 10 月更新）及更高版本

**說明**：ConPTY 是 Microsoft 的原生虛擬主控台 API，讓主控台應用程式能透過類似 Unix PTY 的虛擬終端介面進行通訊。

**優點**：
- Windows 原生 API（無需外部依賴）
- 在現代 Windows 系統上具備一流支援
- 正確處理 VT100/ANSI 轉義序列
- Microsoft 提供完善文件
- 與 Windows Terminal 整合更佳

**需求**：
- Windows 10 Build 17763 或更高版本
- 透過 Rust 存取 Windows API

### 2. WinPTY - **舊版替代方案**

**說明**：WinPTY 是第三方函式庫，為較舊的 Windows 版本提供 PTY 功能。

**優點**：
- 支援較舊的 Windows 版本（Windows 7+）
- 更成熟且經過實戰測試

**缺點**：
- 需要外部原生函式庫
- 效能不如 ConPTY
- 額外的部署複雜度

## Rust 實作選項

### 選項 1：winpty-rs Crate（建議用於靈活性）

**Crate**：[`winpty-rs`](https://crates.io/crates/winpty-rs)

**特性**：
- 抽象化 WinPTY 和 ConPTY 兩種後端
- 自動選擇後端
- 明確指定後端（用於回退支援）
- 積極維護（2024 年 10 月更新）

**使用範例**：
```rust
use winpty_rs::{PTY, PTYArgs, PTYBackend};

// 自動選擇後端
let pty = PTY::new(&PTYArgs {
    cols: 80,
    rows: 24,
    mouse_mode: None,
    timeout: None,
    backend: None, // 自動選擇
})?;

// 或明確指定後端
let pty = PTY::new(&PTYArgs {
    cols: 80,
    rows: 24,
    mouse_mode: None,
    timeout: None,
    backend: Some(PTYBackend::ConPTY), // 強制使用 ConPTY
})?;

// 產生進程
pty.spawn(Command::new("ssh").arg("user@host"))?;

// 讀寫操作
let mut buffer = [0u8; 1024];
let n = pty.read(&mut buffer)?;
pty.write(b"password\n")?;
```

### 選項 2：conpty Crate（僅 ConPTY）

**Crate**：[`conpty`](https://github.com/zhiburt/conpty)

**特性**：
- 直接包裝 ConPTY API
- 輕量級
- ConPTY 特定功能

**使用範例**：
```rust
use conpty::spawn;

let proc = spawn("ssh user@host")?;
proc.write("password\n")?;
let output = proc.read()?;
```

### 選項 3：windows-rs + 手動實作

**Crate**：[`windows`](https://github.com/microsoft/windows-rs)

**特性**：
- 直接存取 Windows API
- 完全控制
- Microsoft 官方綁定

**複雜度**：高 - 需要手動呼叫 ConPTY API

## 提議的架構設計

### 平台抽象層

建立平台特定的抽象來處理 PTY 操作：

```
src/
├── lib.rs
├── main.rs
├── pty/
│   ├── mod.rs          # 公開 PTY 介面
│   ├── unix.rs         # Unix 實作（目前程式碼）
│   └── windows.rs      # Windows 實作（ConPTY）
├── process/
│   ├── mod.rs          # 公開進程介面
│   ├── unix.rs         # Unix fork/exec
│   └── windows.rs      # Windows CreateProcess
└── signal/
    ├── mod.rs          # 公開信號介面
    ├── unix.rs         # Unix 信號
    └── windows.rs      # Windows 主控台事件
```

### 核心 Trait 定義

```rust
// src/pty/mod.rs
pub trait PtyInterface {
    fn new() -> Result<Self> where Self: Sized;
    fn master_fd(&self) -> RawHandle;
    fn slave_name(&self) -> &str;
    fn read(&self, buffer: &mut [u8]) -> Result<usize>;
    fn write_all(&self, data: &[u8]) -> Result<()>;
    fn set_winsize(&self, rows: u16, cols: u16) -> Result<()>;
}

#[cfg(unix)]
pub type Pty = unix::UnixPty;

#[cfg(windows)]
pub type Pty = windows::WindowsPty;
```

### Windows 特定實作

```rust
// src/pty/windows.rs
use winpty_rs::{PTY, PTYArgs, PTYBackend};

pub struct WindowsPty {
    pty: PTY,
    input_handle: Handle,
    output_handle: Handle,
}

impl PtyInterface for WindowsPty {
    fn new() -> Result<Self> {
        let pty = PTY::new(&PTYArgs {
            cols: 80,
            rows: 24,
            mouse_mode: None,
            timeout: Some(1000),
            backend: Some(PTYBackend::ConPTY),
        })?;

        Ok(Self {
            pty,
            input_handle: pty.input_handle(),
            output_handle: pty.output_handle(),
        })
    }

    fn read(&self, buffer: &mut [u8]) -> Result<usize> {
        self.pty.read(buffer)
            .map_err(|e| SshpassError::SystemError(e))
    }

    fn write_all(&self, data: &[u8]) -> Result<()> {
        self.pty.write(data)
            .map_err(|e| SshpassError::SystemError(e))
    }

    // ... 其他實作
}
```

## 實作階段規劃

### 階段 1：研究與設定（1-2 天）
- 研究 Windows PTY 替代方案
- 識別合適的 Rust crate
- 設定 Windows 開發環境
- 使用 winpty-rs 建立概念驗證

### 階段 2：架構重構（3-5 天）
- 將目前 Unix 程式碼提取到平台特定模組
- 定義平台無關的 trait
- 建立條件編譯結構
- 重構 main.rs 使用平台抽象

### 階段 3：Windows 實作（5-7 天）
- 使用 winpty-rs 實作 WindowsPty
- 實作 Windows 進程產生
- 實作 Windows 信號/主控台事件處理
- 處理 VT100/ANSI 轉義序列
- 實作 Windows 密碼提示偵測

### 階段 4：測試與優化（3-5 天）
- 使用各種 SSH 客戶端測試（Windows 版 OpenSSH、PuTTY）
- 在不同 Windows 版本測試（Win10、Win11）
- 處理邊界情況（網路斷線、逾時）
- 效能優化

### 階段 5：文件與發布（2-3 天）
- 更新 README 加入 Windows 支援資訊
- 記錄 Windows 特定需求
- 建立 Windows 安裝指南
- 準備發布說明

**預估總時間**：14-22 天

## 技術挑戰

### 1. VT100/ANSI 轉義序列
**問題**：Windows 主控台應用程式可能產生與 Unix 不同的控制序列

**解決方案**：
- 使用 VT100 解析器 crate（例如 `vte`）
- 在模式匹配前標準化轉義序列
- 使用實際的 Windows SSH 客戶端測試

### 2. 密碼提示偵測
**問題**：Windows SSH 客戶端可能使用不同的提示格式

**解決方案**：
- 使用多種 Windows SSH 客戶端測試：
  - Windows 版 OpenSSH
  - PuTTY/Plink
  - Git Bash SSH
- 使提示模式可設定
- 加入 Windows 特定預設值

### 3. 進程生命週期管理
**問題**：Windows 進程 API 與 Unix 有顯著差異

**解決方案**：
- 使用 `windows` crate 進行進程控制
- 將 Unix 信號對應到 Windows 主控台事件：
  - SIGTERM → CTRL_C_EVENT
  - SIGKILL → TerminateProcess
  - SIGTSTP → 不直接支援

### 4. Handle vs 檔案描述符
**問題**：Windows 使用 HANDLE 而非檔案描述符

**解決方案**：
- 在 trait 中抽象化 handle 操作
- 使用 `std::os::windows::io::AsRawHandle`
- 提供平台特定的 handle 型別

### 5. 行尾符號差異
**問題**：Windows 使用 CRLF (`\r\n`)，Unix 使用 LF (`\n`)

**解決方案**：
- 在傳送密碼時標準化行尾符號
- 在輸出解析中處理 CRLF 和 LF

## 依賴項更新

加入到 `Cargo.toml`：

```toml
[target.'cfg(windows)'.dependencies]
winpty-rs = "1.0"
# 或直接使用 conpty：
# conpty = "0.5"

# 用於低階 Windows API 存取
windows = { version = "0.52", features = [
    "Win32_System_Console",
    "Win32_System_Threading",
    "Win32_Foundation",
] }

# 用於 VT100 解析
vte = "0.13"
```

## 測試策略

### Windows 特定測試

```rust
#[cfg(windows)]
#[test]
fn test_windows_pty_creation() {
    let pty = WindowsPty::new();
    assert!(pty.is_ok());
}

#[cfg(windows)]
#[test]
fn test_windows_ssh_password() {
    // 使用 Windows 版 OpenSSH 測試
    let result = run_sshpass_test("openssh");
    assert!(result.is_ok());
}

#[cfg(windows)]
#[test]
fn test_conpty_backend() {
    // 確保 ConPTY 可用
    assert!(is_conpty_available());
}
```

## 相容性矩陣

| Windows 版本 | ConPTY 支援 | WinPTY 支援 | 建議後端 |
|-------------|------------|-------------|---------|
| Windows 11     | ✅ 是      | ✅ 是       | ConPTY  |
| Windows 10 1809+ | ✅ 是    | ✅ 是       | ConPTY  |
| Windows 10 < 1809 | ❌ 否   | ✅ 是       | WinPTY  |
| Windows 8.1    | ❌ 否      | ✅ 是       | WinPTY  |
| Windows 7      | ❌ 否      | ✅ 是       | WinPTY  |

**建議**：以 Windows 10 1809+ 為目標以簡化實作，可選擇加入 WinPTY 回退支援舊版系統。

## 安全性考量

### Windows 特定安全性

1. **Handle 繼承**：確保 PTY handle 不會被子進程不必要地繼承
2. **進程隔離**：使用 Windows job objects 改善進程隔離
3. **憑證儲存**：考慮整合 Windows Credential Manager
4. **UAC 相容性**：在不同 UAC 等級下測試

### 安全 Handle 管理程式碼範例

```rust
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Threading::SetHandleInformation;

fn secure_handle(handle: HANDLE) -> Result<()> {
    unsafe {
        SetHandleInformation(
            handle,
            HANDLE_FLAG_INHERIT,
            0, // 不繼承
        )?;
    }
    Ok(())
}
```

## 替代方案

### 1. Named Pipes（具名管道）
使用 Windows named pipes 進行 IPC，而非 PTY。這較簡單，但可能無法與所有預期 TTY 的 SSH 客戶端配合。

### 2. PowerShell 整合
建立 PowerShell 包裝器，使用 .NET SSH 函式庫注入密碼。較不可攜但更符合 Windows 生態系統。

### 3. WSL 橋接
對於具備 WSL 的 Windows 10/11，建立橋接到在 WSL 中執行的 Unix 版本。增加 WSL 依賴但可重用現有程式碼。

## 參考資源

### 文件
- [Microsoft ConPTY 文件](https://docs.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session)
- [Windows Terminal ConPTY 原始碼](https://github.com/microsoft/terminal)
- [winpty-rs GitHub](https://github.com/andfoy/winpty-rs)
- [conpty crate GitHub](https://github.com/zhiburt/conpty)

### 範例專案
- Windows Terminal（C++ ConPTY 使用）
- Alacritty（跨平台終端機，支援 Windows）
- VSCode Terminal（在 Windows 上使用 ConPTY）

## 待決定問題

以下問題需要在實作前決定：

1. **最低 Windows 版本支援**
   - 選項：僅支援 Windows 10 1809+（ConPTY）或支援 Windows 7+（WinPTY 回退）
   - 需權衡簡潔性與向後相容性

2. **後端策略**
   - 選項 A：僅 ConPTY（現代 Windows）
   - 選項 B：ConPTY + WinPTY 回退（舊版支援）
   - 選項 C：自動偵測並選擇最佳可用後端

3. **Windows Credential Manager 整合**
   - 是否應整合 Windows 密碼儲存？
   - 安全性影響評估

4. **SSH 客戶端行為差異**
   - 如何處理不同 SSH 客戶端行為（OpenSSH vs PuTTY）？
   - 需要針對不同客戶端的特殊處理嗎？

5. **建置策略**
   - 提供單獨的 Windows 執行檔或通用建置？
   - 發布策略為何？

## 下一步行動

1. **立即行動**：設定 Rust Windows 開發環境
2. **短期目標**：建立最小 ConPTY 概念驗證
3. **中期目標**：開始平台抽象的架構重構
4. **長期目標**：完整 Windows 實作與測試

---

**狀態**：規劃階段
**最後更新**：2025-11-19
**維護者**：sshpass-rs 團隊
