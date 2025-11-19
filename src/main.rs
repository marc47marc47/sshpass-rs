mod ansi;
mod cli;
mod error;
mod monitor;
mod password;
mod process;
mod pty;
mod signal;
mod stdin_forwarder;
mod terminal_response;

use cli::Cli;
use error::{Result, SshpassError};
use monitor::{MonitorResult, OutputMonitor};
use password::{read_password_from_env, PasswordSource, SecureString};
use process::ChildProcess;
use signal::{forward_signal_to_child, handle_window_resize, setup_signal_handlers};

#[cfg(unix)]
use nix::sys::select::{pselect, FdSet};
#[cfg(unix)]
use nix::sys::signal::SigSet;
#[cfg(unix)]
use std::os::fd::BorrowedFd;
#[cfg(windows)]
use std::time::Duration;

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    // Parse command line arguments
    let args = Cli::parse_args();

    // Validate arguments
    if let Err(e) = args.validate() {
        eprintln!("SSHPASS: {}", e);
        eprintln!("Use \"sshpass -h\" to get help");
        return e.exit_code();
    }

    // Determine password source
    let password_source = get_password_source(&args);

    // Read the password
    let password = match read_password(&args, password_source) {
        Ok(pw) => pw,
        Err(e) => {
            eprintln!("SSHPASS: {}", e);
            return e.exit_code();
        }
    };

    // Set up signal handlers
    let signal_flags = match setup_signal_handlers() {
        Ok(flags) => flags,
        Err(e) => {
            eprintln!("SSHPASS: Failed to setup signal handlers: {}", e);
            return e.exit_code();
        }
    };

    // Run the main program
    let result = run_program(&args, password, signal_flags);
    match result {
        Ok(exit_code) => {
            if args.is_verbose() {
                eprintln!("SSHPASS: Child exited with code {}", exit_code);
            }
            exit_code
        }
        Err(e) => {
            eprintln!("SSHPASS: {}", e);
            e.exit_code()
        }
    }
}

/// Determine the password source from command line arguments
fn get_password_source(args: &Cli) -> PasswordSource {
    #[cfg(unix)]
    if let Some(fd) = args.password_fd {
        return PasswordSource::Fd(fd);
    }

    if let Some(ref path) = args.password_file {
        PasswordSource::File(path.clone())
    } else if let Some(ref pw) = args.password {
        PasswordSource::Password(SecureString::new(pw.clone()))
    } else if args.env_var.is_some() {
        // Will be handled separately because we need to clear the env var
        PasswordSource::Stdin // Placeholder
    } else {
        PasswordSource::Stdin
    }
}

/// Read the password from the configured source
fn read_password(args: &Cli, source: PasswordSource) -> Result<SecureString> {
    // Special handling for environment variables
    if let Some(ref env_var) = args.env_var {
        return read_password_from_env(env_var, args.is_verbose());
    }

    // Security warning for -p option
    if args.password.is_some() {
        eprintln!(
            "SSHPASS: Warning: Using -p option is insecure. Consider using -f or -e instead."
        );
    }

    source.read_password(args.is_verbose())
}

/// Main program logic: spawn child and monitor output
fn run_program(
    args: &Cli,
    password: SecureString,
    signal_flags: signal::SignalFlags,
) -> Result<i32> {
    let verbose = args.is_verbose();
    if verbose {
        eprintln!("SSHPASS: Verbose logging enabled");
    }

    // Spawn the child process with PTY
    let child = match ChildProcess::spawn(&args.command, verbose) {
        Ok(child) => child,
        Err(e) => {
            eprintln!("SSHPASS: Failed to spawn child process: {}", e);
            return Err(e);
        }
    };

    if verbose {
        eprintln!("SSHPASS: Spawned child process (debug)");
        #[cfg(unix)]
        {
            eprintln!("SSHPASS: Child process spawned with PID: {}", child.pid);
        }
        #[cfg(windows)]
        {
            if let Some(pid) = child.process_id {
                eprintln!("SSHPASS: Child process spawned with PID: {}", pid);
            }
        }
    }

    // Create output monitor
    let prompt = args.prompt.as_deref();
    let mut monitor = OutputMonitor::new(prompt, verbose);

    // Run the event loop
    run_event_loop(child, &password, &mut monitor, signal_flags, verbose)
}

