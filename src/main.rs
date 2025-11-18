// This program only works on Unix-like systems due to PTY requirements
#![cfg(unix)]

mod cli;
mod error;
mod monitor;
mod password;
mod process;
mod pty;
mod signal_handler;

use cli::Cli;
use error::{Result, SshpassError};
use monitor::{MonitorResult, OutputMonitor};
use password::{read_password_from_env, PasswordSource, SecureString};
use process::ChildProcess;
use signal_handler::{forward_signal_to_child, handle_window_resize, setup_signal_handlers};

use nix::sys::select::{pselect, FdSet};
use nix::sys::signal::SigSet;
use std::os::fd::BorrowedFd;

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
    match run_program(&args, password, signal_flags) {
        Ok(exit_code) => exit_code,
        Err(e) => {
            eprintln!("SSHPASS: {}", e);
            e.exit_code()
        }
    }
}

/// Determine the password source from command line arguments
fn get_password_source(args: &Cli) -> PasswordSource {
    if let Some(ref path) = args.password_file {
        PasswordSource::File(path.clone())
    } else if let Some(fd) = args.password_fd {
        PasswordSource::Fd(fd)
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
fn read_password(args: &Cli, mut source: PasswordSource) -> Result<SecureString> {
    // Special handling for environment variables
    if let Some(ref env_var) = args.env_var {
        return read_password_from_env(env_var, args.is_verbose());
    }

    // Security warning for -p option
    if args.password.is_some() {
        eprintln!("SSHPASS: Warning: Using -p option is insecure. Consider using -f or -e instead.");
    }

    source.read_password(args.is_verbose())
}

/// Main program logic: spawn child and monitor output
fn run_program(
    args: &Cli,
    password: SecureString,
    signal_flags: signal_handler::SignalFlags,
) -> Result<i32> {
    let verbose = args.is_verbose();

    // Spawn the child process with PTY
    let child = ChildProcess::spawn(&args.command, verbose)?;

    if verbose {
        eprintln!("SSHPASS: Child process spawned with PID: {}", child.pid);
    }

    // Create output monitor
    let prompt = args.prompt.as_deref();
    let mut monitor = OutputMonitor::new(prompt, verbose);

    // Run the event loop
    run_event_loop(child, &password, &mut monitor, signal_flags, verbose)
}

/// Main event loop: monitor PTY output and handle signals
fn run_event_loop(
    child: ChildProcess,
    password: &SecureString,
    monitor: &mut OutputMonitor,
    signal_flags: signal_handler::SignalFlags,
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
                                        eprintln!("SSHPASS: Child exited with code {}, after EIO", exit_code);
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
                return Err(SshpassError::RuntimeError(format!(
                    "pselect error: {}",
                    e
                )));
            }
        }
    }
}
