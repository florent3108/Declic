//! Loading translations and producing localised labels.

use crate::paths;
use declic_core::i18n::{Catalog, NumberFormat, language_sort_key, match_language, resolve_language};
use declic_core::vars::DateNames;
use declic_core::{Hotkey, Key, KeyCategory, ModReq, Modifier};
use std::fs;

macro_rules! embedded {
    ($($code:literal),* $(,)?) => {
        [$(($code, include_str!(concat!("../../../lang/", $code, ".toml")))),*]
    };
}

/// Translation files built into the executable (English first: it is the
/// fallback for missing keys).
const EMBEDDED: [(&str, &str); 31] = embedded![
    "en", "fr", "de", "es", "it", "pt-BR", "pt-PT", "nl", "pl", "cs", "sk", "hu", "ro", "sv", "da", "nb", "fi", "el",
    "ru", "uk", "tr", "ja", "ko", "zh-CN", "zh-TW", "vi", "id", "th", "hi", "ar", "he",
];

fn external(code: &str) -> Option<Catalog> {
    let path = paths::lang_dir()?.join(format!("{code}.toml"));
    let text = fs::read_to_string(path).ok()?;
    Catalog::from_toml(code, &text).ok()
}

fn catalog_for(code: &str) -> Option<Catalog> {
    let embedded = EMBEDDED
        .iter()
        .find(|(c, _)| c.eq_ignore_ascii_case(code))
        .and_then(|(c, text)| Catalog::from_toml(c, text).ok());
    match (embedded, external(code)) {
        (Some(mut base), Some(ext)) => {
            base.merge(ext);
            Some(base)
        }
        (Some(base), None) => Some(base),
        (None, ext) => ext,
    }
}

/// Native names of the built-in languages, read once per process.
fn embedded_names() -> &'static [(String, String)] {
    static NAMES: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        EMBEDDED
            .iter()
            .filter_map(|(code, text)| Catalog::from_toml(code, text).ok().map(|c| (code.to_string(), c.language_name().to_string())))
            .collect()
    })
}

/// Codes and native names of the available languages (built-in plus the
/// files found in the `lang` folder next to the executable), sorted by name.
pub fn available_languages() -> Vec<(String, String)> {
    let mut languages: Vec<(String, String)> = embedded_names().to_vec();
    if let Some(dir) = paths::lang_dir()
        && let Ok(entries) = fs::read_dir(dir)
    {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
            if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("toml")) {
                continue;
            }
            // A file for a built-in language only overrides some texts.
            if languages.iter().any(|(c, _)| c.eq_ignore_ascii_case(stem)) {
                continue;
            }
            if let Some(catalog) = external(stem) {
                languages.push((stem.to_string(), catalog.language_name().to_string()));
            }
        }
    }
    languages.sort_by_cached_key(|(_, name)| language_sort_key(name));
    languages
}

/// Resolves the language preference (`auto` or a code) to a language code.
pub fn resolve(preference: &str) -> String {
    resolve_among(preference, &available_languages())
}

/// Like [`resolve`], among an already known list of languages.
pub fn resolve_among(preference: &str, languages: &[(String, String)]) -> String {
    let codes: Vec<&str> = languages.iter().map(|(c, _)| c.as_str()).collect();
    resolve_language(preference, &declic_win::system::ui_language(), &codes)
}

/// Locale for dates and numbers: the user's own regional settings when they
/// are in the interface language (so that their customisations are kept),
/// otherwise the usual formats of the interface language (`format.locale`).
fn format_locale(catalog: &Catalog) -> Option<String> {
    let user = declic_win::system::user_locale();
    if !user.is_empty() && match_language(&user, &[catalog.lang()]).is_some() {
        return None;
    }
    catalog.own("format.locale").map(str::to_string)
}

/// Loads the catalog for a language preference, with English as fallback,
/// and the matching date and number formats.
pub fn load(preference: &str) -> Catalog {
    let code = resolve(preference);
    let english = catalog_for("en").unwrap_or_default();
    let mut catalog = if code.eq_ignore_ascii_case("en") {
        english
    } else {
        match catalog_for(&code) {
            Some(catalog) => catalog.with_fallback(english),
            None => english,
        }
    };
    let locale = format_locale(&catalog);
    let (separator, grouping) = declic_win::system::number_grouping(locale.as_deref());
    catalog.set_number_format(NumberFormat::from_windows(&separator, &grouping));
    catalog.set_format_locale(locale);
    catalog
}

/// Font family recommended by a catalog (`language.font`).
pub fn font_family(tr: &Catalog) -> &str {
    tr.own("language.font").unwrap_or("Segoe UI")
}

/// Date and time of a Unix timestamp, in the formats of the interface
/// language (see [`load`]).
pub fn datetime(tr: &Catalog, secs: u64) -> String {
    declic_win::system::format_unix_time_in(secs, tr.format_locale()).unwrap_or_default()
}

