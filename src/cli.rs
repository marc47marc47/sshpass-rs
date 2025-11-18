use clap::Parser;
use std::path::PathBuf;

/// sshpass - noninteractive ssh password provider
///
/// This is a Rust implementation of sshpass, a utility designed for running ssh
/// using the mode referred to as "keyboard-interactive" password authentication,
/// but in non-interactive mode.
#[derive(Parser, Debug)]
#[command(
    name = "sshpass",
    version = "0.1.0",
    about = "Noninteractive ssh password provider",
    long_about = "A Rust implementation of sshpass for automated SSH password authentication.\n\
                  SSH uses direct TTY access to ensure passwords are issued by interactive users.\n\
                  sshpass runs ssh in a dedicated PTY, allowing automated password entry."
)]
pub struct Cli {
    /// Take password from file
    #[arg(short = 'f', long = "file", value_name = "filename", group = "password_source")]
    pub password_file: Option<PathBuf>,

    /// Use number as file descriptor for getting password
    #[arg(short = 'd', long = "fd", value_name = "number", group = "password_source")]
    pub password_fd: Option<i32>,

    /// Provide password as argument (security unwise)
    #[arg(short = 'p', long = "password", value_name = "password", group = "password_source")]
    pub password: Option<String>,

    /// Password is passed as env-var "SSHPASS" or specified variable
    #[arg(
        short = 'e',
        long = "env",
        value_name = "env_var",
        group = "password_source",
        num_args = 0..=1,
        default_missing_value = "SSHPASS",
        require_equals = true
    )]
    pub env_var: Option<String>,

    /// Which string should sshpass search for to detect a password prompt
    #[arg(short = 'P', long = "prompt", value_name = "prompt")]
    pub prompt: Option<String>,

    /// Be verbose about what you're doing
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Command and its arguments to execute
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    pub command: Vec<String>,
}

impl Cli {
    /// Parse command line arguments
    pub fn parse_args() -> Self {
        Cli::parse()
    }

    /// Validate the parsed arguments
    pub fn validate(&self) -> Result<(), crate::error::SshpassError> {
        use crate::error::SshpassError;

        // Ensure at least one command is provided
        if self.command.is_empty() {
            return Err(SshpassError::InvalidArguments(
                "No command specified".to_string(),
            ));
        }

        // Validate file descriptor if provided
        if let Some(fd) = self.password_fd {
            if fd < 0 {
                return Err(SshpassError::InvalidFileDescriptor(fd));
            }
        }

        // Validate file exists if provided
        if let Some(ref path) = self.password_file {
            if !path.exists() {
                return Err(SshpassError::PasswordFileError(format!(
                    "File does not exist: {}",
                    path.display()
                )));
            }
        }

        // Validate environment variable exists if specified
        if let Some(ref env_var) = self.env_var {
            if std::env::var(env_var).is_err() {
                return Err(SshpassError::EnvVarNotSet(env_var.clone()));
            }
        }

        Ok(())
    }

    /// Check if verbose mode is enabled
    pub fn is_verbose(&self) -> bool {
        self.verbose > 0
    }

    /// Get the verbosity level
    pub fn verbosity_level(&self) -> u8 {
        self.verbose
    }

    /// Get the password prompt to use (default: "assword")
    pub fn get_prompt(&self) -> &str {
        self.prompt.as_deref().unwrap_or("assword")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing() {
        // This would require setting up clap test environment
        // For now, we validate the structure compiles correctly
    }
}
