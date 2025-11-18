# sshpass Rust 版本開發分析文件

## 專案概述

本專案目標是將 C 語言版本的 sshpass 1.10 改寫為 Rust 可執行程式。

sshpass 是一個用於非互動式 SSH 密碼認證的工具程式。它通過偽終端 (pseudo-terminal, PTY) 控制 SSH 客戶端的 TTY，使 SSH 誤以為密碼來自互動式鍵盤用戶，從而實現自動化密碼輸入。

## 功能需求分析

### 核心功能

1. **密碼來源管理**
   - 從標準輸入讀取密碼 (預設)
   - 從檔案讀取密碼 (`-f filename`)
   - 從檔案描述符讀取密碼 (`-d number`)
   - 從命令列參數接收密碼 (`-p password`)
   - 從環境變數讀取密碼 (`-e [env_var]`)

2. **PTY (偽終端) 管理**
   - 建立主/從 PTY 對
   - 處理 PTY 權限設定 (grantpt)
   - 解鎖 PTY (unlockpt)
   - 管理控制 TTY 的關聯

3. **子程序管理**
   - Fork 子程序執行 SSH 命令
   - 設定子程序的會話 ID (setsid)
   - 設定子程序的控制 TTY
   - 監控子程序狀態

4. **輸出監控與密碼注入**
   - 監控 SSH 輸出以偵測密碼提示
   - 匹配密碼提示字串 (預設: "assword")
   - 在偵測到提示後自動輸入密碼
   - 偵測密碼錯誤 (重複提示)

5. **錯誤處理**
   - 偵測主機金鑰未知 ("The authenticity of host ")
   - 偵測主機金鑰變更 ("differs from the key for the IP address")
   - 返回適當的錯誤碼

6. **訊號處理**
   - SIGCHLD: 子程序狀態變更通知
   - SIGWINCH: 視窗大小調整
   - SIGTERM, SIGINT, SIGHUP: 終止訊號處理
   - SIGTSTP: 暫停訊號處理

7. **其他功能**
   - 自訂密碼提示字串 (`-P prompt`)
   - 詳細輸出模式 (`-v`)
   - 顯示說明 (`-h`)
   - 顯示版本資訊 (`-V`)

## 技術架構分析

### 資料結構

#### Args 結構體 (原 C 版本)
```c
struct {
    enum { PWT_STDIN, PWT_FILE, PWT_FD, PWT_PASS } pwtype;
    union {
        const char *filename;
        int fd;
        const char *password;
    } pwsrc;
    const char *pwprompt;
    int verbose;
    char *orig_password;
} args;
```

**Rust 對應設計建議:**
```rust
enum PasswordSource {
    Stdin,
    File(PathBuf),
    Fd(RawFd),
    Password(String),
}

struct Args {
    pw_source: PasswordSource,
    pw_prompt: Option<String>,
    verbose: bool,
}
```

### 返回碼定義

```rust
enum ReturnCode {
    NoError = 0,
    InvalidArguments = 1,
    ConflictingArguments = 2,
    RuntimeError = 3,
    ParseError = 4,
    IncorrectPassword = 5,
    HostKeyUnknown = 6,
    HostKeyChanged = 7,
}
```

### 核心流程

1. **命令列解析階段**
   - 解析選項參數
   - 驗證參數互斥性
   - 從指定來源讀取密碼
   - 安全地隱藏記憶體中的密碼

2. **PTY 建立階段**
   - 使用 posix_openpt 建立主 PTY
   - 設定非阻塞模式
   - 授予權限並解鎖
   - 取得從 PTY 名稱

3. **程序分叉階段**
   - Parent: 保持主 PTY 控制
   - Child:
     - 建立新會話 (setsid)
     - 開啟並設定從 PTY 為控制 TTY
     - 執行目標命令 (execvp)

4. **輸出監控迴圈**
   - 使用 pselect 監控主 PTY 輸出
   - 讀取並分析輸出內容
   - 狀態機匹配密碼提示/錯誤訊息
   - 在適當時機注入密碼

5. **訊號處理**
   - 設定訊號遮罩
   - 註冊訊號處理函數
   - 處理視窗大小調整
   - 轉發終止訊號給子程序

## Rust 實作考量

### 平台相容性

**主要挑戰: Windows 不支援 PTY**

原始 C 版本高度依賴 POSIX PTY API:
- `posix_openpt()`
- `grantpt()`
- `unlockpt()`
- `ptsname()`
- `setsid()`
- `TIOCSCTTY` ioctl

這些在 Windows 上不可用。解決方案:

1. **僅支援 Unix-like 系統** (建議)
   - Linux
   - macOS
   - BSD 系列
   - 使用條件編譯排除 Windows

2. **Windows 替代方案** (複雜)
   - 使用 ConPTY (Windows 10 1809+)
   - 需要完全不同的實作路徑

### 推薦的 Rust Crates

1. **命令列解析**
   - `clap` v4.x - 現代化的命令列解析器
   - 支援複雜的參數驗證

2. **PTY 操作**
   - `nix` - Unix 系統呼叫的 Rust 綁定
   - 提供 `openpty`, `forkpty` 等函數
   - 提供 `ioctl` 支援

3. **非同步 I/O**
   - `mio` - 跨平台的非阻塞 I/O
   - 或保持同步實作使用 `select`/`poll`

4. **訊號處理**
   - `signal-hook` - 安全的訊號處理
   - `nix::sys::signal` - 低階訊號控制

