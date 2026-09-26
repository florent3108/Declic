//! Translations loaded from simple external TOML files.
//!
//! A translation file is a TOML document whose (possibly nested) keys map to
//! strings. Nested tables are flattened with dots, so
//! `[editor]\nsave = "Save"` defines the key `editor.save`.
//!
//! - Placeholders: `{name}` is replaced by the argument named `name`.
//! - Plurals: a key can be a table whose entries are CLDR plural categories
//!   (`zero`, `one`, `two`, `few`, `many`, `other`); see [`Catalog::plural`]
//!   and [`plural_category`].

use std::collections::HashMap;

/// The CLDR plural categories, in their usual order.
pub const PLURAL_CATEGORIES: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];

/// How integers are written: digit-group separator and group sizes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NumberFormat {
    /// Separator between digit groups (e.g. "," in English, a no-break
    /// space in French); empty for no grouping.
    pub separator: String,
    /// Sizes of the groups from the right, the last one repeating
    /// (`[3]` for 1,234,567; `[3, 2]` for the Indian 12,34,567).
    pub groups: Vec<usize>,
}

impl NumberFormat {
    /// Parses a Windows `LOCALE_SGROUPING` value such as `3;0` or `3;2;0`.
    pub fn from_windows(separator: &str, grouping: &str) -> NumberFormat {
        let mut groups: Vec<usize> = grouping.split(';').filter_map(|g| g.trim().parse().ok()).collect();
        // A trailing 0 means "repeat the previous size", which is what we do anyway.
        while groups.last() == Some(&0) {
            groups.pop();
        }
        NumberFormat { separator: separator.to_string(), groups }
    }

    /// Writes an integer with digit grouping.
    pub fn format(&self, n: i64) -> String {
        let digits = n.unsigned_abs().to_string();
        let sign = if n < 0 { "-" } else { "" };
        if self.separator.is_empty() || self.groups.is_empty() || self.groups.contains(&0) {
            return format!("{sign}{digits}");
        }
        let chars: Vec<char> = digits.chars().collect();
        let mut parts: Vec<String> = Vec::new();
        let mut end = chars.len();
        let mut index = 0;
        while end > 0 {
            let size = self.groups[index.min(self.groups.len() - 1)];
            let start = end.saturating_sub(size);
            parts.push(chars[start..end].iter().collect());
            end = start;
            index += 1;
        }
        parts.reverse();
        format!("{sign}{}", parts.join(&self.separator))
    }
}

/// A set of translated strings for one language, with an optional fallback.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    lang: String,
    strings: HashMap<String, String>,
    fallback: Option<Box<Catalog>>,
    numbers: NumberFormat,
    /// Locale used for dates and numbers (`None`: the user's own settings).
    format_locale: Option<String>,
}

fn flatten(prefix: &str, table: &toml::Table, out: &mut HashMap<String, String>) {
    for (key, value) in table {
        let full = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
        match value {
            toml::Value::String(s) => {
                out.insert(full, s.clone());
            }
            toml::Value::Table(t) => flatten(&full, t, out),
            other => {
                out.insert(full, other.to_string());
            }
        }
    }
}

impl Catalog {
    /// Parses a translation file.
    pub fn from_toml(lang: &str, source: &str) -> Result<Catalog, String> {
        let table: toml::Table = source.parse().map_err(|e: toml::de::Error| e.to_string())?;
        let mut strings = HashMap::new();
        flatten("", &table, &mut strings);
        Ok(Catalog { lang: lang.to_string(), strings, fallback: None, numbers: NumberFormat::default(), format_locale: None })
    }

    /// Uses `fallback` for keys missing from this catalog.
    pub fn with_fallback(mut self, fallback: Catalog) -> Catalog {
        self.fallback = Some(Box::new(fallback));
        self
    }

    /// Adds (or overrides) strings from another catalog of the same language.
    pub fn merge(&mut self, other: Catalog) {
        self.strings.extend(other.strings);
    }

    /// Sets how numbers are written (see [`Catalog::number`]).
    pub fn set_number_format(&mut self, format: NumberFormat) {
        self.numbers = format;
    }

    /// Writes an integer the way this language (and region) does.
    pub fn number(&self, n: i64) -> String {
        self.numbers.format(n)
    }

    /// Sets the locale used for dates and numbers (`None`: the user's own
    /// regional settings).
    pub fn set_format_locale(&mut self, locale: Option<String>) {
        self.format_locale = locale;
    }

