use crate::error::{Result, SshpassError};
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt, PtyMaster, Winsize};
use nix::unistd::write;
use std::os::unix::io::{AsRawFd, RawFd};

/// Wrapper around PTY master file descriptor with RAII cleanup
pub struct Pty {
    master: PtyMaster,
    slave_name: String,
}

impl Pty {
    /// Create a new PTY pair (master and slave)
    pub fn new() -> Result<Self> {
        // Open the master PTY
        let master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY).map_err(|e| {
            SshpassError::PtyCreationError(format!("Failed to open PTY master: {}", e))
        })?;

        // Grant access to the slave PTY
        grantpt(&master).map_err(|e| {
            SshpassError::PtyCreationError(format!("Failed to grant PTY permissions: {}", e))
        })?;

        // Unlock the slave PTY
        unlockpt(&master).map_err(|e| {
            SshpassError::PtyCreationError(format!("Failed to unlock PTY: {}", e))
        })?;

        // Get the slave PTY name
        let slave_name = unsafe { ptsname(&master) }.map_err(|e| {
            SshpassError::PtyCreationError(format!("Failed to get PTY slave name: {}", e))
        })?;

        // Set master to non-blocking mode
        fcntl(master.as_raw_fd(), FcntlArg::F_SETFL(OFlag::O_NONBLOCK)).map_err(|e| {
            SshpassError::PtyCreationError(format!("Failed to set PTY to non-blocking: {}", e))
        })?;

        Ok(Self { master, slave_name })
    }

    /// Get the raw file descriptor of the master PTY
    pub fn master_fd(&self) -> RawFd {
        self.master.as_raw_fd()
    }

    /// Get the path to the slave PTY
    pub fn slave_name(&self) -> &str {
        &self.slave_name
    }

    /// Set the window size of the PTY
    pub fn set_winsize(&self, winsize: &Winsize) -> Result<()> {
        use nix::ioctl_write_ptr_bad;

        ioctl_write_ptr_bad!(tiocswinsz, libc::TIOCSWINSZ, Winsize);

        unsafe {
            tiocswinsz(self.master_fd(), winsize as *const Winsize).map_err(|e| {
                SshpassError::RuntimeError(format!("Failed to set window size: {}", e))
            })?;
        }

        Ok(())
    }

    /// Read data from the master PTY
    pub fn read(&self, buffer: &mut [u8]) -> Result<usize> {
        use nix::unistd::read;

        match read(self.master_fd(), buffer) {
            Ok(n) => Ok(n),
            Err(nix::errno::Errno::EAGAIN) | Err(nix::errno::Errno::EWOULDBLOCK) => Ok(0),
            Err(e) => Err(SshpassError::SystemError(e)),
        }
    }

    /// Write data to the master PTY (with reliability guarantee)
    pub fn write_all(&self, data: &[u8]) -> Result<()> {
        reliable_write(self.master_fd(), data)
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        // The PtyMaster will automatically close the fd when dropped
    }
}

/// Get the current window size of the terminal
pub fn get_terminal_winsize() -> Option<Winsize> {
    use nix::ioctl_read_bad;
    use std::fs::OpenOptions;

    ioctl_read_bad!(tiocgwinsz, libc::TIOCGWINSZ, Winsize);

    // Try to open /dev/tty
    let tty = OpenOptions::new()
        .read(true)
        .write(false)
        .open("/dev/tty")
        .ok()?;

    let mut winsize = Winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    unsafe {
        tiocgwinsz(tty.as_raw_fd(), &mut winsize as *mut Winsize).ok()?;
    }

    Some(winsize)
}

/// Reliably write all data to a file descriptor
///
/// This function ensures that all data is written, handling partial writes
/// and EINTR errors. This matches the behavior of the C version's reliable_write().
pub fn reliable_write(fd: RawFd, data: &[u8]) -> Result<()> {
    let mut written = 0;

    while written < data.len() {
        match write(fd, &data[written..]) {
            Ok(n) => {
                if n == 0 {
                    return Err(SshpassError::RuntimeError(
                        "Write returned 0 bytes".to_string(),
                    ));
                }
                written += n;
            }
            Err(nix::errno::Errno::EINTR) => {
                // Interrupted by signal, retry
                continue;
            }
            Err(e) => {
                return Err(SshpassError::RuntimeError(format!(
                    "Write failed after {} bytes: {}",
                    written, e
                )));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pty_creation() {
        let pty = Pty::new();
        assert!(pty.is_ok());

        if let Ok(pty) = pty {
            assert!(pty.master_fd() > 0);
            assert!(!pty.slave_name().is_empty());
            assert!(pty.slave_name().starts_with("/dev/"));
        }
    }

    #[test]
    fn test_reliable_write() {
        // This test requires a valid file descriptor
        // For now, we just ensure the function compiles
    }
}
