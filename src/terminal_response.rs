/// Handle terminal query sequences and provide appropriate responses
///
/// SSH (and other programs) may send ANSI escape sequences to query
/// the terminal capabilities. We need to respond to these queries
/// to prevent the program from hanging.

/// Check if data contains a terminal query and return appropriate response
pub fn get_terminal_response(data: &[u8]) -> Option<Vec<u8>> {
    let s = String::from_utf8_lossy(data);

    // Device Attributes query: ESC [ c
    // Response: ESC [ ? 1 ; 2 c (VT100 with Advanced Video Option)
    if s.contains("\x1b[c") {
        eprintln!("SSHPASS: [TERMINAL] Responding to Device Attributes query (ESC[c)");
        return Some(b"\x1b[?1;2c".to_vec());
    }

    // Cursor Position Report query: ESC [ 6 n
    // Response: ESC [ row ; col R
    if s.contains("\x1b[6n") {
        eprintln!("SSHPASS: [TERMINAL] Responding to Cursor Position query (ESC[6n)");
        return Some(b"\x1b[1;1R".to_vec());
    }

    // For mouse tracking and focus events, we just acknowledge without response
    // These don't require responses:
    // - ESC [ ? 1004 h - Enable focus events
    // - ESC [ ? 9001 h - ?
    // - ESC [ 1 t - Window manipulation

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_attributes() {
        let query = b"\x1b[c";
        let response = get_terminal_response(query);
        assert!(response.is_some());
        assert_eq!(response.unwrap(), b"\x1b[?1;2c");
    }

    #[test]
    fn test_cursor_position() {
        let query = b"\x1b[6n";
        let response = get_terminal_response(query);
        assert!(response.is_some());
        assert_eq!(response.unwrap(), b"\x1b[1;1R");
    }

    #[test]
    fn test_no_response_needed() {
        let query = b"\x1b[?1004h";
        let response = get_terminal_response(query);
        assert!(response.is_none());
    }
}
