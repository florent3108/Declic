//! Key combinations ("combinaisons"): a main key plus modifier requirements.

use crate::key::{Key, strip_prefix_ci};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

/// One of the four modifier families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Win,
}

impl Modifier {
    pub const ALL: [Modifier; 4] = [Modifier::Ctrl, Modifier::Alt, Modifier::Shift, Modifier::Win];

    fn index(self) -> u8 {
        match self {
            Modifier::Ctrl => 0,
            Modifier::Alt => 1,
            Modifier::Shift => 2,
            Modifier::Win => 3,
        }
    }

    /// Canonical name used in the configuration file.
    pub fn id(self) -> &'static str {
        match self {
            Modifier::Ctrl => "Ctrl",
            Modifier::Alt => "Alt",
            Modifier::Shift => "Shift",
            Modifier::Win => "Win",
        }
    }

    fn parse(token: &str) -> Option<Modifier> {
        match token.to_lowercase().as_str() {
            "ctrl" | "control" | "ctl" | "strg" => Some(Modifier::Ctrl),
            "alt" => Some(Modifier::Alt),
            "shift" | "maj" => Some(Modifier::Shift),
            "win" | "windows" | "super" | "meta" => Some(Modifier::Win),
            _ => None,
        }
    }
}

/// Left or right variant of a modifier key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
}

/// What a combination requires from one modifier family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ModReq {
    /// The modifier must not be held.
    #[default]
    Off,
    /// Either side (or both) must be held.
    Any,
    /// The left key must be held, the right one must not.
    Left,
    /// The right key must be held, the left one must not.
    Right,
}

impl ModReq {
    fn accepts(self, left: bool, right: bool) -> bool {
        match self {
            ModReq::Off => !left && !right,
            ModReq::Any => left || right,
            ModReq::Left => left && !right,
            ModReq::Right => right && !left,
        }
    }

    /// Whether some physical state satisfies both requirements.
    fn compatible(self, other: ModReq) -> bool {
        [(false, false), (true, false), (false, true), (true, true)]
            .iter()
            .any(|&(l, r)| self.accepts(l, r) && other.accepts(l, r))
    }

    fn is_sided(self) -> bool {
        matches!(self, ModReq::Left | ModReq::Right)
    }
}

/// Physical state of the eight modifier keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ModState(u8);

impl ModState {
    pub const EMPTY: ModState = ModState(0);

    fn bit(m: Modifier, side: Side) -> u8 {
        let base = m.index() * 2;
        1 << (base + if side == Side::Right { 1 } else { 0 })
    }

    pub fn from_bits(bits: u8) -> Self {
        ModState(bits)
    }

    pub fn bits(self) -> u8 {
        self.0
    }

    pub fn with(self, m: Modifier, side: Side, down: bool) -> Self {
        let bit = Self::bit(m, side);
        if down { ModState(self.0 | bit) } else { ModState(self.0 & !bit) }
    }

    pub fn is_down(self, m: Modifier, side: Side) -> bool {
        self.0 & Self::bit(m, side) != 0
    }

    pub fn any(self, m: Modifier) -> bool {
        self.is_down(m, Side::Left) || self.is_down(m, Side::Right)
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// A key combination: one main key and a requirement for each modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hotkey {
    pub key: Key,
    pub ctrl: ModReq,
    pub alt: ModReq,
    pub shift: ModReq,
    pub win: ModReq,
}

impl Hotkey {
    pub fn new(key: Key) -> Self {
        Hotkey { key, ctrl: ModReq::Off, alt: ModReq::Off, shift: ModReq::Off, win: ModReq::Off }
    }

    pub fn req(&self, m: Modifier) -> ModReq {
        match m {
            Modifier::Ctrl => self.ctrl,
            Modifier::Alt => self.alt,
            Modifier::Shift => self.shift,
            Modifier::Win => self.win,
        }
    }

    pub fn set_req(&mut self, m: Modifier, req: ModReq) {
        match m {
            Modifier::Ctrl => self.ctrl = req,
            Modifier::Alt => self.alt = req,
            Modifier::Shift => self.shift = req,
            Modifier::Win => self.win = req,
        }
    }

    /// Builder-style variant of [`Hotkey::set_req`].
    pub fn with(mut self, m: Modifier, req: ModReq) -> Self {
        self.set_req(m, req);
        self
    }

    /// Builds a combination from a captured physical state. When `sided` is
    /// false, held modifiers are recorded as "either side".
    pub fn from_state(key: Key, state: ModState, sided: bool) -> Self {
        let mut hk = Hotkey::new(key);
        for m in Modifier::ALL {
            let l = state.is_down(m, Side::Left);
            let r = state.is_down(m, Side::Right);
            let req = match (l, r) {
                (false, false) => ModReq::Off,
                (true, false) if sided => ModReq::Left,
                (false, true) if sided => ModReq::Right,
                _ => ModReq::Any,
            };
            hk.set_req(m, req);
        }
        hk
    }