5. **錯誤處理**
   - `thiserror` - 自訂錯誤類型
   - `anyhow` - 簡化錯誤傳播

### 記憶體安全考量

C 版本的安全問題與 Rust 改善:

1. **密碼記憶體清理**
   - C: 手動覆寫記憶體 (`hide_password`)
   - Rust: 使用 `zeroize` crate 自動安全清理

2. **緩衝區溢位**
   - C: 固定大小緩衝區 `char buffer[256]`
   - Rust: `Vec<u8>` 自動管理，邊界檢查

3. **檔案描述符洩漏**
   - C: 需手動 `close()`
   - Rust: RAII 模式自動關閉

4. **訊號處理競爭條件**
   - C: 註解中承認存在 race condition
   - Rust: 使用 `signal-hook` 提供更安全的抽象

### 核心模組設計

```
src/
├── main.rs              # 程式入口點
├── cli.rs               # 命令列解析
├── password.rs          # 密碼來源管理
├── pty.rs               # PTY 操作封裝
├── process.rs           # 子程序管理
├── monitor.rs           # 輸出監控與匹配
├── signal_handler.rs    # 訊號處理
└── error.rs             # 錯誤定義
```

## 關鍵技術挑戰

### 1. PTY 狀態管理複雜性

**問題:** C 版本的註解 3.14159 詳細說明了 PTY 生命週期管理的複雜性:
- 主 PTY 在沒有從 PTY fd 開啟時會返回錯誤
- 需要在父子程序中都保持從 PTY 開啟
- OpenSSH 5.6+ 會關閉未知的 fd

**解決方案:**
- 使用 RAII 模式管理 PTY 生命週期
- 實作 `Drop` trait 確保正確清理
- 參考 C 版本的時序邏輯

### 2. 狀態機式字串匹配

**問題:** `match()` 函數實作簡單的狀態機匹配密碼提示

**Rust 實作:**
```rust
struct Matcher {
    reference: String,
    state: usize,
}

impl Matcher {
    fn feed(&mut self, buffer: &[u8]) -> bool {
        for &byte in buffer {
            if self.reference.as_bytes()[self.state] == byte {
                self.state += 1;
                if self.state == self.reference.len() {
                    return true;
                }
            } else {
                self.state = 0;
                if self.reference.as_bytes()[self.state] == byte {
                    self.state += 1;
                }
            }
        }
        false
    }
}
```

### 3. 可靠寫入保證

**問題:** `reliable_write()` 確保完整寫入

**Rust 實作:**
```rust
fn reliable_write(fd: RawFd, data: &[u8]) -> Result<(), io::Error> {
    let mut written = 0;
    while written < data.len() {
        match nix::unistd::write(fd, &data[written..]) {
            Ok(n) => written += n,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
```

## 安全性考量

### 1. 密碼處理

- **避免記憶體洩漏:** 使用 `zeroize` crate
- **避免 swap:** 考慮使用 `mlock` (需要 root)
- **命令列參數:** 警告 `-p` 選項的不安全性

### 2. 輸入驗證

- 檔案路徑驗證
- 檔案描述符範圍檢查
- 環境變數存在性檢查

### 3. 權限管理

- 檢查檔案權限 (密碼檔應該是 600)
- 警告不安全的檔案權限

## 測試策略

### 單元測試

1. 命令列解析測試
2. 密碼來源讀取測試
3. 字串匹配狀態機測試
4. 錯誤碼返回測試

### 整合測試

1. 與真實 SSH 互動測試
2. 錯誤密碼處理測試
3. 主機金鑰提示測試
4. 訊號處理測試

### 測試環境

- 建立本地 SSH 伺服器
- 使用已知密碼的測試帳號
- 模擬各種錯誤情境

## 開發里程碑

### Phase 1: 基礎架構 (Week 1)
- [ ] 專案初始化與依賴管理
- [ ] 錯誤類型定義
- [ ] 命令列解析實作
- [ ] 密碼來源管理

### Phase 2: 核心功能 (Week 2-3)
- [ ] PTY 操作封裝
- [ ] 程序分叉與執行
- [ ] 輸出監控與匹配
- [ ] 密碼注入邏輯

### Phase 3: 訊號處理 (Week 4)
- [ ] 訊號處理器註冊
- [ ] 視窗大小調整
- [ ] 終止訊號轉發

### Phase 4: 測試與優化 (Week 5)
- [ ] 單元測試
- [ ] 整合測試
- [ ] 效能優化
- [ ] 記憶體洩漏檢查

### Phase 5: 文件與發布 (Week 6)
- [ ] 使用文件
- [ ] API 文件
- [ ] 範例程式
- [ ] 發布準備

## 參考資料

### 原始 sshpass 專案
- 版本: 1.10
- 授權: GPL v2
- 來源: `./sshpass-1.10/`

### 技術規範
- POSIX PTY: IEEE Std 1003.1
- Terminal I/O: `termios(3)`
- Signal Handling: `signal(7)`

### Rust 生態系統
- [nix crate 文件](https://docs.rs/nix/)
- [clap crate 文件](https://docs.rs/clap/)
- [zeroize crate 文件](https://docs.rs/zeroize/)
- [signal-hook 文件](https://docs.rs/signal-hook/)

## 授權聲明

原始 sshpass 程式碼採用 GPL v2 授權。本 Rust 改寫版本也將遵循 GPL v2 授權條款。

---

文件版本: 1.0
建立日期: 2025-11-18
作者: Claude Code
