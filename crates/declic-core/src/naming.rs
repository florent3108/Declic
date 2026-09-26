//! Automatic name suggestions ("Ouvrir Chrome", "Saisir « … »").

use crate::model::{Action, ActionKind};

/// A language-neutral name suggestion; the UI turns it into a sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameHint {
    /// "Open {subject}".
    Open(String),
    /// "Type {excerpt}".
    Type(String),
    /// A macro with the given number of steps.
    Macro(usize),
}

const EXCERPT_LEN: usize = 28;

/// Suggests a name for an action, or `None` when there is nothing to describe yet.
pub fn suggest(action: &Action) -> Option<NameHint> {
    match action {
        Action::Open(open) => open_subject(&open.target).map(NameHint::Open),
        Action::Macro(_) if action.kind() == ActionKind::Text => {
            let (text, _) = action.as_text()?;
            excerpt(text).map(NameHint::Type)
        }
        Action::Macro(m) if m.steps.is_empty() => None,
        Action::Macro(m) => Some(NameHint::Macro(m.steps.len())),
    }
}

/// First non-empty line of a text, shortened.
pub fn excerpt(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    let mut out: String = line.chars().take(EXCERPT_LEN).collect();
    if line.chars().count() > EXCERPT_LEN || text.trim().lines().count() > 1 {
        out.push('…');
    }
    Some(out)
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Human-friendly subject for a target: program name, folder name, web site…
pub fn open_subject(target: &str) -> Option<String> {
    let target = target.trim().trim_matches('"').trim();
    if target.is_empty() {
        return None;
    }
    let lower = target.to_ascii_lowercase();
    for scheme in ["https://", "http://"] {
        if lower.starts_with(scheme) {
            let rest = &target[scheme.len()..];
            let host = rest.split(['/', '?', '#', ':']).next().unwrap_or(rest);
            let host = host.strip_prefix("www.").unwrap_or(host);
            return (!host.is_empty()).then(|| host.to_string());
        }
    }
    if lower.starts_with("mailto:") {
        let address = &target["mailto:".len()..];
        let address = address.split('?').next().unwrap_or(address);
        return (!address.is_empty()).then(|| address.to_string());
    }
    // Other protocols (ms-settings:, shell:…) and plain names.
    let trimmed = target.trim_end_matches(['\\', '/']);
    let last = trimmed.rsplit(['\\', '/']).next().unwrap_or(trimmed);
    if last.is_empty() {
        return Some(target.to_string());
    }
    let is_drive = last.len() == 2 && last.ends_with(':');
    if is_drive {
        return Some(format!("{last}\\"));
    }
    let stem = match last.rsplit_once('.') {
        Some((stem, ext))
            if !stem.is_empty()
                && ["exe", "lnk", "bat", "cmd", "com", "url", "appref-ms"].contains(&ext.to_ascii_lowercase().as_str()) =>
        {
            capitalize(stem)
        }
        _ => last.to_string(),
    };
    Some(stem)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TextMode;

    #[test]
    fn subjects() {
        assert_eq!(open_subject(r"C:\Program Files\Google\Chrome\Application\chrome.exe").as_deref(), Some("Chrome"));
        assert_eq!(open_subject("notepad.exe").as_deref(), Some("Notepad"));
        assert_eq!(open_subject(r"C:\Users\Marie\Projets\").as_deref(), Some("Projets"));
        assert_eq!(open_subject(r"C:\Docs\rapport.docx").as_deref(), Some("rapport.docx"));
        assert_eq!(open_subject("https://www.example.org/page?x=1").as_deref(), Some("example.org"));
        assert_eq!(open_subject("mailto:marie@exemple.fr?subject=x").as_deref(), Some("marie@exemple.fr"));
        assert_eq!(open_subject("ms-settings:display").as_deref(), Some("ms-settings:display"));
        assert_eq!(open_subject(r"D:\").as_deref(), Some(r"D:\"));
        assert_eq!(open_subject("  "), None);
    }

    #[test]
    fn suggestions() {
        assert_eq!(suggest(&Action::open("calc.exe")), Some(NameHint::Open("Calc".into())));
        assert_eq!(
            suggest(&Action::text("prenom.nom@exemple.fr", TextMode::Typing)),
            Some(NameHint::Type("prenom.nom@exemple.fr".into()))
        );
        assert_eq!(
            suggest(&Action::text("Marie\nService achats", TextMode::Typing)),
            Some(NameHint::Type("Marie…".into()))
        );
        assert_eq!(suggest(&Action::text("", TextMode::Typing)), None);
        let long = "a".repeat(40);
        assert_eq!(excerpt(&long).unwrap().chars().count(), EXCERPT_LEN + 1);
    }
}