    /// Locale used for dates and numbers (`None`: the user's own settings).
    pub fn format_locale(&self) -> Option<&str> {
        self.format_locale.as_deref()
    }

    /// A string of this catalog itself, ignoring the fallback.
    pub fn own(&self, key: &str) -> Option<&str> {
        self.strings.get(key).map(String::as_str)
    }

    /// Language code of the catalog, e.g. `fr` or `pt-BR`.
    pub fn lang(&self) -> &str {
        &self.lang
    }

    /// Display name of the language, taken from the `language.name` key.
    pub fn language_name(&self) -> &str {
        self.strings.get("language.name").map(String::as_str).unwrap_or(&self.lang)
    }

    /// Whether the language is written from right to left
    /// (`language.direction = "rtl"`).
    pub fn is_rtl(&self) -> bool {
        self.strings.get("language.direction").is_some_and(|d| d.eq_ignore_ascii_case("rtl"))
    }

    fn lookup(&self, key: &str) -> Option<&str> {
        self.strings
            .get(key)
            .map(String::as_str)
            .or_else(|| self.fallback.as_ref().and_then(|f| f.lookup(key)))
    }

    /// Whether the key exists (in this catalog or its fallback).
    pub fn has(&self, key: &str) -> bool {
        self.lookup(key).is_some()
    }

    /// Returns the translation of `key`, or the key itself when missing.
    pub fn get<'a>(&'a self, key: &'a str) -> &'a str {
        self.lookup(key).unwrap_or(key)
    }

    /// Returns the translation of `key` with `{name}` placeholders replaced.
    pub fn fmt(&self, key: &str, args: &[(&str, &str)]) -> String {
        substitute(self.get(key), args)
    }

    /// Picks the plural form of `key` for `n` in this catalog's own strings:
    /// an explicit `zero` entry for 0, then the CLDR category of the
    /// language, then `other`. Missing forms are taken, as a whole, from the
    /// fallback catalog (with the fallback language's rules).
    fn plural_template(&self, key: &str, n: i64) -> Option<&str> {
        let own = |suffix: &str| self.strings.get(&format!("{key}.{suffix}")).map(String::as_str);
        let found = if n == 0 { own("zero") } else { None }
            .or_else(|| own(plural_category(&self.lang, n)))
            .or_else(|| own("other"));
        found.or_else(|| self.fallback.as_ref().and_then(|f| f.plural_template(key, n)))
    }

    /// Plural-aware translation: picks the form of `key` matching `n` (see
    /// [`plural_category`]) and replaces `{n}` (written with digit grouping)
    /// and the other placeholders.
    pub fn plural(&self, key: &str, n: i64, args: &[(&str, &str)]) -> String {
        let n_text = self.number(n);
        let mut all_args: Vec<(&str, &str)> = vec![("n", n_text.as_str())];
        all_args.extend_from_slice(args);
        substitute(self.plural_template(key, n).unwrap_or(key), &all_args)
    }
}

fn substitute(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) => {
                let name = &after[..end];
                match args.iter().find(|(k, _)| *k == name) {
                    Some((_, value)) => out.push_str(value),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Names of the `{placeholders}` used in a template, in order of appearance.
pub fn placeholders(template: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) => {
                out.push(after[..end].to_string());
                rest = &after[end + 1..];
            }
            None => break,
        }
    }
    out
}

fn base_language(lang: &str) -> String {
    lang.split(['-', '_']).next().unwrap_or(lang).to_ascii_lowercase()
}

