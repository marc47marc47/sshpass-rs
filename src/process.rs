use crate::error::{Result, SshpassError};
use crate::pty::Pty;
use nix::fcntl::OFlag;
use nix::sys::signal::{sigprocmask, SigSet, SigmaskHow};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{close, execvp, fork, setsid, ForkResult, Pid};
use std::ffi::CString;
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;

/// Represents a child process running with a PTY
pub struct ChildProcess {
    pub pid: Pid,
    pub pty: Pty,
    slave_fd: Option<i32>,
}

impl ChildProcess {
    /// Spawn a child process with a PTY to execute the given command
    ///
    /// # Arguments
    /// * `command` - Command and arguments to execute
    /// * `verbose` - Enable verbose logging
    ///
    /// # Returns
    /// A ChildProcess handle on success
    pub fn spawn(command: &[String], verbose: bool) -> Result<Self> {
        if command.is_empty() {
            return Err(SshpassError::InvalidArguments(
                "No command specified".to_string(),
            ));
        }

        // Create PTY before forking
        let pty = Pty::new()?;

        if verbose {
            eprintln!("SSHPASS: Created PTY with slave: {}", pty.slave_name());
        }

        // Set up signal mask before fork
        let mut sigset = SigSet::empty();
        sigset.add(nix::sys::signal::SIGCHLD);
        sigset.add(nix::sys::signal::SIGHUP);
        sigset.add(nix::sys::signal::SIGTERM);
        sigset.add(nix::sys::signal::SIGINT);
        sigset.add(nix::sys::signal::SIGTSTP);

        // Block signals during fork
        sigprocmask(SigmaskHow::SIG_SETMASK, Some(&sigset), None)
            .map_err(|e| SshpassError::SystemError(e))?;

        // Fork the process
        match unsafe { fork() } {
            Ok(ForkResult::Parent { child }) => {
                // Parent process
                if verbose {
                    eprintln!("SSHPASS: Forked child process with PID: {}", child);
                }

                // Open the slave PTY to keep it alive (see C version comment 3.14159)
                let slave_fd = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(pty.slave_name())
                    .map(|f| f.as_raw_fd())
                    .ok();

                // Restore empty signal mask for pselect
                let empty_sigset = SigSet::empty();
                sigprocmask(SigmaskHow::SIG_SETMASK, Some(&empty_sigset), None)
                    .map_err(|e| SshpassError::SystemError(e))?;

                Ok(ChildProcess {
                    pid: child,
                    pty,
                    slave_fd,
                })
            }
            Ok(ForkResult::Child) => {
                // Child process
                if let Err(e) = run_child(&pty, command, verbose) {
                    eprintln!("SSHPASS: Failed to run command: {}", e);
                    std::process::exit(3); // RETURN_RUNTIME_ERROR
                }
                unreachable!();
            }
            Err(e) => Err(SshpassError::ForkError(format!("Fork failed: {}", e))),
        }
    }

    /// Wait for the child process without blocking
    ///
    /// Returns Some(exit_code) if the process has exited, None if still running
    pub fn try_wait(&self) -> Result<Option<i32>> {
        match waitpid(self.pid, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::Exited(_, code)) => Ok(Some(code)),
            Ok(WaitStatus::Signaled(_, sig, _)) => Ok(Some(128 + sig as i32)),
            Ok(WaitStatus::StillAlive) => Ok(None),
            Ok(_) => Ok(None), // Other statuses, continue waiting
            Err(e) => Err(SshpassError::SystemError(e)),
        }
    }

    /// Wait for the child process to exit (blocking)
    pub fn wait(&self) -> Result<i32> {
        match waitpid(self.pid, None) {
            Ok(WaitStatus::Exited(_, code)) => Ok(code),
            Ok(WaitStatus::Signaled(_, sig, _)) => Ok(128 + sig as i32),
            Ok(_) => Ok(255), // Unknown status
            Err(e) => Err(SshpassError::SystemError(e)),
        }
    }

    /// Send a signal to the child process
    pub fn kill(&self, signal: nix::sys::signal::Signal) -> Result<()> {
        nix::sys::signal::kill(self.pid, signal)
            .map_err(|e| SshpassError::SystemError(e))
    }
}

impl Drop for ChildProcess {
    fn drop(&mut self) {
        // Close slave fd if we opened it
        if let Some(fd) = self.slave_fd {
            let _ = close(fd);
        }
    }
}

/// Run the command in the child process
///
/// This function sets up the child's environment and executes the command.
/// It does not return on success (execvp replaces the process).
fn run_child(pty: &Pty, command: &[String], verbose: bool) -> Result<()> {
    // Restore signal mask (unblock all signals)
    let empty_sigset = SigSet::empty();
    sigprocmask(SigmaskHow::SIG_SETMASK, Some(&empty_sigset), None)
        .map_err(|e| SshpassError::SystemError(e))?;

    // Create a new session (detach from current TTY)
    setsid().map_err(|e| {
        SshpassError::RuntimeError(format!("Failed to create new session: {}", e))
    })?;

    // Open the slave PTY
    let slave = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(OFlag::O_NOCTTY.bits())
        .open(pty.slave_name())
        .map_err(|e| {
            SshpassError::RuntimeError(format!("Failed to open slave PTY: {}", e))
        })?;

    let slave_fd = slave.as_raw_fd();

    // Set the slave as the controlling terminal
    use nix::ioctl_write_int_bad;
    ioctl_write_int_bad!(tiocsctty, libc::TIOCSCTTY);

    unsafe {
        tiocsctty(slave_fd, 0).map_err(|e| {
            SshpassError::RuntimeError(format!("Failed to set controlling terminal: {}", e))
        })?;
    }

    // Close the slave fd (we don't need it open, it's now our controlling TTY)
    drop(slave);

    if verbose {
        eprintln!("SSHPASS: Child process set up PTY, executing: {:?}", command);
    }

    // Prepare arguments for execvp
    let c_strings: Result<Vec<CString>> = command
        .iter()
        .map(|s| {
            CString::new(s.as_str()).map_err(|e| {
                SshpassError::InvalidArguments(format!("Invalid argument '{}': {}", s, e))
            })
        })
        .collect();

    let c_strings = c_strings?;

    // Execute the command (this replaces the current process)
    execvp(&c_strings[0], &c_strings).map_err(|e| {
        SshpassError::ExecError(format!("Failed to execute command: {}", e))
    })?;

    // If execvp returns, it's an error
    unreachable!("execvp should not return on success");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_child_process_spawn() {
        // Simple test that spawns echo
        let command = vec!["echo".to_string(), "test".to_string()];
        let result = ChildProcess::spawn(&command, false);

        if let Ok(mut child) = result {
            // Wait a bit for the process to complete
            std::thread::sleep(std::time::Duration::from_millis(100));

            match child.try_wait() {
                Ok(Some(code)) => {
                    assert_eq!(code, 0, "Echo should exit with code 0");
                }
                Ok(None) => {
                    // Process still running, wait for it
                    let code = child.wait().unwrap();
                    assert_eq!(code, 0);
                }
                Err(e) => panic!("Failed to wait for child: {}", e),
            }
        }
    }
}
