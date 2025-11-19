use thiserror::Error;

/// Return codes matching the original C version of sshpass
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnCode {
    NoError = 0,
    InvalidArguments = 1,
    ConflictingArguments = 2,
    RuntimeError = 3,
    ParseError = 4,
    IncorrectPassword = 5,
    HostKeyUnknown = 6,
    HostKeyChanged = 7,
}

impl ReturnCode {
    pub fn as_exit_code(self) -> i32 {
        self as i32
    }
}

/// Main error type for sshpass operations
#[derive(Error, Debug)]
pub enum SshpassError {
    #[error("Invalid command line arguments: {0}")]
    InvalidArguments(String),

    #[error("Conflicting password source arguments provided")]
    ConflictingArguments,

    #[error("Runtime error: {0}")]
    RuntimeError(String),

    #[error("Failed to parse output: {0}")]
    ParseError(String),

    #[error("Incorrect password provided")]
    IncorrectPassword,

    #[error("Host public key is unknown")]
    HostKeyUnknown,

    #[error("Host public key has changed")]
    HostKeyChanged,

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[cfg(unix)]
    #[error("System error: {0}")]
    SystemError(#[from] nix::Error),

    #[cfg(windows)]
    #[error("Windows error: {0}")]
    WindowsError(String),

    #[error("Environment variable not set: {0}")]
    EnvVarNotSet(String),

    #[error("Failed to open password file: {0}")]
    PasswordFileError(String),

    #[error("Invalid file descriptor: {0}")]
    InvalidFileDescriptor(i32),

    #[error("Failed to create PTY: {0}")]
    PtyCreationError(String),

    #[error("Failed to fork process: {0}")]
    ForkError(String),

    #[error("Failed to execute command: {0}")]
    ExecError(String),
}

impl SshpassError {
    /// Convert error to the appropriate return code
    pub fn to_return_code(&self) -> ReturnCode {
        match self {
            SshpassError::InvalidArguments(_) => ReturnCode::InvalidArguments,
            SshpassError::ConflictingArguments => ReturnCode::ConflictingArguments,
            SshpassError::RuntimeError(_) => ReturnCode::RuntimeError,
            SshpassError::ParseError(_) => ReturnCode::ParseError,
            SshpassError::IncorrectPassword => ReturnCode::IncorrectPassword,
            SshpassError::HostKeyUnknown => ReturnCode::HostKeyUnknown,
            SshpassError::HostKeyChanged => ReturnCode::HostKeyChanged,
            SshpassError::IoError(_) => ReturnCode::RuntimeError,
            #[cfg(unix)]
            SshpassError::SystemError(_) => ReturnCode::RuntimeError,
            #[cfg(windows)]
            SshpassError::WindowsError(_) => ReturnCode::RuntimeError,
            SshpassError::EnvVarNotSet(_) => ReturnCode::InvalidArguments,
            SshpassError::PasswordFileError(_) => ReturnCode::RuntimeError,
            SshpassError::InvalidFileDescriptor(_) => ReturnCode::InvalidArguments,
            SshpassError::PtyCreationError(_) => ReturnCode::RuntimeError,
            SshpassError::ForkError(_) => ReturnCode::RuntimeError,
            SshpassError::ExecError(_) => ReturnCode::RuntimeError,
        }
    }

    /// Get the exit code for this error
    pub fn exit_code(&self) -> i32 {
        self.to_return_code().as_exit_code()
    }
}

/// Result type alias for sshpass operations
pub type Result<T> = std::result::Result<T, SshpassError>;

impl From<SshpassError> for i32 {
    fn from(error: SshpassError) -> Self {
        error.exit_code()
    }
}