/// CLDR plural category of an integer in a language (rules of CLDR 44 for
/// integers; `lang` is a code such as `fr`, `pt-PT` or `zh-TW`).
pub fn plural_category(lang: &str, n: i64) -> &'static str {
    let lower = lang.to_ascii_lowercase().replace('_', "-");
    let i = n.unsigned_abs();
    let (m10, m100) = (i % 10, i % 100);
    let millions = i != 0 && i.is_multiple_of(1_000_000);
    match base_language(&lower).as_str() {
        // No plural forms.
        "ja" | "zh" | "ko" | "vi" | "th" | "id" | "ms" | "lo" | "my" | "km" => "other",
        // 0 and 1 are singular; exact millions take "de" / "de" forms.
        "fr" => {
            if i <= 1 {
                "one"
            } else if millions {
                "many"
            } else {
                "other"
            }
        }
        "pt" => {
            let european = lower.starts_with("pt-pt");
            if i == 1 || (!european && i == 0) {
                "one"
            } else if millions {
                "many"
            } else {
                "other"
            }
        }
        "es" | "it" | "ca" => {
            if i == 1 {
                "one"
            } else if millions {
                "many"
            } else {
                "other"
            }
        }
        // Hindi: 0 and 1 are singular.
        "hi" | "bn" | "gu" | "kn" | "mr" | "fa" | "am" | "zu" => {
            if i <= 1 {
                "one"
            } else {
                "other"
            }
        }
        "ru" | "uk" | "be" => {
            if m10 == 1 && m100 != 11 {
                "one"
            } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
                "few"
            } else {
                "many"
            }
        }
        "pl" => {
            if i == 1 {
                "one"
            } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
                "few"
            } else {
                "many"
            }
        }
        // Czech and Slovak: "many" only exists for fractions.
        "cs" | "sk" => {
            if i == 1 {
                "one"
            } else if (2..=4).contains(&i) {
                "few"
            } else {
                "other"
            }
        }
        "ro" | "mo" => {
            if i == 1 {
                "one"
            } else if i == 0 || (1..=19).contains(&m100) {
                "few"
            } else {
                "other"
            }
        }
        "ar" => {
            if i == 0 {
                "zero"
            } else if i == 1 {
                "one"
            } else if i == 2 {
                "two"
            } else if (3..=10).contains(&m100) {
                "few"
            } else if (11..=99).contains(&m100) {
                "many"
            } else {
                "other"
            }
        }
        "he" | "iw" => {
            if i == 1 {
                "one"
            } else if i == 2 {
                "two"
            } else {
                "other"
            }
        }
        // English, German, Dutch, Scandinavian, Finnish, Greek, Turkish,
        // Hungarian and most other languages.
        _ => {
            if i == 1 {
                "one"
            } else {
                "other"
            }
        }
    }
}

/// The plural categories a language uses for integers (in CLDR order).
pub fn plural_categories(lang: &str) -> Vec<&'static str> {
    let mut samples: Vec<i64> = (0..=1_000).collect();
    samples.extend([1_000_000, 2_000_000, 1_000_001]);
    PLURAL_CATEGORIES
        .iter()
        .copied()
        .filter(|c| samples.iter().any(|&n| plural_category(lang, n) == *c))
        .collect()
}

/// Finds the available language that best matches a BCP-47 tag such as
/// `pt-PT`, `zh-Hant-TW` or `nn-NO`.
pub fn match_language(tag: &str, available: &[&str]) -> Option<String> {
    let tag = tag.trim().replace('_', "-").to_ascii_lowercase();
    if tag.is_empty() {
        return None;
    }
    let find = |code: &str| available.iter().find(|a| a.eq_ignore_ascii_case(code)).map(|a| a.to_string());
    if let Some(found) = find(&tag) {
        return Some(found);
    }
    let parts: Vec<&str> = tag.split('-').collect();
    let lang = match parts[0] {
        "iw" => "he",
        "in" => "id",
        "no" | "nn" => "nb",
        "ji" => "yi",
        other => other,
    };
    let script = parts[1..].iter().find(|p| p.len() == 4).copied();
    let region = parts[1..].iter().find(|p| p.len() == 2 || (p.len() == 3 && p.chars().all(|c| c.is_ascii_digit()))).copied();
    // Variants that are separate translations.
    let preferred = match lang {
        "zh" => Some(match (script, region) {
            (Some("hant"), _) => "zh-tw",
            (Some(_), _) => "zh-cn",
            (None, Some("tw" | "hk" | "mo")) => "zh-tw",
            _ => "zh-cn",
        }),
        "pt" => Some(match region {
            None | Some("br") => "pt-br",
            Some(_) => "pt-pt",
        }),
        _ => None,
    };
    let mut candidates: Vec<String> = Vec::new();
    if let Some(p) = preferred {
        candidates.push(p.to_string());
    }
    if let Some(r) = region {
        candidates.push(format!("{lang}-{r}"));
    }
    candidates.push(lang.to_string());
    for candidate in &candidates {
        if let Some(found) = find(candidate) {
            return Some(found);
        }
    }
    // Any other variant of the same language (e.g. "pt-PT" for "pt-BR").
    available.iter().find(|a| base_language(a) == lang).map(|a| a.to_string())
}