    /// Whether any modifier is required.
    pub fn has_modifiers(&self) -> bool {
        Modifier::ALL.iter().any(|&m| self.req(m) != ModReq::Off)
    }

    /// Whether left/right sides are distinguished for at least one modifier.
    pub fn is_sided(&self) -> bool {
        Modifier::ALL.iter().any(|&m| self.req(m).is_sided())
    }

    /// Number of modifiers constrained to one side (used for specificity).
    pub fn sided_count(&self) -> u8 {
        Modifier::ALL.iter().filter(|&&m| self.req(m).is_sided()).count() as u8
    }

    /// Same combination, with every side-specific requirement relaxed to "either side".
    pub fn unsided(mut self) -> Self {
        for m in Modifier::ALL {
            if self.req(m).is_sided() {
                self.set_req(m, ModReq::Any);
            }
        }
        self
    }

    /// Makes every "either side" requirement side-specific, defaulting to the left key.
    pub fn sided_default_left(mut self) -> Self {
        for m in Modifier::ALL {
            if self.req(m) == ModReq::Any {
                self.set_req(m, ModReq::Left);
            }
        }
        self
    }

    /// The set of modifier families that are required, regardless of side.
    pub fn modifier_families(&self) -> Vec<Modifier> {
        Modifier::ALL.iter().copied().filter(|&m| self.req(m) != ModReq::Off).collect()
    }

    /// Whether pressing `key` while the modifiers are in `state` triggers this combination.
    pub fn matches(&self, key: Key, state: ModState) -> bool {
        self.key == key
            && Modifier::ALL.iter().all(|&m| {
                self.req(m).accepts(state.is_down(m, Side::Left), state.is_down(m, Side::Right))
            })
    }

    /// Whether some keyboard state triggers both combinations.
    pub fn overlaps(&self, other: &Hotkey) -> bool {
        self.key == other.key && Modifier::ALL.iter().all(|&m| self.req(m).compatible(other.req(m)))
    }

    /// Whether this combination only uses the given modifier families (any side).
    pub fn is_exactly(&self, families: &[Modifier]) -> bool {
        Modifier::ALL
            .iter()
            .all(|m| (self.req(*m) != ModReq::Off) == families.contains(m))
    }
}

impl fmt::Display for Hotkey {
    /// Canonical, layout-independent text form, e.g. `Ctrl+Alt+M` or `RightCtrl+P`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for m in Modifier::ALL {
            match self.req(m) {
                ModReq::Off => continue,
                ModReq::Any => write!(f, "{}+", m.id())?,
                ModReq::Left => write!(f, "Left{}+", m.id())?,
                ModReq::Right => write!(f, "Right{}+", m.id())?,
            }
        }
        write!(f, "{}", self.key.id())
    }
}

/// Error returned when a combination cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyParseError(pub String);

impl fmt::Display for HotkeyParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid key combination: {}", self.0)
    }
}

impl std::error::Error for HotkeyParseError {}

pub(crate) fn parse_modifier_token(token: &str) -> Option<(Modifier, ModReq)> {
    if let Some(m) = Modifier::parse(token) {
        return Some((m, ModReq::Any));
    }
    for (prefixes, req) in [(["left", "l"], ModReq::Left), (["right", "r"], ModReq::Right)] {
        for prefix in prefixes {
            if let Some(rest) = strip_prefix_ci(token, prefix)
                && let Some(m) = Modifier::parse(rest)
            {
                return Some((m, req));
            }
        }
    }
    None
}

impl FromStr for Hotkey {
    type Err = HotkeyParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || HotkeyParseError(s.to_string());
        let tokens: Vec<&str> = s.split('+').map(str::trim).collect();
        let (last, mods) = tokens.split_last().ok_or_else(err)?;
        if last.is_empty() || mods.iter().any(|t| t.is_empty()) {
            return Err(err());
        }
        let key = Key::from_id(last).ok_or_else(err)?;
        let mut hk = Hotkey::new(key);
        for token in mods {
            let (m, req) = parse_modifier_token(token).ok_or_else(err)?;
            if hk.req(m) != ModReq::Off {
                return Err(err());
            }
            hk.set_req(m, req);
        }
        Ok(hk)
    }
}