/// Main event loop: monitor PTY output and handle signals (Unix implementation)
#[cfg(unix)]
fn run_event_loop(
    child: ChildProcess,
    password: &SecureString,
    monitor: &mut OutputMonitor,
    signal_flags: signal::SignalFlags,
    verbose: bool,
) -> Result<i32> {
    let mut buffer = vec![0u8; 256];
    let master_fd = child.pty.master_fd();
    let mut terminated = false;

    // Handle initial window size
    if let Err(e) = handle_window_resize(&child.pty) {
        if verbose {
            eprintln!("SSHPASS: Warning: Failed to set initial window size: {}", e);
        }
    }

    loop {
        // Check for signals
        if signal_flags.check_and_clear_sigwinch() {
            if let Err(e) = handle_window_resize(&child.pty) {
                if verbose {
                    eprintln!("SSHPASS: Warning: Failed to handle window resize: {}", e);
                }
            }
        }

        if signal_flags.check_and_clear_sigtstp() {
            if let Err(e) = forward_signal_to_child(nix::sys::signal::SIGTSTP, &child, verbose) {
                if verbose {
                    eprintln!("SSHPASS: Warning: Failed to forward SIGTSTP: {}", e);
                }
            }
        }

        if let Some(sig) = signal_flags.get_term_signal() {
            if verbose {
                eprintln!("SSHPASS: Received termination signal, forwarding to child");
            }
            let _ = forward_signal_to_child(sig, &child, verbose);
            terminated = true;
        }

        // Check if child has exited
        if let Some(exit_code) = child.try_wait()? {
            if verbose {
                eprintln!("SSHPASS: Child process exited with code: {}", exit_code);
            }
            return Ok(exit_code);
        }

        if terminated {
            // Wait for child to exit
            return child.wait();
        }

        // Use pselect to monitor the PTY with signal handling
        let mut read_fds = FdSet::new();
        let master_fd_borrowed = unsafe { BorrowedFd::borrow_raw(master_fd) };
        read_fds.insert(&master_fd_borrowed);

        let empty_sigset = SigSet::empty();
        match pselect(
            master_fd + 1,
            Some(&mut read_fds),
            None,
            None,
            None,
            Some(&empty_sigset),
        ) {
            Ok(n) if n > 0 => {
                // Data available to read
                match child.pty.read(&mut buffer) {
                    Ok(0) => {
                        // EOF on PTY
                        if verbose {
                            eprintln!("SSHPASS: EOF on PTY");
                        }
                        // Continue to wait for child exit
                        continue;
                    }
                    Ok(n) => {
                        // Process the output
                        match monitor.handle_output(&buffer[..n]) {
                            MonitorResult::Continue => {
                                // Keep monitoring
                            }
                            MonitorResult::SendPassword => {
                                // Send the password
                                if verbose {
                                    eprintln!("SSHPASS: Sending password");
                                }
                                child.pty.write_all(password.as_bytes())?;
                                child.pty.write_all(b"\n")?;
                            }
                            MonitorResult::IncorrectPassword => {
                                // Wrong password, terminate
                                return Err(SshpassError::IncorrectPassword);
                            }
                            MonitorResult::HostKeyUnknown => {
                                return Err(SshpassError::HostKeyUnknown);
                            }
                            MonitorResult::HostKeyChanged => {
                                return Err(SshpassError::HostKeyChanged);
                            }
                        }
                    }
                    Err(e) => {
                        // Check if this is EIO (I/O error)
                        if let SshpassError::SystemError(nix_err) = &e {
                            if *nix_err == nix::errno::Errno::EIO {
                                // EIO can mean:
                                // 1. Child hasn't opened slave yet (temporary)
                                // 2. Child has terminated (permanent)
                                // Check if child is still running
                                if let Some(exit_code) = child.try_wait()? {
                                    if verbose {
                                        eprintln!(
                                            "SSHPASS: Child exited with code {}, after EIO",
                                            exit_code
                                        );
                                    }
                                    return Ok(exit_code);
                                }
                                // Child still running, EIO is temporary (slave not open yet)
                                // Continue to next iteration
                                continue;
                            }
                        }

                        if verbose {
                            eprintln!("SSHPASS: Read error: {}", e);
                        }
                        // For other errors, return the error
                        return Err(e);
                    }
                }
            }
            Ok(_) => {
                // Timeout or interrupted by signal
                continue;
            }
            Err(nix::errno::Errno::EINTR) => {
                // Interrupted by signal, continue
                continue;
            }
            Err(e) => {
                return Err(SshpassError::RuntimeError(format!("pselect error: {}", e)));
            }
        }
    }
}

