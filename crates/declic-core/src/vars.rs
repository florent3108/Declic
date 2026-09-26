//! Variables in targets, arguments and texts (F-ACT-04, F-TXT-05).
//!
//! Syntax (Windows-style, between percent signs):
//! - `%NAME%`: environment variable, e.g. `%USERPROFILE%`;
//! - `%CLIPBOARD%`: current text content of the clipboard;
//! - `%DATE%` / `%TIME%`: current date / time in the user's regional format;
//! - `%DATE:pattern%` / `%TIME:pattern%`: custom format, see [`format_datetime`];
//! - `%%`: a literal percent sign.
//!
//! Unknown names are left untouched, so ordinary texts such as "50% off,
//! 20% more" are typed as written.

/// A local date and time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalDateTime {
    pub year: i32,
    /// 1–12.
    pub month: u8,
    /// 1–31.
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    /// 0 = Monday … 6 = Sunday.
    pub weekday: u8,
}

/// Source of variable values, implemented by the platform layer (and by fakes in tests).
pub trait VarProvider {
    fn env_var(&self, name: &str) -> Option<String>;
    fn clipboard_text(&self) -> Option<String>;
    fn now(&self) -> LocalDateTime;
    /// Date in the user's regional short format, if the platform provides one.
    fn format_short_date(&self, _dt: &LocalDateTime) -> Option<String> {
        None
    }
    /// Time in the user's regional short format, if the platform provides one.
    fn format_short_time(&self, _dt: &LocalDateTime) -> Option<String> {
        None
    }
}

/// Localised month and weekday names used by custom date patterns.
#[derive(Debug, Clone)]
pub struct DateNames {
    pub months: [String; 12],
    pub weekdays: [String; 7],
}

impl DateNames {
    /// Builds names from two comma-separated lists (12 months, 7 weekdays from Monday).
    pub fn from_lists(months: &str, weekdays: &str) -> Option<DateNames> {
        let months: Vec<String> = months.split(',').map(|s| s.trim().to_string()).collect();
        let weekdays: Vec<String> = weekdays.split(',').map(|s| s.trim().to_string()).collect();
        Some(DateNames { months: months.try_into().ok()?, weekdays: weekdays.try_into().ok()? })
    }

    pub fn english() -> DateNames {
        DateNames::from_lists(
            "January,February,March,April,May,June,July,August,September,October,November,December",
            "Monday,Tuesday,Wednesday,Thursday,Friday,Saturday,Sunday",
        )
        .expect("static lists are well-formed")
    }
}

impl Default for DateNames {
    fn default() -> Self {
        DateNames::english()
    }
}

fn abbreviate(name: &str) -> String {
    name.chars().take(3).collect()
}