/// Picks the language to use: the explicit preference when available,
/// otherwise the system UI language when available, otherwise English (or the
/// first available language).
pub fn resolve_language(preference: &str, system: &str, available: &[&str]) -> String {
    if !preference.eq_ignore_ascii_case("auto")
        && let Some(found) = match_language(preference, available)
    {
        return found;
    }
    match_language(system, available)
        .or_else(|| match_language("en", available))
        .or_else(|| available.first().map(|a| a.to_string()))
        .unwrap_or_else(|| "en".to_string())
}

/// Sort key for language names written in their own language: accents are
/// ignored, and names in Latin script come first, then Greek, Cyrillic,
/// Hebrew, Arabic, Indic, Thai and East Asian scripts (Unicode order).
pub fn language_sort_key(name: &str) -> String {
    name.chars()
        .flat_map(|c| {
            let folded = match c {
                'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' | 'ă' | 'ą' | 'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' | 'Ă' | 'Ą' => 'a',
                'ç' | 'č' | 'ć' | 'Ç' | 'Č' | 'Ć' => 'c',
                'ď' | 'đ' | 'Ď' | 'Đ' => 'd',
                'é' | 'è' | 'ê' | 'ë' | 'ě' | 'ę' | 'ế' | 'ệ' | 'ề' | 'É' | 'È' | 'Ê' | 'Ë' | 'Ě' | 'Ę' => 'e',
                'ğ' | 'Ğ' => 'g',
                'í' | 'ì' | 'î' | 'ï' | 'ı' | 'Í' | 'Ì' | 'Î' | 'Ï' | 'İ' => 'i',
                'ľ' | 'ĺ' | 'ł' | 'Ľ' | 'Ĺ' | 'Ł' => 'l',
                'ñ' | 'ň' | 'ń' | 'Ñ' | 'Ň' | 'Ń' => 'n',
                'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ő' | 'ø' | 'ơ' | 'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' | 'Ő' | 'Ø' => 'o',
                'ř' | 'ŕ' | 'Ř' | 'Ŕ' => 'r',
                'š' | 'ś' | 'ș' | 'ş' | 'Š' | 'Ś' | 'Ș' | 'Ş' => 's',
                'ť' | 'ț' | 'ţ' | 'Ť' | 'Ț' | 'Ţ' => 't',
                'ú' | 'ù' | 'û' | 'ü' | 'ů' | 'ű' | 'ư' | 'Ú' | 'Ù' | 'Û' | 'Ü' | 'Ů' | 'Ű' => 'u',
                'ý' | 'ÿ' | 'Ý' => 'y',
                'ž' | 'ź' | 'ż' | 'Ž' | 'Ź' | 'Ż' => 'z',
                other => other,
            };
            folded.to_lowercase()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const EN: &str = r#"
        [language]
        name = "English"
        [editor]
        save = "Save"
        hello = "Hello {name}!"
        [list.count]
        zero = "No shortcut"
        one = "{n} shortcut"
        other = "{n} shortcuts"
        only_en = "English only"
    "#;

    const FR: &str = r#"
        [language]
        name = "Français"
        [editor]
        save = "Enregistrer"
        hello = "Bonjour {name} !"
        [list.count]
        one = "{n} raccourci"
        other = "{n} raccourcis"
    "#;

    const RU: &str = r#"
        [language]
        name = "Русский"
        [list.count]
        one = "{n} ярлык"
        few = "{n} ярлыка"
        many = "{n} ярлыков"
    "#;

    fn fr() -> Catalog {
        Catalog::from_toml("fr", FR).unwrap().with_fallback(Catalog::from_toml("en", EN).unwrap())
    }

    #[test]
    fn lookup_with_fallback() {
        let fr = fr();
        assert_eq!(fr.get("editor.save"), "Enregistrer");
        assert_eq!(fr.get("list.count.only_en"), "English only");
        assert_eq!(fr.get("missing.key"), "missing.key");
        assert_eq!(fr.language_name(), "Français");
        assert!(!fr.is_rtl());
        let he = Catalog::from_toml("he", "[language]\nname = \"עברית\"\ndirection = \"rtl\"").unwrap();
        assert!(he.is_rtl());
    }

    #[test]
    fn placeholders_are_replaced_and_listed() {
        let fr = fr();
        assert_eq!(fr.fmt("editor.hello", &[("name", "Marie")]), "Bonjour Marie !");
        assert_eq!(substitute("{a} {b} {", &[("a", "1")]), "1 {b} {");
        assert_eq!(placeholders("{n} of {file} {"), vec!["n", "file"]);
    }

    #[test]
    fn plurals_use_the_catalog_forms() {
        let fr = fr();
        assert_eq!(fr.plural("list.count", 0, &[]), "0 raccourci");
        assert_eq!(fr.plural("list.count", 1, &[]), "1 raccourci");
        assert_eq!(fr.plural("list.count", 2, &[]), "2 raccourcis");
        let en = Catalog::from_toml("en", EN).unwrap();
        assert_eq!(en.plural("list.count", 0, &[]), "No shortcut");
        assert_eq!(en.plural("list.count", 1, &[]), "1 shortcut");
        assert_eq!(en.plural("list.count", 5, &[]), "5 shortcuts");
        let mut ru = Catalog::from_toml("ru", RU).unwrap().with_fallback(Catalog::from_toml("en", EN).unwrap());
        ru.set_number_format(NumberFormat::from_windows("\u{a0}", "3;0"));
        assert_eq!(ru.plural("list.count", 1, &[]), "1 ярлык");
        assert_eq!(ru.plural("list.count", 3, &[]), "3 ярлыка");
        assert_eq!(ru.plural("list.count", 11, &[]), "11 ярлыков");
        assert_eq!(ru.plural("list.count", 1_021, &[]), "1\u{a0}021 ярлык");
        // A key missing in Russian comes whole from English.
        assert_eq!(ru.plural("editor.hello", 2, &[]), "editor.hello");
    }

    #[test]
    fn plural_rules() {
        let cat = |lang: &str, n: i64| plural_category(lang, n);
        // One/other with 1 singular.
        for lang in ["en", "de", "nl", "sv", "da", "nb", "fi", "el", "tr", "hu", "it", "es", "pt-PT"] {
            assert_eq!(cat(lang, 1), "one", "{lang}");
            assert_eq!(cat(lang, 0), "other", "{lang}");
            assert_eq!(cat(lang, 2), "other", "{lang}");
            assert_eq!(cat(lang, 21), "other", "{lang}");
        }
        // 0 and 1 singular.
        for lang in ["fr", "fr-CA", "pt-BR", "pt", "hi"] {
            assert_eq!(cat(lang, 0), "one", "{lang}");
            assert_eq!(cat(lang, 1), "one", "{lang}");
            assert_eq!(cat(lang, 2), "other", "{lang}");
        }
        assert_eq!(cat("fr", 1_000_000), "many");
        assert_eq!(cat("es", 2_000_000), "many");
        assert_eq!(cat("hi", 1_000_000), "other");
        // No plural.
        for lang in ["ja", "ko", "zh-CN", "zh-TW", "th", "vi", "id"] {
            assert!([0, 1, 2, 5, 11, 101].iter().all(|&n| cat(lang, n) == "other"), "{lang}");
        }
        // Slavic: one / few / many.
        for lang in ["ru", "uk"] {
            let got: Vec<_> = [1, 2, 4, 5, 11, 12, 14, 21, 22, 25, 101, 111, 0].iter().map(|&n| cat(lang, n)).collect();
            assert_eq!(got, ["one", "few", "few", "many", "many", "many", "many", "one", "few", "many", "one", "many", "many"], "{lang}");
        }
        let pl: Vec<_> = [1, 2, 4, 5, 12, 21, 22, 25, 0, 101].iter().map(|&n| cat("pl", n)).collect();
        assert_eq!(pl, ["one", "few", "few", "many", "many", "many", "few", "many", "many", "many"]);
        for lang in ["cs", "sk"] {
            let got: Vec<_> = [0, 1, 2, 4, 5, 22].iter().map(|&n| cat(lang, n)).collect();
            assert_eq!(got, ["other", "one", "few", "few", "other", "other"], "{lang}");
        }
        let ro: Vec<_> = [0, 1, 2, 19, 20, 101, 119, 120].iter().map(|&n| cat("ro", n)).collect();
        assert_eq!(ro, ["few", "one", "few", "few", "other", "few", "few", "other"]);
        let ar: Vec<_> = [0, 1, 2, 3, 10, 11, 99, 100, 102, 103, 111].iter().map(|&n| cat("ar", n)).collect();
        assert_eq!(ar, ["zero", "one", "two", "few", "few", "many", "many", "other", "other", "few", "many"]);
        let he: Vec<_> = [0, 1, 2, 3, 20].iter().map(|&n| cat("he", n)).collect();
        assert_eq!(he, ["other", "one", "two", "other", "other"]);
        // Negative numbers follow the absolute value.
        assert_eq!(cat("ru", -21), "one");
    }

    #[test]
    fn categories_per_language() {
        assert_eq!(plural_categories("en"), ["one", "other"]);
        assert_eq!(plural_categories("ja"), ["other"]);
        assert_eq!(plural_categories("ru"), ["one", "few", "many"]);
        assert_eq!(plural_categories("cs"), ["one", "few", "other"]);
        assert_eq!(plural_categories("ar"), ["zero", "one", "two", "few", "many", "other"]);
        assert_eq!(plural_categories("fr"), ["one", "many", "other"]);
        assert_eq!(plural_categories("he"), ["one", "two", "other"]);
    }

    #[test]
    fn language_resolution() {
        let available = [
            "en", "fr", "de", "es", "pt-BR", "pt-PT", "zh-CN", "zh-TW", "nb", "he", "id", "ar", "ja",
        ];
        let auto = |system: &str| resolve_language("auto", system, &available);
        assert_eq!(auto("fr-FR"), "fr");
        assert_eq!(auto("fr-CA"), "fr");
        assert_eq!(auto("de-AT"), "de");
        assert_eq!(auto("es-419"), "es");
        assert_eq!(auto("en-GB"), "en");
        assert_eq!(auto("pt-BR"), "pt-BR");
        assert_eq!(auto("pt-PT"), "pt-PT");
        assert_eq!(auto("pt-AO"), "pt-PT");
        assert_eq!(auto("pt"), "pt-BR");
        assert_eq!(auto("zh-CN"), "zh-CN");
        assert_eq!(auto("zh-SG"), "zh-CN");
        assert_eq!(auto("zh-Hans-HK"), "zh-CN");
        assert_eq!(auto("zh-Hans"), "zh-CN");
        assert_eq!(auto("zh-TW"), "zh-TW");
        assert_eq!(auto("zh-HK"), "zh-TW");
        assert_eq!(auto("zh-MO"), "zh-TW");
        assert_eq!(auto("zh-Hant"), "zh-TW");
        assert_eq!(auto("zh_Hant_TW"), "zh-TW");
        assert_eq!(auto("nb-NO"), "nb");
        assert_eq!(auto("nn-NO"), "nb");
        assert_eq!(auto("no"), "nb");
        assert_eq!(auto("iw-IL"), "he");
        assert_eq!(auto("in-ID"), "id");
        assert_eq!(auto("ar-SA"), "ar");
        assert_eq!(auto("ja-JP"), "ja");
        assert_eq!(auto("sr-Latn-RS"), "en");
        assert_eq!(auto(""), "en");
        // Explicit preferences, including codes written in lower case by
        // hand in the configuration file.
        assert_eq!(resolve_language("pt-br", "fr-FR", &available), "pt-BR");
        assert_eq!(resolve_language("de", "fr-FR", &available), "de");
        assert_eq!(resolve_language("it", "fr-BE", &available), "fr");
        assert_eq!(resolve_language("auto", "de", &["fr"]), "fr");
        // Only one Portuguese or Chinese variant available.
        assert_eq!(resolve_language("auto", "pt-BR", &["en", "pt-PT"]), "pt-PT");
        assert_eq!(resolve_language("auto", "zh-TW", &["en", "zh-CN"]), "zh-CN");
    }

    #[test]
    fn number_formats() {
        let en = NumberFormat::from_windows(",", "3;0");
        assert_eq!(en.format(0), "0");
        assert_eq!(en.format(999), "999");
        assert_eq!(en.format(1_234), "1,234");
        assert_eq!(en.format(-1_234_567), "-1,234,567");
        let hi = NumberFormat::from_windows(",", "3;2;0");
        assert_eq!(hi.format(1_234_567), "12,34,567");
        assert_eq!(NumberFormat::default().format(12_345), "12345");
        assert_eq!(NumberFormat::from_windows(".", "0").format(12_345), "12345");
    }

    #[test]
    fn language_names_sort_by_script_then_letters() {
        let mut names = vec!["日本語", "Deutsch", "Čeština", "العربية", "English", "Ελληνικά", "Русский", "Español", "Dansk", "עברית", "한국어"];
        names.sort_by_key(|n| language_sort_key(n));
        assert_eq!(names, ["Čeština", "Dansk", "Deutsch", "English", "Español", "Ελληνικά", "Русский", "עברית", "العربية", "日本語", "한국어"]);
    }

    #[test]
    fn invalid_file_is_an_error() {
        assert!(Catalog::from_toml("xx", "not = [valid").is_err());
    }
}
