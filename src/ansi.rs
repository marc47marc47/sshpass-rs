//! ANSI/VT100 控制碼處理
//!
//! 透過持續化的 VTE parser 來濾除控制碼，並正規化行結尾。

use vte::{Parser, Perform};

/// 執行 VTE 回呼的實作者
struct AnsiPerformer {
    output: Vec<u8>,
}

impl AnsiPerformer {
    fn new() -> Self {
        Self { output: Vec::new() }
    }

    fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }
}

impl Perform for AnsiPerformer {
    fn print(&mut self, c: char) {
        let mut buf = [0u8; 4];
        let s = c.encode_utf8(&mut buf);
        self.output.extend_from_slice(s.as_bytes());
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' | b'\r' | b'\t' | b'\x08' => self.output.push(byte),
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        _params: &vte::Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: char,
    ) {
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
    fn hook(&mut self, _params: &vte::Params, _intermediates: &[u8], _ignore: bool, _action: char) {
    }
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
}

/// ANSI 控制碼濾波器，保留 parser 狀態以跨呼叫處理片段。
pub struct AnsiFilter {
    parser: Parser,
    performer: AnsiPerformer,
}

impl AnsiFilter {
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
            performer: AnsiPerformer::new(),
        }
    }

    /// 濾除控制碼，並回傳正規化換行後的結果
    pub fn process(&mut self, input: &[u8]) -> Vec<u8> {
        for &byte in input {
            self.parser.advance(&mut self.performer, byte);
        }

        let filtered = self.performer.take_output();
        normalize_line_endings(&filtered)
    }
}

impl Default for AnsiFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// 將 CRLF/CR 轉成 LF
pub fn normalize_line_endings(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut i = 0;

    while i < input.len() {
        if input[i] == b'\r' {
            if i + 1 < input.len() && input[i + 1] == b'\n' {
                output.push(b'\n');
                i += 2;
            } else {
                output.push(b'\n');
                i += 1;
            }
        } else {
            output.push(input[i]);
            i += 1;
        }
    }

    output
}

/// 舊的便利函式，供測試與少數呼叫使用
#[allow(dead_code)]
pub fn process_output(input: &[u8]) -> Vec<u8> {
    let mut filter = AnsiFilter::new();
    filter.process(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ansi_filter_plain_text() {
        let mut filter = AnsiFilter::new();
        let input = b"Hello, World!";
        let output = filter.process(input);
        assert_eq!(output, input);
    }

    #[test]
    fn test_ansi_filter_color_codes() {
        let mut filter = AnsiFilter::new();
        let input = b"\x1b[31mHello\x1b[0m";
        let output = filter.process(input);
        assert_eq!(output, b"Hello");
    }

    #[test]
    fn test_ansi_filter_cursor_movement() {
        let mut filter = AnsiFilter::new();
        let input = b"Hel\x1b[2Clo";
        let output = filter.process(input);
        assert_eq!(output, b"Hello");
    }

    #[test]
    fn test_ansi_filter_split_escape_sequence() {
        let mut filter = AnsiFilter::new();
        let part1 = b"\x1b[61;4;";
        let part2 = b"6c";
        filter.process(part1);
        let output = filter.process(part2);
        assert!(output.is_empty());
    }

    #[test]
    fn test_normalize_line_endings_crlf() {
        let input = b"Line1\r\nLine2\r\nLine3";
        let output = normalize_line_endings(input);
        assert_eq!(output, b"Line1\nLine2\nLine3");
    }

    #[test]
    fn test_normalize_line_endings_cr() {
        let input = b"Line1\rLine2\rLine3";
        let output = normalize_line_endings(input);
        assert_eq!(output, b"Line1\nLine2\nLine3");
    }

    #[test]
    fn test_normalize_line_endings_mixed() {
        let input = b"Line1\r\nLine2\rLine3\nLine4";
        let output = normalize_line_endings(input);
        assert_eq!(output, b"Line1\nLine2\nLine3\nLine4");
    }

    #[test]
    fn test_process_output_combined() {
        let input = b"\x1b[31mPassword:\x1b[0m\r\n";
        let output = process_output(input);
        assert_eq!(output, b"Password:\n");
    }

    #[test]
    fn test_password_prompt_with_ansi() {
        let input = b"\x1b[1mroot@192.168.1.1's password:\x1b[0m ";
        let output = process_output(input);
        assert!(output.windows(8).any(|w| w == b"password"));
    }
}
