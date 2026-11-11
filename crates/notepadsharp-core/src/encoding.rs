//! Text encodings: BOM sniffing, decoding, and re-encoding on save.
//!
//! Notepad++'s "Encoding" menu is one of its most useful features, so files keep the
//! encoding they were opened with and can be converted explicitly.

use encoding_rs::WINDOWS_1252;
use serde::{Deserialize, Serialize};

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Encoding {
    #[default]
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Windows1252,
}

impl Encoding {
    pub const ALL: [Encoding; 5] = [
        Encoding::Utf8,
        Encoding::Utf8Bom,
        Encoding::Utf16Le,
        Encoding::Utf16Be,
        Encoding::Windows1252,
    ];

    /// Status-bar label.
    pub fn label(self) -> &'static str {
        match self {
            Encoding::Utf8 => "UTF-8",
            Encoding::Utf8Bom => "UTF-8 BOM",
            Encoding::Utf16Le => "UTF-16 LE",
            Encoding::Utf16Be => "UTF-16 BE",
            Encoding::Windows1252 => "ANSI",
        }
    }

    pub fn has_bom(self) -> bool {
        matches!(
            self,
            Encoding::Utf8Bom | Encoding::Utf16Le | Encoding::Utf16Be
        )
    }

    /// Decodes `bytes`, choosing an encoding from the BOM or by validating UTF-8.
    ///
    /// Invalid UTF-8 without a BOM falls back to Windows-1252 ("ANSI" in Notepad++)
    /// with replacement characters for bytes that cannot be mapped.
    pub fn decode(bytes: &[u8]) -> (String, Encoding) {
        if let Some(rest) = bytes.strip_prefix(UTF8_BOM) {
            return (
                String::from_utf8_lossy(rest).into_owned(),
                Encoding::Utf8Bom,
            );
        }
        if bytes.starts_with(&[0xFF, 0xFE]) {
            let (text, _, _) = encoding_rs::UTF_16LE.decode(&bytes[2..]);
            return (text.into_owned(), Encoding::Utf16Le);
        }
        if bytes.starts_with(&[0xFE, 0xFF]) {
            let (text, _, _) = encoding_rs::UTF_16BE.decode(&bytes[2..]);
            return (text.into_owned(), Encoding::Utf16Be);
        }
        match std::str::from_utf8(bytes) {
            Ok(text) => (text.to_owned(), Encoding::Utf8),
            Err(_) => {
                let (text, _, _) = WINDOWS_1252.decode(bytes);
                (text.into_owned(), Encoding::Windows1252)
            }
        }
    }

    /// Encodes `text` in this encoding, including a BOM where this encoding uses one.
    pub fn encode(self, text: &str) -> Vec<u8> {
        match self {
            Encoding::Utf8 => text.as_bytes().to_vec(),
            Encoding::Utf8Bom => {
                let mut out = Vec::with_capacity(text.len() + 3);
                out.extend_from_slice(UTF8_BOM);
                out.extend_from_slice(text.as_bytes());
                out
            }
            Encoding::Utf16Le => Self::encode_utf16(text, true),
            Encoding::Utf16Be => Self::encode_utf16(text, false),
            Encoding::Windows1252 => {
                let (bytes, _, _) = WINDOWS_1252.encode(text);
                bytes.into_owned()
            }
        }
    }

    fn encode_utf16(text: &str, little_endian: bool) -> Vec<u8> {
        let mut out = Vec::with_capacity(text.len() * 2 + 2);
        out.extend_from_slice(if little_endian {
            &[0xFF, 0xFE]
        } else {
            &[0xFE, 0xFF]
        });
        for unit in text.encode_utf16() {
            let bytes = if little_endian {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            };
            out.extend_from_slice(&bytes);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "héllo wörld ♫\nsecond line\n";

    #[test]
    fn utf8_round_trip() {
        for encoding in [
            Encoding::Utf8,
            Encoding::Utf8Bom,
            Encoding::Utf16Le,
            Encoding::Utf16Be,
        ] {
            let bytes = encoding.encode(SAMPLE);
            let (text, detected) = Encoding::decode(&bytes);
            assert_eq!(text, SAMPLE, "{encoding:?}");
            assert_eq!(detected, encoding, "{encoding:?}");
        }
    }

    #[test]
    fn plain_utf8_has_no_bom_and_is_preferred() {
        let bytes = Encoding::Utf8.encode("alpha");
        assert_eq!(bytes, b"alpha");
        assert_eq!(
            Encoding::decode(&bytes),
            ("alpha".to_owned(), Encoding::Utf8)
        );
    }

    #[test]
    fn bom_is_stripped_from_text() {
        let bytes = Encoding::Utf8Bom.encode("alpha");
        let (text, encoding) = Encoding::decode(&bytes);
        assert_eq!(text, "alpha");
        assert_eq!(encoding, Encoding::Utf8Bom);
        assert_eq!(bytes.len(), 8);
    }

    #[test]
    fn ansi_fallback_handles_invalid_utf8() {
        let bytes = [0x63, 0x61, 0x66, 0xE9]; // "café" in Windows-1252
        let (text, encoding) = Encoding::decode(&bytes);
        assert_eq!(text, "café");
        assert_eq!(encoding, Encoding::Windows1252);
        assert_eq!(Encoding::Windows1252.encode(&text), bytes);
    }

    #[test]
    fn empty_input_decodes_to_empty_utf8() {
        let (text, encoding) = Encoding::decode(&[]);
        assert_eq!(text, "");
        assert_eq!(encoding, Encoding::Utf8);
    }
}