/// Month and weekday names for date variables.
pub fn date_names(tr: &Catalog) -> DateNames {
    DateNames::from_lists(tr.get("dates.months"), tr.get("dates.weekdays")).unwrap_or_default()
}

/// Label of a key as printed on the user's keyboard or translated.
pub fn key_label(tr: &Catalog, key: Key) -> String {
    let id = key.id();
    let translation_key = format!("keys.{id}");
    if tr.has(&translation_key) {
        return tr.get(&translation_key).to_string();
    }
    match key.category() {
        KeyCategory::Letter | KeyCategory::Digit | KeyCategory::Function => id.into_owned(),
        KeyCategory::Punctuation => declic_win::keys::layout_char(key).map(|c| c.to_string()).unwrap_or_else(|| id.into_owned()),
        _ => declic_win::keys::system_key_name(key).unwrap_or_else(|| id.into_owned()),
    }
}

fn modifier_label(tr: &Catalog, m: Modifier, req: ModReq) -> String {
    let base = tr.get(match m {
        Modifier::Ctrl => "modifiers.ctrl",
        Modifier::Alt => "modifiers.alt",
        Modifier::Shift => "modifiers.shift",
        Modifier::Win => "modifiers.win",
    });
    match req {
        ModReq::Left => tr.fmt("modifiers.left", &[("key", base)]),
        ModReq::Right => tr.fmt("modifiers.right", &[("key", base)]),
        _ => base.to_string(),
    }
}

/// Labels of the keycaps of a combination, modifiers first.
pub fn keycaps(tr: &Catalog, hotkey: &Hotkey) -> Vec<String> {
    let mut caps: Vec<String> = Modifier::ALL
        .iter()
        .filter(|&&m| hotkey.req(m) != ModReq::Off)
        .map(|&m| modifier_label(tr, m, hotkey.req(m)))
        .collect();
    caps.push(key_label(tr, hotkey.key));
    caps
}