/// PTY 輸出事件
#[cfg(windows)]
enum PtyEvent {
    Data(Vec<u8>),
    Eof,
    Error(String),
}

/// Main event loop: monitor PTY output and handle signals (Windows stub)
#[cfg(windows)]
fn run_event_loop(
    mut child: ChildProcess,
    password: &SecureString,
    monitor: &mut OutputMonitor,
    signal_flags: signal::SignalFlags,
    verbose: bool,
) -> Result<i32> {
    use std::sync::mpsc::channel;
    use std::thread;

    if verbose {
        eprintln!("SSHPASS: [DEBUG] Entering run_event_loop (Windows)");
    }

    let mut terminated = false;
    let mut empty_read_count = 0u32;
    let mut last_status_report = std::time::Instant::now();

    if verbose {
        eprintln!("SSHPASS: [DEBUG] About to create StdinForwarder...");
    }

    // 創建 stdin 轉發器
    let stdin_forwarder = stdin_forwarder::StdinForwarder::new(verbose).map_err(|e| {
        SshpassError::RuntimeError(format!("Failed to setup stdin forwarder: {}", e))
    })?;

    if verbose {
        eprintln!("SSHPASS: [DEBUG] StdinForwarder created");
    }

    // 創建 PTY 讀取線程
    let (pty_tx, pty_rx) = channel();
    let pty_reader = child.pty_ref().reader.clone();

    if verbose {
        eprintln!("SSHPASS: [DEBUG] Starting PTY reader thread...");
    }

    thread::spawn(move || {
        let mut buffer = vec![0u8; 512];
        loop {
            let mut reader = match pty_reader.lock() {
                Ok(r) => r,
                Err(_) => {
                    let _ = pty_tx.send(PtyEvent::Error("Reader lock poisoned".into()));
                    break;
                }
            };

            match reader.read(&mut buffer) {
                Ok(0) => {
                    // EOF
                    let _ = pty_tx.send(PtyEvent::Eof);
                    break;
                }
                Ok(n) => {
                    let data = buffer[..n].to_vec();
                    if pty_tx.send(PtyEvent::Data(data)).is_err() {
                        break; // 接收端已關閉
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // 非阻塞模式下沒有數據
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    continue;
                }
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::UnexpectedEof
                        || e.kind() == std::io::ErrorKind::BrokenPipe
                    {
                        let _ = pty_tx.send(PtyEvent::Eof);
                    } else {
                        let _ = pty_tx.send(PtyEvent::Error(format!("PTY read failed: {}", e)));
                    }
                    break;
                }
            }
        }
    });

    if verbose {
        eprintln!("SSHPASS: [DEBUG] PTY reader thread started, entering main loop");
    }

    let mut password_sent = false;

    if let Err(e) = handle_window_resize(child.pty_ref()) {
        if verbose {
            eprintln!("SSHPASS: Warning: Failed to set initial window size: {}", e);
        }
    }

    loop {
        // 處理 stdin 輸入（在密碼發送後才開始轉發）
        if password_sent {
            while let Some(event) = stdin_forwarder.try_recv() {
                match event {
                    stdin_forwarder::StdinEvent::Data(data) => {
                        if verbose {
                            eprintln!(
                                "SSHPASS: [DEBUG] Forwarding {} bytes from stdin to PTY",
                                data.len()
                            );
                        }
                        child.pty_ref().write_all(&data)?;
                    }
                    stdin_forwarder::StdinEvent::Eof => {
                        if verbose {
                            eprintln!("SSHPASS: [DEBUG] stdin EOF received (will continue reading PTY output)");
                        }
                        // 不要立即終止 - 繼續讀取 PTY 輸出直到子進程退出
                        // 這對於非互動式使用很重要（例如 echo "command" | sshpass ...）
                    }
                }
            }
        }

        if signal_flags.check_and_clear_sigwinch() {
            if let Err(e) = handle_window_resize(child.pty_ref()) {
                if verbose {
                    eprintln!("SSHPASS: Warning: Failed to handle window resize: {}", e);
                }
            }
        }

        if let Some(_) = signal_flags.get_term_signal() {
            if verbose {
                eprintln!("SSHPASS: Received console termination event, forwarding to child");
            }
            let _ = forward_signal_to_child((), &mut child, verbose);
            terminated = true;
        }

        if let Some(exit_code) = child.try_wait()? {
            if verbose {
                eprintln!("SSHPASS: Child process exited with code: {}", exit_code);
            }
            return Ok(exit_code);
        }

        if terminated {
            return child.wait();
        }

        // 處理 PTY 輸出
        match pty_rx.try_recv() {
            Ok(PtyEvent::Data(buffer)) => {
                empty_read_count = 0;

                if verbose {
                    eprintln!("SSHPASS: [DEBUG] PTY read {} bytes", buffer.len());
                    if buffer.len() < 100 {
                        eprintln!(
                            "SSHPASS: [DEBUG] Data: {:?}",
                            String::from_utf8_lossy(&buffer)
                        );
                    }
                }

                // Check for terminal queries (portable-pty handles these internally, but we log them)
                if let Some(response) = terminal_response::get_terminal_response(&buffer) {
                    if verbose {
                        eprintln!(
                            "SSHPASS: [DEBUG] Terminal query detected ({} bytes)",
                            response.len()
                        );
                        eprintln!("SSHPASS: [DEBUG] portable-pty handles these automatically");
                    }
                }

                let result = monitor.handle_output(&buffer);

                // 在密碼發送後，將所有 PTY 輸出轉發到 stdout
                if password_sent {
                    use std::io::Write;
                    let _ = std::io::stdout().write_all(&buffer);
                    let _ = std::io::stdout().flush();
                }

                match result {
                    MonitorResult::Continue => {
                        // Just continue monitoring
                    }
                    MonitorResult::SendPassword => {
                        if verbose {
                            eprintln!("SSHPASS: Sending password");
                        }
                        child.pty_ref().write_all(password.as_bytes())?;
                        child.pty_ref().write_all(b"\r\n")?;
                        password_sent = true; // 標記密碼已發送，開始轉發 stdin
                        if verbose {
                            eprintln!("SSHPASS: [DEBUG] Password sent, now forwarding stdin");
                        }
                    }
                    MonitorResult::IncorrectPassword => {
                        return Err(SshpassError::IncorrectPassword);
                    }
                    MonitorResult::HostKeyUnknown => {
                        return Err(SshpassError::HostKeyUnknown);
                    }
                    MonitorResult::HostKeyChanged => {
                        return Err(SshpassError::HostKeyChanged);
                    }
                }
            }
            Ok(PtyEvent::Eof) => {
                if verbose {
                    eprintln!("SSHPASS: [DEBUG] PTY EOF received");
                }
                // Continue to wait for child exit
            }
            Ok(PtyEvent::Error(e)) => {
                if verbose {
                    eprintln!("SSHPASS: [DEBUG] PTY read error: {}", e);
                }
                // Check if child has exited
                if let Some(exit_code) = child.try_wait()? {
                    if verbose {
                        eprintln!("SSHPASS: Child exited with code {}", exit_code);
                    }
                    return Ok(exit_code);
                }
            }
            Err(_) => {
                // No data available this iteration
                empty_read_count += 1;

                // Report status every 2 seconds if still getting empty reads
                if verbose && last_status_report.elapsed().as_secs() >= 2 {
                    eprintln!("SSHPASS: [STATUS] Still waiting for data... (empty reads: {}, elapsed: {:.1}s)",
                        empty_read_count,
                        last_status_report.elapsed().as_secs_f64());
                    last_status_report = std::time::Instant::now();
                }
            }
        }

        std::thread::sleep(Duration::from_millis(1));
    }
}