/// Formats a date/time with a simple pattern language:
/// `yyyy` `yy` (year), `MMMM` `MMM` `MM` `M` (month), `dddd` `ddd` `dd` `d`
/// (day), `HH` `H` (24 h), `hh` `h` (12 h), `mm` `m` (minutes), `ss` `s`
/// (seconds), `tt` (AM/PM). Any other character is copied as is, and text
/// between single quotes is copied literally.
pub fn format_datetime(dt: &LocalDateTime, pattern: &str, names: &DateNames) -> String {
    const TOKENS: [&str; 18] = [
        "yyyy", "MMMM", "dddd", "MMM", "ddd", "yy", "MM", "dd", "HH", "hh", "mm", "ss", "tt", "M", "d", "H", "h", "m",
    ];
    let mut out = String::new();
    let mut rest = pattern;
    'outer: while !rest.is_empty() {
        if let Some(stripped) = rest.strip_prefix('\'') {
            match stripped.find('\'') {
                Some(end) => {
                    out.push_str(&stripped[..end]);
                    rest = &stripped[end + 1..];
                }
                None => {
                    out.push_str(stripped);
                    rest = "";
                }
            }
            continue;
        }
        if let Some(stripped) = rest.strip_prefix('s') {
            // "s" alone is a token too (not in TOKENS to keep "ss" first).
            if !stripped.starts_with('s') {
                out.push_str(&dt.second.to_string());
                rest = stripped;
                continue;
            }
        }
        for token in TOKENS {
            if let Some(stripped) = rest.strip_prefix(token) {
                let hour12 = match dt.hour % 12 {
                    0 => 12,
                    h => h,
                };
                let value = match token {
                    "yyyy" => format!("{:04}", dt.year),
                    "yy" => format!("{:02}", dt.year.rem_euclid(100)),
                    "MMMM" => names.months[(dt.month.clamp(1, 12) - 1) as usize].clone(),
                    "MMM" => abbreviate(&names.months[(dt.month.clamp(1, 12) - 1) as usize]),
                    "MM" => format!("{:02}", dt.month),
                    "M" => dt.month.to_string(),
                    "dddd" => names.weekdays[(dt.weekday % 7) as usize].clone(),
                    "ddd" => abbreviate(&names.weekdays[(dt.weekday % 7) as usize]),
                    "dd" => format!("{:02}", dt.day),
                    "d" => dt.day.to_string(),
                    "HH" => format!("{:02}", dt.hour),
                    "H" => dt.hour.to_string(),
                    "hh" => format!("{hour12:02}"),
                    "h" => hour12.to_string(),
                    "mm" => format!("{:02}", dt.minute),
                    "m" => dt.minute.to_string(),
                    "ss" => format!("{:02}", dt.second),
                    "tt" => if dt.hour < 12 { "AM" } else { "PM" }.to_string(),
                    _ => unreachable!(),
                };
                out.push_str(&value);
                rest = stripped;
                continue 'outer;
            }
        }
        let c = rest.chars().next().expect("rest is not empty");
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Whether the text between two percent signs can be a variable reference.
/// Plain names contain no whitespace; date/time patterns may contain spaces.
fn is_valid_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 256 || name.contains(['\n', '\r', '\t']) {
        return false;
    }
    let upper = name.get(..5).map(str::to_ascii_uppercase);
    matches!(upper.as_deref(), Some("DATE:") | Some("TIME:")) || !name.contains(' ')
}