impl Serialize for Hotkey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Hotkey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(mods: &[(Modifier, Side)]) -> ModState {
        mods.iter().fold(ModState::EMPTY, |s, &(m, side)| s.with(m, side, true))
    }

    #[test]
    fn parse_and_display_round_trip() {
        for text in ["Ctrl+Alt+M", "Win+N", "Ctrl+Num1", "RightCtrl+P", "LeftAlt+Shift+F5", "F13", "Ctrl+Shift+S"] {
            let hk: Hotkey = text.parse().unwrap();
            assert_eq!(hk.to_string(), text);
        }
    }

    #[test]
    fn parse_is_lenient_about_case_order_and_aliases() {
        let hk: Hotkey = "alt + ctrl + m".parse().unwrap();
        assert_eq!(hk.to_string(), "Ctrl+Alt+M");
        let hk: Hotkey = "Maj+RCtrl+Esc".parse().unwrap();
        assert_eq!(hk.to_string(), "RightCtrl+Shift+Escape");
        assert!("Ctrl+".parse::<Hotkey>().is_err());
        assert!("Ctrl+Ctrl+A".parse::<Hotkey>().is_err());
        assert!("Hyper+A".parse::<Hotkey>().is_err());
        assert!("".parse::<Hotkey>().is_err());
    }

    #[test]
    fn unsided_modifiers_match_either_side() {
        let hk: Hotkey = "Ctrl+Alt+M".parse().unwrap();
        let left = st(&[(Modifier::Ctrl, Side::Left), (Modifier::Alt, Side::Left)]);
        let right = st(&[(Modifier::Ctrl, Side::Right), (Modifier::Alt, Side::Left)]);
        assert!(hk.matches(Key::M, left));
        assert!(hk.matches(Key::M, right));
        assert!(!hk.matches(Key::N, left));
        // An extra modifier prevents the match.
        assert!(!hk.matches(Key::M, left.with(Modifier::Shift, Side::Left, true)));
        // A missing modifier prevents the match.
        assert!(!hk.matches(Key::M, st(&[(Modifier::Ctrl, Side::Left)])));
    }

    #[test]
    fn sided_modifiers_distinguish_left_and_right() {
        let right_ctrl_p: Hotkey = "RightCtrl+P".parse().unwrap();
        assert!(right_ctrl_p.matches(Key::P, st(&[(Modifier::Ctrl, Side::Right)])));
        assert!(!right_ctrl_p.matches(Key::P, st(&[(Modifier::Ctrl, Side::Left)])));
        let left_ctrl_p: Hotkey = "LeftCtrl+P".parse().unwrap();
        assert!(!left_ctrl_p.overlaps(&right_ctrl_p));
        let ctrl_p: Hotkey = "Ctrl+P".parse().unwrap();
        assert!(ctrl_p.overlaps(&right_ctrl_p));
    }

    #[test]
    fn capture_from_state() {
        let state = st(&[(Modifier::Ctrl, Side::Right), (Modifier::Shift, Side::Left)]);
        assert_eq!(Hotkey::from_state(Key::S, state, false).to_string(), "Ctrl+Shift+S");
        assert_eq!(Hotkey::from_state(Key::S, state, true).to_string(), "RightCtrl+LeftShift+S");
        let both = st(&[(Modifier::Ctrl, Side::Right), (Modifier::Ctrl, Side::Left)]);
        assert_eq!(Hotkey::from_state(Key::S, both, true).to_string(), "Ctrl+S");
    }

    #[test]
    fn overlap_requires_same_key_and_compatible_modifiers() {
        let a: Hotkey = "Ctrl+Alt+M".parse().unwrap();
        let b: Hotkey = "Ctrl+Alt+M".parse().unwrap();
        let c: Hotkey = "Ctrl+M".parse().unwrap();
        let d: Hotkey = "Ctrl+Alt+N".parse().unwrap();
        assert!(a.overlaps(&b));
        assert!(!a.overlaps(&c));
        assert!(!a.overlaps(&d));
    }

    #[test]
    fn sided_helpers() {
        let hk: Hotkey = "Ctrl+Alt+M".parse().unwrap();
        assert!(!hk.is_sided());
        let sided = hk.sided_default_left();
        assert_eq!(sided.to_string(), "LeftCtrl+LeftAlt+M");
        assert_eq!(sided.sided_count(), 2);
        assert_eq!(sided.unsided(), hk);
        assert!(hk.is_exactly(&[Modifier::Ctrl, Modifier::Alt]));
        assert!(!hk.is_exactly(&[Modifier::Ctrl]));
    }

    #[test]
    fn modstate_bits() {
        let s = ModState::EMPTY.with(Modifier::Win, Side::Right, true);
        assert!(s.is_down(Modifier::Win, Side::Right));
        assert!(!s.is_down(Modifier::Win, Side::Left));
        assert!(s.any(Modifier::Win));
        assert!(s.with(Modifier::Win, Side::Right, false).is_empty());
        assert_eq!(ModState::from_bits(s.bits()), s);
    }
}
