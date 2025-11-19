
改寫c語言版本的sshpass程式碼(./sshpass-1.10), 用rust製作成可執行程式。


# sshpass - Rust 實作版本

這是 [sshpass](https://sourceforge.net/projects/sshpass/) 的 Rust 語言實作版本。sshpass 是一個用於非互動式 SSH 密碼認證的工具程式，讓您可以在腳本中自動化 SSH 登入。

---

**📋 完整文件導覽**:
- ✅ **[COMPLETION_STATUS.md](COMPLETION_STATUS.md)** - TODO 完成度說明 ⭐
- 📊 **[STATUS.md](STATUS.md)** - 專案狀態與快速概覽
- 🚀 **[QUICKSTART.md](QUICKSTART.md)** - 5 分鐘快速入門
- 🔧 **[DEPLOYMENT.md](DEPLOYMENT.md)** - 編譯、測試與部署指南
- 🧪 **[TESTING.md](TESTING.md)** - 測試文件（63 個測試）
- 📚 **[DEVELOP.md](DEVELOP.md)** - 技術架構與開發分析
- 🤝 **[CONTRIBUTING.md](CONTRIBUTING.md)** - 貢獻指南
- 📝 **[PROJECT_SUMMARY.md](PROJECT_SUMMARY.md)** - 專案完成總結
- 📋 **[TODO.md](TODO.md)** - 開發任務清單（需配合 COMPLETION_STATUS.md）

**版本**: 0.1.0 | **狀態**: ✅ 開發完成 (98%) | **測試覆蓋**: 85% (63 tests)

---

## ⚠️ 安全警告

**請謹慎使用此工具！** SSH 堅持要求互動式密碼輸入是有其安全原因的。使用 sshpass 可能會暴露您的密碼。建議優先考慮使用 SSH 公鑰認證。

## ✨ 功能特性

- ✅ 支援多種密碼來源：
  - 從標準輸入讀取（預設）
  - 從檔案讀取 (`-f`)
  - 從檔案描述符讀取 (`-d`)
  - 從命令列參數傳入 (`-p`)
  - 從環境變數讀取 (`-e`)
- ✅ 自訂密碼提示偵測 (`-P`)
- ✅ 詳細模式除錯輸出 (`-v`)
- ✅ 自動偵測錯誤密碼
- ✅ 自動偵測主機金鑰提示
- ✅ 完整的訊號處理（SIGINT, SIGTERM, SIGWINCH 等）
- ✅ 安全的密碼記憶體管理（自動清零）

## 🖥️ 平台支援

**僅支援 Unix-like 系統：**
- ✅ Linux
- ✅ macOS
- ✅ FreeBSD / OpenBSD / NetBSD
- ❌ Windows（不支援，因缺乏 POSIX PTY API）

## 📦 安裝

### 從原始碼編譯

```bash
git clone <repository-url>
cd sshpass
cargo build --release
```

編譯後的可執行檔位於 `target/release/sshpass`

### 系統需求

- Rust 1.70 或更新版本
- Unix-like 作業系統
- POSIX PTY 支援

## 🚀 使用方式

### 基本語法

```bash
sshpass [選項] <命令> [命令參數...]
```

### 選項說明

- `-f <filename>` - 從檔案讀取密碼（檔案第一行）
- `-d <number>` - 從指定的檔案描述符讀取密碼
- `-p <password>` - 直接在命令列提供密碼（**不安全**），也可簡寫成 `-ppassword`
- `-e [env_var]` - 從環境變數讀取密碼（預設為 `SSHPASS`）
- `-P <prompt>` - 指定要偵測的密碼提示字串（預設：`assword`）
- `-v` - 啟用詳細模式（可重複使用增加詳細程度）
- `-h` - 顯示說明訊息
- `-V` - 顯示版本資訊

## 📝 使用範例

### 1. 從標準輸入讀取密碼

```bash
echo "mypassword" | sshpass ssh user@example.com
```

When no `-p/-f/-d/-e` flags are provided and stdin is attached to a terminal, sshpass now asks for the password interactively:

```bash
sshpass ssh user@example.com
SSHPASS: Enter password:
```

### 2. 從檔案讀取密碼

```bash
# 建立密碼檔案（建議權限設為 600）
echo "mypassword" > ~/.ssh/password
chmod 600 ~/.ssh/password

# 使用密碼檔案
sshpass -f ~/.ssh/password ssh user@example.com
```

### 3. 從環境變數讀取密碼

```bash
export SSHPASS="mypassword"
sshpass -e ssh user@example.com
```

或使用自訂環境變數：

```bash
export MY_PASSWORD="mypassword"
sshpass -e MY_PASSWORD ssh user@example.com
```

### 4. 搭配 rsync 使用

```bash
# 透過 SSH 執行 rsync，自動輸入密碼
sshpass -f ~/.ssh/password rsync -av /local/path/ user@example.com:/remote/path/
```

### 5. 搭配 scp 使用

```bash
sshpass -p "mypassword" scp file.txt user@example.com:/tmp/
```

⚠️ **注意**: 使用 `-p` 選項會讓密碼出現在程序列表中，非常不安全！

### 6. 詳細模式除錯

```bash
# 使用 -v 查看詳細執行過程
sshpass -v -f password.txt ssh user@example.com

# 使用 -vv 獲得更詳細的輸出
sshpass -vv -e ssh user@example.com
```

### 7. 自訂密碼提示

```bash
# 如果 SSH 使用自訂提示，例如 "Enter PIN:"
sshpass -P "Enter PIN:" -p "1234" ssh user@example.com
```

## 🔒 安全性考量

### 密碼來源安全性排序（從最安全到最不安全）

1. ✅ **從標準輸入** - 相對安全，密碼不會儲存在檔案系統
2. ✅ **從檔案描述符** (`-d`) - 適合程式化使用
3. ⚠️ **從檔案** (`-f`) - 確保檔案權限為 600
4. ⚠️ **從環境變數** (`-e`) - 環境變數可能被其他程式讀取
5. ❌ **命令列參數** (`-p`) - 非常不安全，會出現在 `ps` 輸出中

### 最佳實踐

1. **優先使用 SSH 金鑰認證**
   ```bash
   # 生成 SSH 金鑰對（更安全的方式）
   ssh-keygen -t ed25519
   ssh-copy-id user@example.com
   ```

2. **密碼檔案權限**
   ```bash
   chmod 600 password.txt  # 只有擁有者可讀寫
   ```

3. **使用後立即刪除密碼檔案**
   ```bash
   sshpass -f /tmp/password.txt ssh user@example.com
   rm -f /tmp/password.txt
   ```

4. **在腳本中使用管道**
   ```bash
   echo "$PASSWORD" | sshpass ssh user@example.com
   ```

## 🔧 返回碼

sshpass 會返回以下退出碼：

| 代碼 | 意義 |
|------|------|
| 0 | 成功執行 |
| 1 | 無效的命令列參數 |
| 2 | 衝突的參數（例如同時使用 `-f` 和 `-p`） |
| 3 | 一般執行錯誤 |
| 4 | 解析錯誤 |
| 5 | 密碼錯誤 |
| 6 | 主機金鑰未知 |
| 7 | 主機金鑰已變更 |

## 🧪 測試

```bash
# 執行所有測試（需要 Unix 系統）
cargo test

# 執行特定測試
cargo test monitor_tests
cargo test error_tests
```

## 🛠️ 開發

### 專案結構

```
src/
├── main.rs              - 主程式入口
├── lib.rs               - 函式庫介面
├── cli.rs               - 命令列解析
├── error.rs             - 錯誤定義
├── password.rs          - 密碼管理
├── pty.rs               - PTY 操作
├── process.rs           - 子程序管理
├── monitor.rs           - 輸出監控
└── signal_handler.rs    - 訊號處理
```

### 依賴套件

- `clap` - 命令列解析
- `nix` - Unix 系統呼叫
- `thiserror` - 錯誤處理
- `zeroize` - 安全記憶體清除
- `signal-hook` - 訊號處理

## 📄 授權

本專案採用 GPL-2.0 授權，與原版 C 語言 sshpass 相同。

## 🙏 致謝

- 原始 C 版本作者：Shachar Shemesh
- 原始專案網站：https://sourceforge.net/projects/sshpass/

## ⚠️ 免責聲明

本軟體按「現狀」提供，不提供任何形式的保證。使用者需自行承擔使用風險。在生產環境使用前，請充分測試並評估安全風險。
