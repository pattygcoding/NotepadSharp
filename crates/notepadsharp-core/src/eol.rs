//! Line ending detection, normalization and re-application.
//!
//! Documents are held in memory with `\n` only; the file's original line ending is
//! remembered and applied again on save, and can be converted from the View menu.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Eol {
    Lf,
    CrLf,
    Cr,
}

impl Default for Eol {
    fn default() -> Self {
        if cfg!(windows) {
            Eol::CrLf
        } else {
            Eol::Lf
        }
    }
}

impl Eol {
    pub const ALL: [Eol; 3] = [Eol::Lf, Eol::CrLf, Eol::Cr];

    /// The literal bytes this line ending writes.
    pub fn as_str(self) -> &'static str {
        match self {
            Eol::Lf => "\n",
            Eol::CrLf => "\r\n",
            Eol::Cr => "\r",
        }
    }

    /// Short label used in the status bar.
    pub fn label(self) -> &'static str {
        match self {
            Eol::Lf => "LF",
            Eol::CrLf => "CRLF",
            Eol::Cr => "CR",
        }
    }

    /// Long name used in menus.
    pub fn name(self) -> &'static str {
        match self {
            Eol::Lf => "Unix (LF)",
            Eol::CrLf => "Windows (CRLF)",
            Eol::Cr => "Macintosh (CR)",
        }
    }

    /// Detects the dominant line ending of `text`; defaults to platform style when mixed or empty.
    pub fn detect(text: &str) -> Eol {
        let (crlf, lf, cr) = Eol::counts(text);
        if crlf == 0 && lf == 0 && cr == 0 {
            return Eol::default();
        }
        if crlf >= lf && crlf >= cr {
            Eol::CrLf
        } else if lf >= cr {
            Eol::Lf
        } else {
            Eol::Cr
        }
    }

    /// Returns `(crlf, lone_lf, lone_cr)` counts.
    pub fn counts(text: &str) -> (usize, usize, usize) {
        let bytes = text.as_bytes();
        let mut crlf = 0;
        let mut lf = 0;
        let mut cr = 0;
        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b'\r' => {
                    if bytes.get(index + 1) == Some(&b'\n') {
                        crlf += 1;
                        index += 1;
                    } else {
                        cr += 1;
                    }
                }
                b'\n' => lf += 1,
                _ => {}
            }
            index += 1;
        }
        (crlf, lf, cr)
    }

    /// True when the text mixes line ending styles.
    pub fn is_mixed(text: &str) -> bool {
        let (crlf, lf, cr) = Eol::counts(text);
        let kinds = crlf.min(1) + lf.min(1) + cr.min(1);
        kinds > 1
    }
}

/// Converts every line ending in `text` to `\n`.
pub fn normalize_to_lf(text: &str) -> String {
    if !text.as_bytes().contains(&b'\r') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(ch);
        }
    }
    out
}

/// Applies `eol` to text that is already normalized to `\n`.
pub fn apply_eol(text: &str, eol: Eol) -> String {
    if eol == Eol::Lf || !text.contains('\n') {
        return text.to_owned();
    }
    text.replace('\n', eol.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_styles() {
        assert_eq!(Eol::detect("a\nb\n"), Eol::Lf);
        assert_eq!(Eol::detect("a\r\nb\r\n"), Eol::CrLf);
        assert_eq!(Eol::detect("a\rb\r"), Eol::Cr);
        assert_eq!(Eol::detect("single line"), Eol::default());
        assert_eq!(Eol::detect("a\r\nb\r\nc\n"), Eol::CrLf, "majority wins");
    }

    #[test]
    fn counts_are_exact() {
        assert_eq!(Eol::counts("a\r\nb\nc\r"), (1, 1, 1));
        assert_eq!(Eol::counts(""), (0, 0, 0));
        assert!(Eol::is_mixed("a\r\nb\n"));
        assert!(!Eol::is_mixed("a\r\nb\r\n"));
    }

    #[test]
    fn normalize_handles_all_styles() {
        assert_eq!(normalize_to_lf("a\r\nb\rc\nd"), "a\nb\nc\nd");
        assert_eq!(normalize_to_lf("plain"), "plain");
        assert_eq!(normalize_to_lf("\r\n\r\n"), "\n\n");
    }

    #[test]
    fn apply_round_trips_per_style() {
        for eol in Eol::ALL {
            let normalized = "one\ntwo\nthree";
            let encoded = apply_eol(normalized, eol);
            assert_eq!(normalize_to_lf(&encoded), normalized, "{eol:?}");
            assert_eq!(Eol::detect(&encoded), eol);
        }
    }
}
