//! Turning a text into the sequence of keystrokes that types it (F-TXT-01/02).
//!
//! Characters are sent as Unicode code units, independently of the keyboard
//! layout; line breaks and tabs are sent as the Enter and Tab keys, which is
//! what applications (terminals, forms, editors) expect.

/// One unit of simulated typing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeUnit {
    /// A UTF-16 code unit (characters outside the BMP produce two units).
    Unit(u16),
    Enter,
    Tab,
}

/// Plans the keystrokes for `text`. `\r\n`, `\n` and `\r` each produce one Enter.
pub fn plan(text: &str) -> Vec<TypeUnit> {
    let mut out = Vec::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push(TypeUnit::Enter);
            }
            '\n' => out.push(TypeUnit::Enter),
            '\t' => out.push(TypeUnit::Tab),
            c => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    out.push(TypeUnit::Unit(*unit));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(s: &str) -> Vec<TypeUnit> {
        s.encode_utf16().map(TypeUnit::Unit).collect()
    }

    #[test]
    fn scenario_1_email_is_typed_verbatim() {
        let text = "prenom.nom@exemple.fr";
        assert_eq!(plan(text), units(text));
    }

    #[test]
    fn full_unicode() {
        // Accents, symbols and an emoji outside the BMP (surrogate pair).
        let planned = plan("é€😀");
        assert_eq!(planned, vec![TypeUnit::Unit(0xE9), TypeUnit::Unit(0x20AC), TypeUnit::Unit(0xD83D), TypeUnit::Unit(0xDE00)]);
    }

    #[test]
    fn line_breaks_and_tabs() {
        assert_eq!(plan("a\r\nb\nc\rd\te"), vec![
            TypeUnit::Unit(b'a' as u16),
            TypeUnit::Enter,
            TypeUnit::Unit(b'b' as u16),
            TypeUnit::Enter,
            TypeUnit::Unit(b'c' as u16),
            TypeUnit::Enter,
            TypeUnit::Unit(b'd' as u16),
            TypeUnit::Tab,
            TypeUnit::Unit(b'e' as u16),
        ]);
    }
}
