# Functional specification
## Declic — keyboard shortcut application for Windows

**Version 0.2** · September 26, 2026 · English translation of the original
French specification (version 0.1, September 25, 2026).

**Status:** the MVP and V1 (§ 13) are implemented; V2 items remain open.

---

## 0. Design rules ("clean room")

- This document describes **only the expected behaviour, as seen by the
  user**. It contains no implementation details.
- The development team **has no access to the source code of other keyboard
  shortcut software** and reuses none of its code, UI texts, translations,
  icons or documentation.
- Texts, documentation, translations and visuals are **created specifically
  for this project**.
- The software architecture is designed freely, in idiomatic Rust.

---

## 1. Goal

Let any Windows user bind a **key combination** to an **action**:

- open a program, a file, a folder or a website;
- type a text (email address, signature, formula…);
- chain a small sequence of actions (macro).

All of this in a **modern, clear interface that is quick to learn**, for an
audience ranging from beginners ("I just want to type my email address with
one key") to advanced users (macros with windows and the mouse).

## 2. Guiding principles

1. **Discreet**: runs in the background and uses very few resources.
2. **Immediate**: creating a simple shortcut takes less than 15 seconds.
3. **Reliable**: typed text arrives exactly as intended, in any application.
4. **Readable**: what a shortcut does is clear at a glance.
5. **No surprises**: no silent conflicts between shortcuts.

## 3. Vocabulary

| Term | Definition |
|---|---|
| **Shortcut** | A combination, an action, conditions and descriptive information (name, group) bound together. |
| **Combination** | A main key with zero or more modifiers. |
| **Modifier** | Ctrl, Alt, Shift, Win. Left and right sides can be told apart. |
| **Action** | What happens when the shortcut is triggered: *Open*, *Text* or *Macro*. |
| **Step** | One item of a macro (wait, click, type…). |
| **Condition** | A rule that limits when the shortcut is active (foreground program, lock-key state). |
| **Group** | A folder used to organize shortcuts. |

---

## 4. Key combinations

| Ref. | Requirement |
|---|---|
| F-COMB-01 | **Any key** can be the main key: letters, digits, punctuation, F1 to F24, arrows, Home, End, Page Up/Down, Insert, Delete, Esc, Tab, Enter, Space, Backspace, Print Screen, Pause, Scroll Lock, Menu key, **numeric keypad** (distinct from the top-row digits), **media keys** (play, volume, track…), **browser keys** (back, home, favorites…) and **launch keys** (mail, calculator…). |
| F-COMB-02 | Modifiers combine freely. Per-shortcut option: **tell left and right apart**, e.g. Right Ctrl + P different from Left Ctrl + P. |
| F-COMB-03 | **Entry by recording**: a "Press your combination…" field records what the user presses. While recording, no existing shortcut fires, including combinations normally intercepted by Windows (Win+E, for example). |
| F-COMB-04 | **Manual entry** as an alternative: searchable key list and modifier checkboxes, for keys missing from the physical keyboard. |
| F-COMB-05 | Combinations are displayed as **drawn keys** (keycaps), following the **user's keyboard layout** (AZERTY, QWERTY, QWERTZ…). |
| F-COMB-06 | A shortcut can **take over a combination already used** by Windows or an application when technically possible. Otherwise, a clear message explains why. |
| F-COMB-07 | **Lock-key conditions**: for Caps Lock, Num Lock and Scroll Lock, either *any*, *on* or *off*. |

---

## 5. Actions

The interface offers **three action types**: **Open**, **Text** and
**Macro**. Internally, *Text* is a one-step macro. The user can turn a *Text*
into a *Macro* at any time.

### 5.1 Open

| Ref. | Requirement |
|---|---|
| F-ACT-01 | Possible targets: executable, document (opened with the default application), folder, web address (http/https), mailto:, other registered protocols, special Windows locations (Settings, Recycle Bin…). |
| F-ACT-02 | Options: arguments, working folder, window state (normal, minimized, maximized). **New**: run as administrator. |
| F-ACT-03 | Picking aids: file/folder picker; **list of installed programs** with search and icons; **drag-and-drop** of a file or Windows shortcut into the editor. For a `.lnk` shortcut, the real target is shown. |
| F-ACT-04 | **Variables** usable in the target, the arguments and texts: environment variables (`%USERPROFILE%`…), clipboard text content, today's date, time. |
| F-ACT-05 | If the target cannot be found, a non-blocking notification says so. |

### 5.2 Text

| Ref. | Requirement |
|---|---|
| F-TXT-01 | A text, on one or several lines, is typed into the active window **as if the user typed it**. It must work in all common applications: browsers, Office, terminals, messaging apps, windowed games. |
| F-TXT-02 | **Full Unicode**: accents, symbols, emoji, whatever the keyboard layout. |
| F-TXT-03 | Keys still held by the user when the shortcut fires (for example Ctrl and Alt of Ctrl+Alt+M) **must not alter** the typed text. |
| F-TXT-04 | **New**, two input modes: *typing* (default) or *paste*. Paste goes through the clipboard, is faster for long texts, and the original clipboard content is restored afterwards. |
| F-TXT-05 | The variables of F-ACT-04 are available. |

### 5.3 Macro

| Ref. | Requirement |
|---|---|
| F-MAC-01 | A macro is an **ordered list of steps** run one after the other. |
| F-MAC-02 | **Visual block editor**: add through a "+" menu, reorder by drag-and-drop, duplicate, delete, temporarily disable a step. |
| F-MAC-03 | A running macro can be **interrupted** with a stop key (Esc by default, configurable). |
| F-MAC-04 | **Eyedropper**: for mouse and window steps, the user can click a window or a screen position to fill in the field automatically. |
| F-MAC-05 | A progress bar or a discreet indicator shows that a macro is running. |

**Available steps:**

| Step | Settings |
|---|---|
| Type a text | Text, mode (typing/paste). |
| Press keys | Combination (Ctrl+V, Enter, Tab…) and number of repetitions. |
| Hold / release keys | Keys to keep pressed during the following steps, then release. |
| Wait | Duration in milliseconds. |
| Activate a window | Title (with `*` wildcards) or program; maximum wait; if not found: *stop the macro* or *continue*. |
| Activate or launch | As above; if the window doesn't exist, launch a target then wait for it to appear. |
| Open | Same settings as the *Open* action (§ 5.1). |
| Copy to clipboard | Text (variables allowed). |
| Mouse click | Button (left, right, middle), type (single, double, press, release). |
| Move the mouse | Absolute screen position, relative to the active window or relative to the current position. |
| Wheel | Direction (up, down, left, right) and number of notches. |
| Notification | **New**: show a short message. |

---

## 6. Activation conditions

| Ref. | Requirement |
|---|---|
| F-CND-01 | **Per program**: *everywhere*, *only in…* or *everywhere except…*, with a list of programs identified by their executable (`chrome.exe`…). |
| F-CND-02 | Programs are added with the **eyedropper** (click a window on screen), from the list of open programs, or typed manually. |
| F-CND-03 | The same combination can be used by several shortcuts if their conditions don't overlap. Example: Ctrl+Shift+S does A in Word and B elsewhere. **The most specific wins**: *only in* > *everywhere except* > *everywhere*. |
| F-CND-04 | If no shortcut matches in the current context, the combination is **passed on normally** to the active application, as if the application didn't exist. |
| F-CND-05 | Lock-key conditions are described in F-COMB-07. |
| F-CND-06 | *(V2)* Condition on the active window's title. |

---

## 7. Conflicts and validation

| Ref. | Requirement |
|---|---|
| F-CNF-01 | **Live detection** while editing: same combination with overlapping conditions. A message names the conflicting shortcut, with a link to open it. |
| F-CNF-02 | **Unavailable** combination (reserved by Windows or another application): a warning badge shows in the list and in the editor. |
| F-CNF-03 | Warning for **risky combinations**: Ctrl+C without a condition, a single key without a modifier, etc. |
| F-CNF-04 | Required fields are checked before saving, with clear messages next to the field concerned. |

---

## 8. Interface

### 8.1 Notification area (taskbar)
- **Left click**: opens the main window.
- **Menu**: Open · Pause all shortcuts (the icon then changes) · Settings ·
  Quit.
- Configurable **global combination** to open the main window.

### 8.2 Main window
- **Search bar**: instant filter on name, combination, target and text. A
  button allows **searching by combination** by pressing it directly.
- **Side panel**: groups (*All*, *Favorites*, user groups) and filters by
  action type.
- **Shortcut list**, one row per shortcut:
  - icon (the target program's, or the action type's);
  - name;
  - combination as drawn keys;
  - action summary;
  - condition badges;
  - usage counter;
  - on/off switch.
- **Sorting** by name, combination, type, number of uses or date of last use.
- **Multi-selection**: delete, enable/disable, move to a group, duplicate,
  export.
- A clearly visible **"New shortcut"** button. On first launch, a
  **welcoming empty state** offers a few ready-to-use examples.
- **Undoable deletion**: an "Undo" bar stays visible for a few seconds.

### 8.3 Shortcut editor
Shown as a side panel, in a natural filling order:
1. **Combination** (recording).
2. **Action type**: Open / Text / Macro.
3. Action **details**.
4. **Conditions**, in an "Advanced" section collapsed by default.
5. **Name and group**. The name is suggested automatically, for example
   "Open Chrome", and stays editable.

**Test** button, which runs the action. For a text: in a test area of the
editor, or in the previous window after a countdown.

Save (Ctrl+S) / Cancel (Esc).

### 8.4 Cheat sheet *(new)*
An overlay window, opened with a configurable combination, lists the
shortcuts active in the current application.

### 8.5 Look and accessibility
- **Windows 11 (Fluent)** style; **light/dark** theme following the system
  or forced; system accent colour.
- Crisp rendering at high resolution (high DPI), multi-monitor support.
- Application **fully usable from the keyboard**; compatible with screen
  readers (Narrator, NVDA).
- Discreet animations, turned off if the user turned them off in Windows.

---

## 9. Settings

- Start with Windows.
- Language (automatic by default).
- Theme.
- Combination to open the main window; cheat-sheet combination.
- Default text input mode; delay between keystrokes (for slow
  applications).
- Macro stop key.
- Notifications on or off.
- Data location: **standard** (user profile) or **portable** (next to the
  executable, detected automatically).

---

## 10. Data

| Ref. | Requirement |
|---|---|
| F-DAT-01 | Configuration stored in a **human-readable text file that can be edited by hand** (format to be defined, for example TOML), with a format version number for future migrations. |
| F-DAT-02 | **Automatic save** on every change, with atomic writes: the file must never be corrupted, even on power loss. |
| F-DAT-03 | Automatic retention of the last *n* versions of the file. |
| F-DAT-04 | **Automatic reload** if the file is changed outside the application. |
| F-DAT-05 | **Import/export** of all shortcuts or a selection. On import, duplicates are handled by merging, replacing or ignoring, as chosen. |
| F-DAT-06 | **Readable export** of the list (CSV, or copy as a table) to print or share it. |
| F-DAT-07 | **Statistics**: number of uses and date of last use per shortcut; can be reset. |
| F-DAT-08 | *(V2)* **Import from other shortcut software**, from the user's configuration file (interoperability). The format will be described in a separate appendix, based on a user's configuration file. |

---

## 11. Languages

- **V1**: French and English.
- Translations in **simple external files**, to make community contributions
  easy (other languages in V2).
- Plurals and date/number formats handled per language.
- All texts are written specifically for this project.

---

## 12. Non-functional requirements

| Criterion | Target |
|---|---|
| Memory at rest (window closed) | < 15 MB |
| Delay between key press and start of the action | < 50 ms |
| Startup until shortcuts are active | < 500 ms |
| Opening the main window | < 300 ms |
| CPU at rest | ≈ 0 % |

- **Systems**: Windows 10 (22H2) and Windows 11, x64. ARM64 in V2.
- **No administrator rights** needed to install or use the application.
- **Administrator windows**: Windows prevents a normal application from
  sending keystrokes to a window launched as administrator. The application
  must detect this and tell the user. *(V2)* Option to start the application
  itself as administrator.
- **Robustness**: a failing shortcut never crashes the application. An event
  log can be viewed from the settings.
- **Privacy**: no telemetry. No network access, except an update check
  explicitly enabled by the user (V2).
- **Distribution**: a single executable, plus a lightweight installer.

---

## 13. Prioritization

| Phase | Content |
|---|---|
| **MVP** (usable day to day) | Combinations and recording · *Open* and *Text* actions · Per-program conditions · Notification area · List and search · Editor · Configuration file · Start with Windows · French/English · Dark theme |
| **V1** | Full macros (block editor, mouse, windows) · Complete conflict detection · Groups · Import/export · Global pause · Cheat sheet · Statistics · Paste mode |
| **V2** | Advanced text mode for macros · Import from other shortcut software · Window-title condition · ARM64 · More languages · Updates · Administrator mode |

---

## 14. Open questions

1. ~~Name~~ → **Declic** (decided September 25, 2026).
2. ~~License~~ → Declic is open source under the dual **MIT OR Apache-2.0**
   license (decided September 25, 2026). User interface: **iced** (MIT). All
   dependencies must use a permissive license (MIT, Apache-2.0, BSD, ISC,
   Zlib…): no GPL/LGPL/AGPL, and no visible attribution required in the
   interface. Third-party license texts are gathered in a file shipped with
   the application.
3. ~~Shortcut triggered while a macro is running~~ → **ignored** (and logged)
   rather than queued: the safer choice.
4. Default combination to open the main window.
5. Distribution: website only, or also winget / Microsoft Store?

---

## Appendix A — Text syntax for macros *(V2, to be designed)*

The configuration file stores steps in a **structured** way, independently
of any text syntax. The advanced text mode is just another way to edit the
same macro, with round-tripping between the visual editor and text. This
syntax will be designed specifically for this project: readable,
unambiguous, easy to escape.

## Appendix B — Usage scenarios (used as acceptance tests)

1. **Email address**: Ctrl+Alt+M types `first.last@example.com` in Word,
   Chrome and Notepad, without any stray character.
2. **Launch or switch to**: Win+N activates Notepad if it's open, otherwise
   launches it.
3. **Application-specific shortcut**: Ctrl+Shift+S only has an effect in
   Excel. Elsewhere, the combination works normally.
4. **Media key**: the keyboard's "Calculator" key opens another application
   chosen by the user.
5. **Numeric keypad**: Ctrl+Num 1 opens the "Projects" folder, while Ctrl+1
   (top row) stays free.
6. **Dated signature**: a combination types a three-line signature
   containing today's date.
7. **Mouse macro**: a combination activates a window, clicks at a position
   relative to it, waits 200 ms, then presses Enter. Esc interrupts the macro
   at any time.
8. **Conflict**: creating Ctrl+Alt+M a second time without a condition
   immediately shows a warning naming the existing shortcut.