/// Expands the variables of `input`.
pub fn expand(input: &str, provider: &dyn VarProvider, names: &DateNames) -> String {
    let mut out = String::with_capacity(input.len());
    let mut now: Option<LocalDateTime> = None;
    let mut rest = input;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &after[..end];
        if name.is_empty() {
            out.push('%');
            rest = &after[end + 1..];
            continue;
        }
        let value = if is_valid_name(name) {
            resolve(name, provider, names, &mut now)
        } else {
            None
        };
        match value {
            Some(value) => {
                out.push_str(&value);
                rest = &after[end + 1..];
            }
            None => {
                // Not a variable: keep the first '%' and continue from the second one.
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn resolve(name: &str, provider: &dyn VarProvider, names: &DateNames, now: &mut Option<LocalDateTime>) -> Option<String> {
    let (base, pattern) = match name.split_once(':') {
        Some((base, pattern)) => (base, Some(pattern)),
        None => (name, None),
    };
    let upper = base.to_ascii_uppercase();
    match (upper.as_str(), pattern) {
        ("CLIPBOARD", None) => Some(provider.clipboard_text().unwrap_or_default()),
        ("DATE", None) => {
            let dt = *now.get_or_insert_with(|| provider.now());
            Some(provider.format_short_date(&dt).unwrap_or_else(|| format_datetime(&dt, "yyyy-MM-dd", names)))
        }
        ("TIME", None) => {
            let dt = *now.get_or_insert_with(|| provider.now());
            Some(provider.format_short_time(&dt).unwrap_or_else(|| format_datetime(&dt, "HH:mm", names)))
        }
        ("DATE" | "TIME", Some(pattern)) => {
            let dt = *now.get_or_insert_with(|| provider.now());
            Some(format_datetime(&dt, pattern, names))
        }
        (_, Some(_)) => None,
        (_, None) => provider.env_var(name),
    }
}

/// Whether the text contains the clipboard variable (useful to avoid reading
/// the clipboard needlessly).
pub fn uses_clipboard(input: &str) -> bool {
    input.to_ascii_uppercase().contains("%CLIPBOARD%")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Fake {
        env: HashMap<&'static str, &'static str>,
        clipboard: Option<&'static str>,
    }

    impl VarProvider for Fake {
        fn env_var(&self, name: &str) -> Option<String> {
            self.env.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.to_string())
        }
        fn clipboard_text(&self) -> Option<String> {
            self.clipboard.map(str::to_string)
        }
        fn now(&self) -> LocalDateTime {
            LocalDateTime { year: 2026, month: 9, day: 25, hour: 14, minute: 5, second: 9, weekday: 4 }
        }
    }

    fn fake() -> Fake {
        Fake {
            env: HashMap::from([("USERPROFILE", r"C:\Users\Marie"), ("ProgramFiles(x86)", r"C:\Program Files (x86)")]),
            clipboard: Some("copié"),
        }
    }

    fn french() -> DateNames {
        DateNames::from_lists(
            "janvier,février,mars,avril,mai,juin,juillet,août,septembre,octobre,novembre,décembre",
            "lundi,mardi,mercredi,jeudi,vendredi,samedi,dimanche",
        )
        .unwrap()
    }

    #[test]
    fn environment_variables() {
        let p = fake();
        assert_eq!(expand(r"%USERPROFILE%\Documents", &p, &DateNames::english()), r"C:\Users\Marie\Documents");
        assert_eq!(expand(r"%userprofile%", &p, &DateNames::english()), r"C:\Users\Marie");
        assert_eq!(expand(r"%ProgramFiles(x86)%\App", &p, &DateNames::english()), r"C:\Program Files (x86)\App");
    }

    #[test]
    fn unknown_names_and_lone_percent_signs_are_kept() {
        let p = fake();
        let names = DateNames::english();
        assert_eq!(expand("50% off, 20% more", &p, &names), "50% off, 20% more");
        assert_eq!(expand("%NOPE%", &p, &names), "%NOPE%");
        assert_eq!(expand("100%", &p, &names), "100%");
        assert_eq!(expand("100%% sûr", &p, &names), "100% sûr");
        assert_eq!(expand("a %NOPE% b %USERPROFILE%", &p, &names), r"a %NOPE% b C:\Users\Marie");
    }

    #[test]
    fn clipboard_date_and_time() {
        let p = fake();
        let names = french();
        assert_eq!(expand("[%CLIPBOARD%]", &p, &names), "[copié]");
        assert_eq!(expand("%DATE%", &p, &names), "2026-09-25");
        assert_eq!(expand("%TIME%", &p, &names), "14:05");
        assert_eq!(expand("%DATE:dddd d MMMM yyyy%", &p, &names), "vendredi 25 septembre 2026");
        assert_eq!(expand("%DATE:dd/MM/yy%", &p, &names), "25/09/26");
        assert_eq!(expand("%TIME:HH'h'mm%", &p, &names), "14h05");
        assert_eq!(expand("%TIME:h:mm:ss tt%", &p, &names), "2:05:09 PM");
        assert!(uses_clipboard("x %clipboard% y"));
        assert!(!uses_clipboard("x"));
    }

    #[test]
    fn empty_clipboard_expands_to_nothing() {
        let p = Fake { env: HashMap::new(), clipboard: None };
        assert_eq!(expand("<%CLIPBOARD%>", &p, &DateNames::english()), "<>");
    }

    #[test]
    fn scenario_6_dated_signature() {
        // A three-line signature containing today's date.
        struct Regional(Fake);
        impl VarProvider for Regional {
            fn env_var(&self, n: &str) -> Option<String> {
                self.0.env_var(n)
            }
            fn clipboard_text(&self) -> Option<String> {
                None
            }
            fn now(&self) -> LocalDateTime {
                self.0.now()
            }
            fn format_short_date(&self, dt: &LocalDateTime) -> Option<String> {
                Some(format!("{:02}/{:02}/{}", dt.day, dt.month, dt.year))
            }
        }
        let p = Regional(fake());
        let signature = "Marie Dupont\nService achats\nLe %DATE%";
        assert_eq!(expand(signature, &p, &french()), "Marie Dupont\nService achats\nLe 25/09/2026");
    }

    #[test]
    fn date_names_validation() {
        assert!(DateNames::from_lists("a,b", "c").is_none());
    }
}
