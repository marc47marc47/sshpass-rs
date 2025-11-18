/// State machine for matching strings in output
///
/// This implements a simple string matching algorithm that can handle
/// patterns split across multiple buffers. It matches the behavior of
/// the C version's match() function.
#[derive(Debug, Clone)]
pub struct Matcher {
    reference: String,
    state: usize,
}

impl Matcher {
    /// Create a new matcher for the given reference string
    pub fn new(reference: impl Into<String>) -> Self {
        Self {
            reference: reference.into(),
            state: 0,
        }
    }

    /// Feed data to the matcher and check if pattern is found
    ///
    /// Returns true if the complete pattern has been matched.
    /// The matcher maintains state across multiple calls.
    pub fn feed(&mut self, buffer: &[u8]) -> bool {
        let reference_bytes = self.reference.as_bytes();

        for &byte in buffer {
            if self.state < reference_bytes.len() && reference_bytes[self.state] == byte {
                self.state += 1;
                if self.state == reference_bytes.len() {
                    return true;
                }
            } else {
                // No match, reset and try again from the beginning
                self.state = 0;
                if self.state < reference_bytes.len() && reference_bytes[self.state] == byte {
                    self.state += 1;
                }
            }
        }

        false
    }

    /// Reset the matcher state
    pub fn reset(&mut self) {
        self.state = 0;
    }

    /// Check if the matcher has completed matching
    pub fn is_complete(&self) -> bool {
        self.state == self.reference.len()
    }
}

/// Result of monitoring output from SSH
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorResult {
    /// Continue monitoring
    Continue,
    /// Password prompt detected, need to send password
    SendPassword,
    /// Password prompt detected again (incorrect password)
    IncorrectPassword,
    /// Host key unknown prompt detected
    HostKeyUnknown,
    /// Host key changed prompt detected
    HostKeyChanged,
}

/// Monitors SSH output for password prompts and error conditions
pub struct OutputMonitor {
    password_matcher: Matcher,
    host_auth_matcher: Matcher,
    host_key_changed_matcher: Matcher,
    password_sent: bool,
    verbose: bool,
    first_output: bool,
}

impl OutputMonitor {
    /// Create a new output monitor
    ///
    /// # Arguments
    /// * `prompt` - Optional custom password prompt (default: "assword")
    /// * `verbose` - Enable verbose logging
    pub fn new(prompt: Option<&str>, verbose: bool) -> Self {
        let password_prompt = prompt.unwrap_or("assword");

        if verbose {
            eprintln!(
                "SSHPASS: searching for password prompt using match \"{}\"",
                password_prompt
            );
        }

        Self {
            password_matcher: Matcher::new(password_prompt),
            host_auth_matcher: Matcher::new("The authenticity of host "),
            host_key_changed_matcher: Matcher::new("differs from the key for the IP address"),
            password_sent: false,
            verbose,
            first_output: true,
        }
    }

    /// Handle output from SSH and determine what action to take
    ///
    /// # Arguments
    /// * `data` - Buffer containing output from SSH
    ///
    /// # Returns
    /// MonitorResult indicating what action should be taken
    pub fn handle_output(&mut self, data: &[u8]) -> MonitorResult {
        if self.verbose {
            if self.first_output {
                self.first_output = false;
            }
            // Print the data for debugging
            if let Ok(s) = std::str::from_utf8(data) {
                eprint!("SSHPASS: read: {}", s);
            }
        }

        // Check for password prompt
        if self.password_matcher.feed(data) {
            if !self.password_sent {
                if self.verbose {
                    eprintln!("SSHPASS: detected prompt. Sending password.");
                }
                self.password_sent = true;
                self.password_matcher.reset();
                return MonitorResult::SendPassword;
            } else {
                // Password prompt appeared again - wrong password
                if self.verbose {
                    eprintln!("SSHPASS: detected prompt, again. Wrong password. Terminating.");
                }
                return MonitorResult::IncorrectPassword;
            }
        }

        // Check for host authentication prompt
        if self.host_auth_matcher.feed(data) {
            if self.verbose {
                eprintln!("SSHPASS: detected host authentication prompt. Exiting.");
            }
            return MonitorResult::HostKeyUnknown;
        }

        // Check for host key changed prompt
        if self.host_key_changed_matcher.feed(data) {
            if self.verbose {
                eprintln!("SSHPASS: detected host key changed prompt. Exiting.");
            }
            return MonitorResult::HostKeyChanged;
        }

        MonitorResult::Continue
    }

    /// Check if password has been sent
    pub fn password_sent(&self) -> bool {
        self.password_sent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matcher_simple() {
        let mut matcher = Matcher::new("Password:");
        assert!(!matcher.feed(b"Enter "));
        assert!(!matcher.feed(b"your "));
        assert!(matcher.feed(b"Password:"));
    }

    #[test]
    fn test_matcher_split() {
        let mut matcher = Matcher::new("Password:");
        assert!(!matcher.feed(b"Pass"));
        assert!(matcher.feed(b"word:"));
    }

    #[test]
    fn test_matcher_reset_on_mismatch() {
        let mut matcher = Matcher::new("assword");
        assert!(matcher.feed(b"Password"));
        matcher.reset();
        assert!(!matcher.feed(b"Pass"));
        assert!(matcher.feed(b"word"));
    }

    #[test]
    fn test_matcher_partial_match() {
        let mut matcher = Matcher::new("password");
        assert!(!matcher.feed(b"pass"));
        assert_eq!(matcher.state, 4);
        matcher.reset();
        assert_eq!(matcher.state, 0);
    }

    #[test]
    fn test_output_monitor_password_prompt() {
        let mut monitor = OutputMonitor::new(Some("assword"), false);

        let result = monitor.handle_output(b"Enter Password: ");
        assert_eq!(result, MonitorResult::SendPassword);

        // Second prompt should indicate wrong password
        let result = monitor.handle_output(b"Password: ");
        assert_eq!(result, MonitorResult::IncorrectPassword);
    }

    #[test]
    fn test_output_monitor_host_auth() {
        let mut monitor = OutputMonitor::new(None, false);

        let result = monitor.handle_output(b"The authenticity of host 'example.com' can't be established.");
        assert_eq!(result, MonitorResult::HostKeyUnknown);
    }

    #[test]
    fn test_output_monitor_host_key_changed() {
        let mut monitor = OutputMonitor::new(None, false);

        let result = monitor.handle_output(b"WARNING: The key differs from the key for the IP address");
        assert_eq!(result, MonitorResult::HostKeyChanged);
    }
}