/// Combination as a single line of text, e.g. "Ctrl + Alt + M".
pub fn hotkey_text(tr: &Catalog, hotkey: &Hotkey) -> String {
    keycaps(tr, hotkey).join(" + ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use declic_core::i18n::{PLURAL_CATEGORIES, placeholders, plural_categories, plural_category};
    use std::collections::{BTreeMap, BTreeSet};

    /// Leaf strings of a translation file, with plural tables kept apart.
    struct Entries {
        strings: BTreeMap<String, String>,
        plurals: BTreeMap<String, BTreeMap<String, String>>,
    }

    fn entries(src: &str) -> Entries {
        fn walk(prefix: &str, t: &toml::Table, out: &mut Entries) {
            let is_plural = !t.is_empty() && t.iter().all(|(k, v)| PLURAL_CATEGORIES.contains(&k.as_str()) && v.is_str());
            if is_plural {
                let forms = t.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string())).collect();
                out.plurals.insert(prefix.to_string(), forms);
                return;
            }
            for (k, v) in t {
                let full = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                match v {
                    toml::Value::Table(t) => walk(&full, t, out),
                    toml::Value::String(s) => {
                        out.strings.insert(full, s.clone());
                    }
                    other => panic!("{full} is not a string: {other}"),
                }
            }
        }
        let mut out = Entries { strings: BTreeMap::new(), plurals: BTreeMap::new() };
        walk("", &src.parse::<toml::Table>().expect("valid TOML"), &mut out);
        out
    }

    fn sorted(v: Vec<String>) -> Vec<String> {
        let mut v = v;
        v.sort();
        v
    }

    /// Every built-in translation defines exactly the keys of en.toml, with
    /// the same placeholders, and the plural forms its language needs.
    #[test]
    fn every_translation_is_complete() {
        let en = entries(EMBEDDED[0].1);
        assert!(en.plurals.len() >= 10 && en.strings.len() >= 400);
        let mut problems = Vec::new();
        for (code, src) in EMBEDDED {
            let lang = entries(src);
            let keys: BTreeSet<_> = lang.strings.keys().collect();
            let en_keys: BTreeSet<_> = en.strings.keys().collect();
            for missing in en_keys.difference(&keys) {
                problems.push(format!("{code}: missing {missing}"));
            }
            for extra in keys.difference(&en_keys) {
                problems.push(format!("{code}: unknown key {extra}"));
            }
            for (key, text) in &lang.strings {
                if let Some(en_text) = en.strings.get(key)
                    && sorted(placeholders(text)) != sorted(placeholders(en_text))
                {
                    problems.push(format!("{code}: placeholders of {key} differ from English: {text:?}"));
                }
                if text.trim().is_empty() {
                    problems.push(format!("{code}: {key} is empty"));
                }
            }
            // Plural forms: the categories the language uses for counts, no
            // others, and the English placeholders ({n} may be left out of a
            // form that stands for a single number, like English "one").
            let allowed = plural_categories(code);
            let required: BTreeSet<&str> = (0..=10_000).map(|n| plural_category(code, n)).collect();
            for (key, en_forms) in &en.plurals {
                let Some(forms) = lang.plurals.get(key) else {
                    problems.push(format!("{code}: missing plural {key}"));
                    continue;
                };
                for category in &required {
                    if !forms.contains_key(*category) {
                        problems.push(format!("{code}: plural {key} lacks the \"{category}\" form"));
                    }
                }
                let en_names: BTreeSet<String> = placeholders(&en_forms["other"]).into_iter().collect();
                for (category, text) in forms {
                    if !allowed.contains(&category.as_str()) {
                        problems.push(format!("{code}: plural {key} has a \"{category}\" form that {code} does not use"));
                    }
                    let names: BTreeSet<String> = placeholders(text).into_iter().collect();
                    let mut expected = en_names.clone();
                    let single_number = (0..=10_000).filter(|&n| plural_category(code, n) == category.as_str()).count() == 1;
                    if single_number && !names.contains("n") {
                        expected.remove("n");
                    }
                    if names != expected {
                        problems.push(format!("{code}: placeholders of {key}.{category} differ from English: {text:?}"));
                    }
                }
            }
            for key in lang.plurals.keys() {
                if !en.plurals.contains_key(key) {
                    problems.push(format!("{code}: unknown plural {key}"));
                }
            }
            let catalog = Catalog::from_toml(code, src).unwrap();
            let names = date_names(&catalog);
            if DateNames::from_lists(catalog.get("dates.months"), catalog.get("dates.weekdays")).is_none() {
                problems.push(format!("{code}: dates need 12 months and 7 weekdays"));
            }
            assert!(!names.months[0].is_empty());
            let direction = catalog.get("language.direction");
            let expected_direction = if matches!(code, "ar" | "he") { "rtl" } else { "ltr" };
            if direction != expected_direction {
                problems.push(format!("{code}: direction {direction}"));
            }
            if declic_win::system::format_unix_time_in(1_767_323_045, Some(catalog.get("format.locale"))).is_none() {
                problems.push(format!("{code}: unknown locale {}", catalog.get("format.locale")));
            }
        }
        assert!(problems.is_empty(), "{} problem(s):\n{}", problems.len(), problems.join("\n"));
    }

    #[test]
    fn languages_are_listed_by_native_name() {
        let languages = available_languages();
        let names: Vec<&str> = languages.iter().map(|(_, n)| n.as_str()).collect();
        assert!(names.len() >= EMBEDDED.len());
        let position = |name: &str| names.iter().position(|n| *n == name).unwrap_or_else(|| panic!("{name} missing"));
        assert!(position("Dansk") < position("Deutsch"));
        assert!(position("English") < position("Français"));
        assert!(position("Türkçe") < position("Ελληνικά"));
        assert!(position("Русский") < position("עברית"));
        assert!(position("العربية") < position("日本語"));
        // Codes keep their written form.
        assert!(languages.iter().any(|(c, _)| c == "pt-BR"));
    }

    #[test]
    fn catalogs_load_with_fonts_and_directions() {
        let ja = catalog_for("ja").unwrap();
        assert_eq!(font_family(&ja), "Yu Gothic UI");
        assert!(catalog_for("ar").unwrap().is_rtl());
        assert!(catalog_for("he").unwrap().is_rtl());
        assert!(!catalog_for("de").unwrap().is_rtl());
        assert_eq!(catalog_for("PT-br").unwrap().lang(), "pt-BR");
        assert_eq!(font_family(&catalog_for("en").unwrap()), "Segoe UI");
    }

    #[test]
    fn keycap_labels() {
        let fr = Catalog::from_toml("fr", EMBEDDED[1].1).unwrap();
        let hk: Hotkey = "Ctrl+Shift+Num1".parse().unwrap();
        assert_eq!(keycaps(&fr, &hk), vec!["Ctrl", "Maj", "Num 1"]);
        let hk: Hotkey = "RightCtrl+P".parse().unwrap();
        assert_eq!(hotkey_text(&fr, &hk), "Ctrl droit + P");
        let de = catalog_for("de").unwrap();
        let hk: Hotkey = "Ctrl+Shift+Delete".parse().unwrap();
        assert_eq!(keycaps(&de, &hk), vec!["Strg", "Umschalt", "Entf"]);
        let hk: Hotkey = "Ctrl+PageUp".parse().unwrap();
        assert_eq!(hotkey_text(&de, &hk), "Strg + Bild↑");
        assert!(date_names(&fr).months[7].starts_with("ao"));
    }
}
