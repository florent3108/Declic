//! Instant search over shortcuts (name, combination, target, text).

use crate::model::{Action, Shortcut};

/// Lowercases and removes common diacritics so that "ecran" finds "Écran".
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        let base = match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => 'a',
            'ç' | 'ć' | 'č' => 'c',
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' | 'ě' => 'e',
            'ì' | 'í' | 'î' | 'ï' | 'ī' => 'i',
            'ñ' | 'ń' | 'ň' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => 'o',
            'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' => 'u',
            'ý' | 'ÿ' => 'y',
            'š' | 'ś' => 's',
            'ž' | 'ź' | 'ż' => 'z',
            'ł' => 'l',
            'œ' => {
                out.push_str("oe");
                continue;
            }
            'æ' => {
                out.push_str("ae");
                continue;
            }
            'ß' => {
                out.push_str("ss");
                continue;
            }
            other => other,
        };
        out.push(base);
    }
    out
}

/// Text in which a shortcut is searched. `displayed_keys` is the combination
/// as shown to the user (localised, layout-aware), in addition to the
/// canonical form.
pub fn haystack(shortcut: &Shortcut, displayed_keys: &str) -> String {
    let mut parts = vec![shortcut.name.clone(), shortcut.keys.to_string(), displayed_keys.to_string()];
    match &shortcut.action {
        Action::Open(open) => {
            parts.push(open.target.clone());
            parts.push(open.arguments.clone());
        }
        Action::Macro(m) => {
            for step in &m.steps {
                parts.extend(step.step.texts().into_iter().map(str::to_string));
            }
        }
    }
    parts.extend(shortcut.conditions.programs.iter().cloned());
    parts.push(shortcut.group.clone());
    fold(&parts.join("\n"))
}

/// Whether every word of the query appears in the haystack (already folded).
pub fn matches(folded_haystack: &str, query: &str) -> bool {
    fold(query).split_whitespace().all(|word| folded_haystack.contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    #[test]
    fn folding() {
        assert_eq!(fold("Écran Œuvre Straße"), "ecran oeuvre strasse");
    }

    #[test]
    fn search_on_all_fields() {
        let mut s = Shortcut::new(1, "Ctrl+Alt+M".parse().unwrap(), Action::text("prenom.nom@exemple.fr", TextMode::Typing));
        s.name = "Mon adresse e-mail".into();
        let hay = haystack(&s, "Ctrl + Alt + M");
        assert!(matches(&hay, "adresse"));
        assert!(matches(&hay, "ADRESSE mail"));
        assert!(matches(&hay, "exemple.fr"));
        assert!(matches(&hay, "ctrl+alt+m"));
        assert!(matches(&hay, ""));
        assert!(!matches(&hay, "chrome"));
        let open = Shortcut::new(2, "Win+N".parse().unwrap(), Action::open(r"C:\Windows\notepad.exe"));
        let hay = haystack(&open, "Win + N");
        assert!(matches(&hay, "notepad"));
        assert!(matches(&hay, "win+n"));
    }

    #[test]
    fn accents_are_ignored() {
        let mut s = Shortcut::new(1, "F9".parse().unwrap(), Action::open("x"));
        s.name = "Capture d'écran".into();
        assert!(matches(&haystack(&s, "F9"), "ecran"));
        assert!(matches(&haystack(&s, "F9"), "ÉCRAN"));
    }
}
