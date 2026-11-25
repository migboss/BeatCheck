use crate::brand::{self, StrokeIcon};
use beatcheck_core::model::MarkKind;
use egui::{
    Align, Color32, CornerRadius, Event, Frame, InputState, Key, Layout, Margin, RichText,
    ScrollArea, Stroke, Ui, Vec2,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Logical shortcut categories for grouping on the Keyboard Shortcuts page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutCategory {
    Playback,
    Markers,
    Editing,
    Workspace,
}

impl ShortcutCategory {
    pub const ALL: [ShortcutCategory; 4] = [
        ShortcutCategory::Playback,
        ShortcutCategory::Markers,
        ShortcutCategory::Editing,
        ShortcutCategory::Workspace,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            ShortcutCategory::Playback => "PLAYBACK & NAVIGATION",
            ShortcutCategory::Markers => "MARKER PLACEMENT",
            ShortcutCategory::Editing => "MARKER EDITING",
            ShortcutCategory::Workspace => "WORKSPACE & HISTORY",
        }
    }
}

/// Every rebindable keyboard shortcut in BeatCheck.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ShortcutAction {
    PlayPause,
    StepFrameBack,
    StepFrameForward,
    PrevDownbeat,
    NextDownbeat,
    RewindToStart,
    ToggleMetronome,
    ToggleLoop,
    TapTempo,
    AddRegularBeat,
    AddDownbeat,
    AddStrongBeat,
    AddEnergyDrop,
    NudgeMarkBack,
    NudgeMarkForward,
    DeleteSelectedMark,
    SelectAllMarks,
    DeselectMark,
    OpenFile,
    Undo,
    Redo,
    ToggleInspector,
}

impl ShortcutAction {
    pub const ALL: [ShortcutAction; 22] = [
        ShortcutAction::PlayPause,
        ShortcutAction::StepFrameBack,
        ShortcutAction::StepFrameForward,
        ShortcutAction::PrevDownbeat,
        ShortcutAction::NextDownbeat,
        ShortcutAction::RewindToStart,
        ShortcutAction::ToggleMetronome,
        ShortcutAction::ToggleLoop,
        ShortcutAction::TapTempo,
        ShortcutAction::AddRegularBeat,
        ShortcutAction::AddDownbeat,
        ShortcutAction::AddStrongBeat,
        ShortcutAction::AddEnergyDrop,
        ShortcutAction::NudgeMarkBack,
        ShortcutAction::NudgeMarkForward,
        ShortcutAction::DeleteSelectedMark,
        ShortcutAction::SelectAllMarks,
        ShortcutAction::DeselectMark,
        ShortcutAction::OpenFile,
        ShortcutAction::Undo,
        ShortcutAction::Redo,
        ShortcutAction::ToggleInspector,
    ];

    pub fn category(&self) -> ShortcutCategory {
        match self {
            ShortcutAction::PlayPause
            | ShortcutAction::StepFrameBack
            | ShortcutAction::StepFrameForward
            | ShortcutAction::PrevDownbeat
            | ShortcutAction::NextDownbeat
            | ShortcutAction::RewindToStart
            | ShortcutAction::ToggleMetronome
            | ShortcutAction::ToggleLoop
            | ShortcutAction::TapTempo => ShortcutCategory::Playback,

            ShortcutAction::AddRegularBeat
            | ShortcutAction::AddDownbeat
            | ShortcutAction::AddStrongBeat
            | ShortcutAction::AddEnergyDrop => ShortcutCategory::Markers,

            ShortcutAction::NudgeMarkBack
            | ShortcutAction::NudgeMarkForward
            | ShortcutAction::DeleteSelectedMark
            | ShortcutAction::SelectAllMarks
            | ShortcutAction::DeselectMark => ShortcutCategory::Editing,

            ShortcutAction::OpenFile
            | ShortcutAction::Undo
            | ShortcutAction::Redo
            | ShortcutAction::ToggleInspector => ShortcutCategory::Workspace,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ShortcutAction::PlayPause => "Play / Pause",
            ShortcutAction::StepFrameBack => "Step Playhead -1 Frame",
            ShortcutAction::StepFrameForward => "Step Playhead +1 Frame",
            ShortcutAction::PrevDownbeat => "Previous Beat 1 (Downbeat)",
            ShortcutAction::NextDownbeat => "Next Beat 1 (Downbeat)",
            ShortcutAction::RewindToStart => "Rewind to Start",
            ShortcutAction::ToggleMetronome => "Toggle Metronome Click",
            ShortcutAction::ToggleLoop => "Toggle In / Out Loop",
            ShortcutAction::TapTempo => "Tap Tempo",
            ShortcutAction::AddRegularBeat => "Add Regular Beat (Blue)",
            ShortcutAction::AddDownbeat => "Add Downbeat / Beat 1 (Red)",
            ShortcutAction::AddStrongBeat => "Add Strong Beat (Yellow)",
            ShortcutAction::AddEnergyDrop => "Add Energy Drop (Green)",
            ShortcutAction::NudgeMarkBack => "Nudge Selected Marker(s) -1 Frame",
            ShortcutAction::NudgeMarkForward => "Nudge Selected Marker(s) +1 Frame",
            ShortcutAction::DeleteSelectedMark => "Delete Selected Marker(s)",
            ShortcutAction::SelectAllMarks => "Select All Visible Markers",
            ShortcutAction::DeselectMark => "Deselect Marker(s)",
            ShortcutAction::OpenFile => "Open Audio File",
            ShortcutAction::Undo => "Undo",
            ShortcutAction::Redo => "Redo",
            ShortcutAction::ToggleInspector => "Show / Hide Timeline Inspector",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            ShortcutAction::PlayPause => "Start or pause music playback",
            ShortcutAction::StepFrameBack => "Move playhead cursor backward by 1 exact timeline frame",
            ShortcutAction::StepFrameForward => "Move playhead cursor forward by 1 exact timeline frame",
            ShortcutAction::PrevDownbeat => "Jump playhead to the previous musical bar start",
            ShortcutAction::NextDownbeat => "Jump playhead to the next musical bar start",
            ShortcutAction::RewindToStart => "Seek playhead to 00:00",
            ShortcutAction::ToggleMetronome => "Turn audible beat click on or off",
            ShortcutAction::ToggleLoop => "Loop playback between In and Out brackets",
            ShortcutAction::TapTempo => "Tap 3+ times with the music to lock BPM",
            ShortcutAction::AddRegularBeat => "Drop a Blue marker at the playhead while playing or paused",
            ShortcutAction::AddDownbeat => "Drop a Red Beat 1 marker at the playhead",
            ShortcutAction::AddStrongBeat => "Drop a Yellow Strong Beat marker at the playhead",
            ShortcutAction::AddEnergyDrop => "Drop a Green Energy Drop marker at the playhead",
            ShortcutAction::NudgeMarkBack => "Move the selected marker(s) 1 frame earlier",
            ShortcutAction::NudgeMarkForward => "Move the selected marker(s) 1 frame later",
            ShortcutAction::DeleteSelectedMark => "Remove all currently selected markers",
            ShortcutAction::SelectAllMarks => "Select every currently visible marker on the timeline",
            ShortcutAction::DeselectMark => "Clear marker selection",
            ShortcutAction::OpenFile => "Browse for an audio track to analyze",
            ShortcutAction::Undo => "Undo the last beat or grid edit",
            ShortcutAction::Redo => "Redo a previously undone edit",
            ShortcutAction::ToggleInspector => "Open or close the right-hand Timeline Inspector",
        }
    }

    pub fn for_mark_kind(kind: MarkKind) -> Self {
        match kind {
            MarkKind::Downbeat => ShortcutAction::AddDownbeat,
            MarkKind::StrongBeat => ShortcutAction::AddStrongBeat,
            MarkKind::RegularBeat => ShortcutAction::AddRegularBeat,
            MarkKind::EnergyDrop => ShortcutAction::AddEnergyDrop,
        }
    }

    pub fn default_binding(&self) -> ActionBinding {
        match self {
            ShortcutAction::PlayPause => ActionBinding::single(KeyChord::plain(Key::Space)),
            ShortcutAction::StepFrameBack => ActionBinding::single(KeyChord::plain(Key::ArrowLeft)),
            ShortcutAction::StepFrameForward => {
                ActionBinding::single(KeyChord::plain(Key::ArrowRight))
            }
            ShortcutAction::PrevDownbeat => ActionBinding::pair(
                KeyChord::shift(Key::ArrowLeft),
                KeyChord::plain(Key::ArrowUp),
            ),
            ShortcutAction::NextDownbeat => ActionBinding::pair(
                KeyChord::shift(Key::ArrowRight),
                KeyChord::plain(Key::ArrowDown),
            ),
            ShortcutAction::RewindToStart => ActionBinding::single(KeyChord::plain(Key::Home)),
            ShortcutAction::ToggleMetronome => ActionBinding::single(KeyChord::plain(Key::C)),
            ShortcutAction::ToggleLoop => ActionBinding::single(KeyChord::plain(Key::L)),
            ShortcutAction::TapTempo => ActionBinding::single(KeyChord::plain(Key::T)),
            ShortcutAction::AddRegularBeat => {
                ActionBinding::pair(KeyChord::plain(Key::M), KeyChord::plain(Key::Num3))
            }
            ShortcutAction::AddDownbeat => ActionBinding::single(KeyChord::plain(Key::Num1)),
            ShortcutAction::AddStrongBeat => ActionBinding::single(KeyChord::plain(Key::Num2)),
            ShortcutAction::AddEnergyDrop => ActionBinding::single(KeyChord::plain(Key::Num4)),
            ShortcutAction::NudgeMarkBack => ActionBinding::pair(
                KeyChord::plain(Key::Comma),
                KeyChord::plain(Key::OpenBracket),
            ),
            ShortcutAction::NudgeMarkForward => ActionBinding::pair(
                KeyChord::plain(Key::Period),
                KeyChord::plain(Key::CloseBracket),
            ),
            ShortcutAction::DeleteSelectedMark => ActionBinding::pair(
                KeyChord::plain(Key::Delete),
                KeyChord::plain(Key::Backspace),
            ),
            ShortcutAction::SelectAllMarks => ActionBinding::single(KeyChord::cmd(Key::A)),
            ShortcutAction::DeselectMark => ActionBinding::single(KeyChord::plain(Key::Escape)),
            ShortcutAction::OpenFile => ActionBinding::single(KeyChord::cmd(Key::O)),
            ShortcutAction::Undo => ActionBinding::single(KeyChord::cmd(Key::Z)),
            ShortcutAction::Redo => {
                #[cfg(target_os = "macos")]
                {
                    ActionBinding::pair(KeyChord::cmd_shift(Key::Z), KeyChord::cmd(Key::Y))
                }
                #[cfg(not(target_os = "macos"))]
                {
                    ActionBinding::pair(KeyChord::cmd(Key::Y), KeyChord::cmd_shift(Key::Z))
                }
            }
            ShortcutAction::ToggleInspector => ActionBinding::single(KeyChord::plain(Key::E)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingSlot {
    Primary,
    Secondary,
}

/// A single key + modifier combination (e.g. `M`, `Shift+←`, `Ctrl+Z`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub key: Key,
    pub cmd: bool,
    pub shift: bool,
    pub alt: bool,
}

impl KeyChord {
    pub const fn new(key: Key, cmd: bool, shift: bool, alt: bool) -> Self {
        Self {
            key,
            cmd,
            shift,
            alt,
        }
    }

    pub const fn plain(key: Key) -> Self {
        Self::new(key, false, false, false)
    }

    pub const fn shift(key: Key) -> Self {
        Self::new(key, false, true, false)
    }

    pub const fn cmd(key: Key) -> Self {
        Self::new(key, true, false, false)
    }

    pub const fn cmd_shift(key: Key) -> Self {
        Self::new(key, true, true, false)
    }

    pub fn matches(&self, i: &InputState) -> bool {
        if !i.key_pressed(self.key) {
            return false;
        }
        let cmd = i.modifiers.ctrl || i.modifiers.command;
        self.cmd == cmd && self.shift == i.modifiers.shift && self.alt == i.modifiers.alt
    }

    /// Human-friendly display label for UI badges & tooltips.
    pub fn display_label(&self) -> String {
        let mut parts: Vec<&str> = Vec::with_capacity(4);
        if self.cmd {
            parts.push(brand::MOD_KEY);
        }
        if self.alt {
            #[cfg(target_os = "macos")]
            parts.push("Opt");
            #[cfg(not(target_os = "macos"))]
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        parts.push(pretty_key_name(self.key));
        parts.join("+")
    }

    /// Stable portable config string (e.g. `"Shift+ArrowLeft"`, `"Ctrl+Z"`, `"M"`).
    pub fn to_config_string(&self) -> String {
        let mut parts: Vec<&str> = Vec::with_capacity(4);
        if self.cmd {
            parts.push("Cmd");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        parts.push(self.key.name());
        parts.join("+")
    }

    /// Parse a portable config string back into a `KeyChord`.
    pub fn parse_config_string(raw: &str) -> Option<Self> {
        let mut cmd = false;
        let mut shift = false;
        let mut alt = false;
        let mut key_token: Option<&str> = None;

        for part in raw.split('+').map(str::trim).filter(|s| !s.is_empty()) {
            match part.to_ascii_lowercase().as_str() {
                "cmd" | "ctrl" | "control" | "command" => cmd = true,
                "shift" => shift = true,
                "alt" | "opt" | "option" => alt = true,
                _ => key_token = Some(part),
            }
        }

        let token = key_token?;
        let key = Key::from_name(token)?;
        Some(Self {
            key,
            cmd,
            shift,
            alt,
        })
    }
}

fn pretty_key_name(key: Key) -> &'static str {
    match key {
        Key::ArrowLeft => "←",
        Key::ArrowRight => "→",
        Key::ArrowUp => "↑",
        Key::ArrowDown => "↓",
        Key::Escape => "Esc",
        Key::Comma => ",",
        Key::Period => ".",
        Key::OpenBracket => "[",
        Key::CloseBracket => "]",
        Key::Minus => "-",
        Key::Plus => "+",
        Key::Equals => "=",
        Key::Semicolon => ";",
        Key::Slash => "/",
        Key::Backslash => "\\",
        Key::Backtick => "`",
        Key::Quote => "'",
        Key::Colon => ":",
        Key::Questionmark => "?",
        Key::Pipe => "|",
        Key::OpenCurlyBracket => "{",
        Key::CloseCurlyBracket => "}",
        Key::Num0 => "0",
        Key::Num1 => "1",
        Key::Num2 => "2",
        Key::Num3 => "3",
        Key::Num4 => "4",
        Key::Num5 => "5",
        Key::Num6 => "6",
        Key::Num7 => "7",
        Key::Num8 => "8",
        Key::Num9 => "9",
        other => other.name(),
    }
}

impl Serialize for KeyChord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_config_string())
    }
}

impl<'de> Deserialize<'de> for KeyChord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        KeyChord::parse_config_string(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("Unknown key chord '{s}'")))
    }
}

/// Primary + optional secondary (alternate) binding for an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ActionBinding {
    #[serde(default)]
    pub primary: Option<KeyChord>,
    #[serde(default)]
    pub secondary: Option<KeyChord>,
}

impl ActionBinding {
    pub const fn single(primary: KeyChord) -> Self {
        Self {
            primary: Some(primary),
            secondary: None,
        }
    }

    pub const fn pair(primary: KeyChord, secondary: KeyChord) -> Self {
        Self {
            primary: Some(primary),
            secondary: Some(secondary),
        }
    }

    pub fn get(&self, slot: BindingSlot) -> Option<KeyChord> {
        match slot {
            BindingSlot::Primary => self.primary,
            BindingSlot::Secondary => self.secondary,
        }
    }

    pub fn set(&mut self, slot: BindingSlot, chord: Option<KeyChord>) {
        match slot {
            BindingSlot::Primary => self.primary = chord,
            BindingSlot::Secondary => self.secondary = chord,
        }
    }

    pub fn matches(&self, i: &InputState) -> bool {
        self.primary.is_some_and(|c| c.matches(i)) || self.secondary.is_some_and(|c| c.matches(i))
    }

    /// Compact label combining primary and secondary (e.g. `"M / 3"` or `"Shift+← / ↑"`).
    pub fn display_label(&self) -> String {
        match (self.primary, self.secondary) {
            (Some(p), Some(s)) => format!("{} / {}", p.display_label(), s.display_label()),
            (Some(p), None) => p.display_label(),
            (None, Some(s)) => s.display_label(),
            (None, None) => "Unbound".to_string(),
        }
    }

    /// Primary-only compact label (falls back to secondary if primary is cleared).
    pub fn short_label(&self) -> String {
        if let Some(p) = self.primary {
            p.display_label()
        } else if let Some(s) = self.secondary {
            s.display_label()
        } else {
            "—".to_string()
        }
    }
}

/// Full user-customizable keyboard shortcut map (serialized into `beatcheck.toml`).
/// Missing or corrupt individual entries automatically fall back to defaults so one bad
/// line in a user-edited config never wipes out all other preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeyBindings {
    bindings: BTreeMap<ShortcutAction, ActionBinding>,
}

impl<'de> Deserialize<'de> for KeyBindings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawKeyBindings {
            #[serde(default)]
            bindings: BTreeMap<String, toml::Value>,
        }
        let raw = RawKeyBindings::deserialize(deserializer)?;
        let mut kb = KeyBindings::default();
        for (action_name, val) in raw.bindings {
            let Ok(action) = toml::Value::String(action_name).try_into::<ShortcutAction>() else {
                continue;
            };
            if let Ok(binding) = val.try_into::<ActionBinding>() {
                kb.bindings.insert(action, binding);
            }
        }
        Ok(kb)
    }
}

impl Default for KeyBindings {
    fn default() -> Self {
        let mut bindings = BTreeMap::new();
        for action in ShortcutAction::ALL {
            bindings.insert(action, action.default_binding());
        }
        Self { bindings }
    }
}

impl KeyBindings {
    pub fn get(&self, action: ShortcutAction) -> ActionBinding {
        self.bindings
            .get(&action)
            .copied()
            .unwrap_or_else(|| action.default_binding())
    }

    pub fn is_pressed(&self, action: ShortcutAction, input: &InputState) -> bool {
        self.get(action).matches(input)
    }

    pub fn label(&self, action: ShortcutAction) -> String {
        self.get(action).display_label()
    }

    pub fn short_label(&self, action: ShortcutAction) -> String {
        self.get(action).short_label()
    }

    /// Bind `chord` to `(action, slot)`, automatically clearing any conflicting binding elsewhere.
    /// Returns the list of `(Action, Slot)` entries that were cleared due to conflict.
    pub fn set_binding(
        &mut self,
        action: ShortcutAction,
        slot: BindingSlot,
        chord: KeyChord,
    ) -> Vec<(ShortcutAction, BindingSlot)> {
        let mut cleared = Vec::new();
        for other_action in ShortcutAction::ALL {
            let mut b = self.get(other_action);
            let mut changed = false;
            for other_slot in [BindingSlot::Primary, BindingSlot::Secondary] {
                if other_action == action && other_slot == slot {
                    continue;
                }
                if b.get(other_slot) == Some(chord) {
                    b.set(other_slot, None);
                    cleared.push((other_action, other_slot));
                    changed = true;
                }
            }
            if changed {
                self.bindings.insert(other_action, b);
            }
        }

        let mut target = self.get(action);
        target.set(slot, Some(chord));
        self.bindings.insert(action, target);
        cleared
    }

    pub fn clear_binding(&mut self, action: ShortcutAction, slot: BindingSlot) {
        let mut target = self.get(action);
        target.set(slot, None);
        self.bindings.insert(action, target);
    }

    pub fn reset_action(&mut self, action: ShortcutAction) -> Vec<(ShortcutAction, BindingSlot)> {
        let def = action.default_binding();
        let mut cleared = Vec::new();
        self.bindings.insert(action, ActionBinding::default());
        if let Some(p) = def.primary {
            cleared.extend(self.set_binding(action, BindingSlot::Primary, p));
        }
        if let Some(s) = def.secondary {
            cleared.extend(self.set_binding(action, BindingSlot::Secondary, s));
        }
        cleared
    }

    pub fn reset_all(&mut self) {
        *self = Self::default();
    }

    pub fn is_action_modified(&self, action: ShortcutAction) -> bool {
        self.get(action) != action.default_binding()
    }

    pub fn is_any_modified(&self) -> bool {
        ShortcutAction::ALL
            .iter()
            .any(|&a| self.is_action_modified(a))
    }
}

/// Interactive state for the Keyboard Shortcuts page.
#[derive(Debug, Default)]
pub struct ShortcutsPageState {
    pub search_query: String,
    pub capturing: Option<(ShortcutAction, BindingSlot)>,
    pub notice: Option<(String, bool)>,
    pub dirty: bool,
}

#[derive(Debug, Default)]
pub struct ShortcutsPageActions {
    pub close_page: bool,
    pub save_portable_config: bool,
    pub bindings_changed: bool,
}

/// Full-page Obsidian & Electric Violet Keyboard Shortcuts editor.
pub fn draw_shortcuts_page(
    ui: &mut Ui,
    shortcuts: &mut KeyBindings,
    state: &mut ShortcutsPageState,
    has_track: bool,
    actions: &mut ShortcutsPageActions,
) {
    // 1. Capture live key presses when recording a shortcut slot.
    // Consume captured key events so Space / Enter cannot re-trigger a focused button on the same frame!
    let mut captured_this_frame = false;
    if let Some((cap_action, cap_slot)) = state.capturing {
        if ui.ctx().wants_keyboard_input() {
            state.capturing = None;
        } else {
            let mut captured_event: Option<(Key, egui::Modifiers)> = None;
            ui.input_mut(|i| {
                i.events.retain(|ev| {
                    if let Event::Key {
                        key,
                        pressed: true,
                        repeat: false,
                        modifiers,
                        ..
                    } = ev
                    {
                        if captured_event.is_none() {
                            captured_event = Some((*key, *modifiers));
                            return false;
                        }
                    }
                    true
                });
            });

            if let Some((key, mods)) = captured_event {
                captured_this_frame = true;
                let cmd = mods.ctrl || mods.command;
                let shift = mods.shift;
                let alt = mods.alt;
                if key == Key::Escape && !cmd && !shift && !alt {
                    state.capturing = None;
                    state.notice = Some(("Shortcut capture cancelled".to_string(), false));
                } else {
                    let chord = KeyChord::new(key, cmd, shift, alt);
                    let conflicts = shortcuts.set_binding(cap_action, cap_slot, chord);
                    state.capturing = None;
                    state.dirty = true;
                    actions.bindings_changed = true;
                    if conflicts.is_empty() {
                        state.notice = Some((
                            format!("Bound {} -> {}", chord.display_label(), cap_action.label()),
                            false,
                        ));
                    } else {
                        let cleared_names: Vec<&str> =
                            conflicts.iter().map(|(a, _)| a.label()).collect();
                        state.notice = Some((
                            format!(
                                "Bound {} -> {} (reassigned from {})",
                                chord.display_label(),
                                cap_action.label(),
                                cleared_names.join(", ")
                            ),
                            true,
                        ));
                    }
                }
            }
        }
    } else if !ui.ctx().wants_keyboard_input()
        && ui.input(|i| i.key_pressed(Key::Escape))
    {
        actions.close_page = true;
    }

    let avail_w = ui.available_width();
    let max_w = (avail_w - 28.0).min(960.0).max(460.0);
    let side_pad = ((avail_w - max_w) * 0.5).max(6.0);

    // Pinned Header (outside ScrollArea so it stays visible while scrolling)
    ui.horizontal(|ui| {
        ui.add_space(side_pad);
        ui.vertical(|ui| {
            ui.set_width(max_w);

            Frame::new()
                .fill(brand::PANEL)
                .stroke(Stroke::new(1.0, brand::BORDER))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    ui.set_width(max_w - 32.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(10.0, 6.0);

                        let back_label = if has_track {
                            "Back to Timeline"
                        } else {
                            "Back"
                        };
                        if brand::stroke_button(
                            ui,
                            Some(StrokeIcon::PrevBar),
                            back_label,
                            false,
                            28.0,
                        )
                        .on_hover_text("Return to main workspace (Esc)")
                        .clicked()
                        {
                            state.capturing = None;
                            actions.close_page = true;
                        }

                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            ui.label(
                                RichText::new("Keyboard Shortcuts")
                                    .font(brand::font_display_bold(17.0))
                                    .color(brand::TEXT),
                            );
                            ui.label(
                                RichText::new(
                                    "Click any key slot to rebind. Press Esc while recording to cancel.",
                                )
                                .size(11.5)
                                .color(brand::MUTED),
                            );
                        });

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;

                            if state.dirty {
                                if brand::primary_cta_button(
                                    ui,
                                    Some(StrokeIcon::Save),
                                    "Save Shortcuts",
                                    Vec2::new(138.0, 28.0),
                                )
                                .on_hover_text("Save keyboard shortcuts to beatcheck.toml")
                                .clicked()
                                {
                                    state.capturing = None;
                                    actions.save_portable_config = true;
                                }
                            } else if brand::stroke_button(
                                ui,
                                Some(StrokeIcon::Save),
                                "Save to File",
                                false,
                                28.0,
                            )
                            .on_hover_text("Save keyboard shortcuts & settings to beatcheck.toml")
                            .clicked()
                            {
                                state.capturing = None;
                                actions.save_portable_config = true;
                            }

                            ui.add_enabled_ui(shortcuts.is_any_modified(), |ui| {
                                if brand::stroke_button(
                                    ui,
                                    Some(StrokeIcon::Reset),
                                    "Reset All Defaults",
                                    false,
                                    28.0,
                                )
                                .on_hover_text("Restore every shortcut back to its built-in default")
                                .clicked()
                                {
                                    state.capturing = None;
                                    shortcuts.reset_all();
                                    state.dirty = true;
                                    actions.bindings_changed = true;
                                    state.notice = Some((
                                        "Restored all shortcuts to default bindings".to_string(),
                                        false,
                                    ));
                                }
                            });
                        });
                    });

                    ui.add_space(10.0);

                    // Search bar + live recording / notice banner
                    ui.horizontal_wrapped(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut state.search_query)
                                .desired_width(250.0)
                                .hint_text("Filter shortcuts (e.g. marker, frame)..."),
                        );
                        if !state.search_query.is_empty() && ui.small_button("Clear").clicked() {
                            state.search_query.clear();
                        }

                        ui.add_space(8.0);

                        if let Some((cap_action, cap_slot)) = state.capturing {
                            let slot_lbl = match cap_slot {
                                BindingSlot::Primary => "Primary",
                                BindingSlot::Secondary => "Alt",
                            };
                            Frame::new()
                                .fill(brand::primary_tint(46))
                                .stroke(Stroke::new(1.0, brand::PRIMARY))
                                .corner_radius(CornerRadius::same(7))
                                .inner_margin(Margin::symmetric(10, 4))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "Recording {slot_lbl} key for \"{}\" — press any key (Esc to cancel)",
                                            cap_action.label()
                                        ))
                                        .font(brand::font_ui_semibold(11.5))
                                        .color(brand::LAVENDER),
                                    );
                                });
                        } else if let Some((msg, is_warn)) = &state.notice {
                            let col = if *is_warn {
                                Color32::from_rgb(250, 204, 21)
                            } else {
                                brand::SUCCESS
                            };
                            ui.label(
                                RichText::new(msg)
                                    .font(brand::font_mono_bold(11.0))
                                    .color(col),
                            );
                        }
                    });
                });
        });
    });

    ui.add_space(10.0);

    // Scrollable category cards — placed directly in the vertical CentralPanel so it fills 100% of remaining height!
    let query = state.search_query.trim().to_ascii_lowercase();
    ScrollArea::vertical()
        .id_salt("shortcuts_page_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let scroll_w = ui.available_width();
            let list_pad = ((scroll_w - max_w) * 0.5).max(6.0);
            ui.horizontal(|ui| {
                ui.add_space(list_pad);
                ui.vertical(|ui| {
                    ui.set_width(max_w);
                    let mut total_shown = 0usize;

                    for category in ShortcutCategory::ALL {
                        let matching: Vec<ShortcutAction> = ShortcutAction::ALL
                            .into_iter()
                            .filter(|a| a.category() == category)
                            .filter(|a| {
                                if query.is_empty() {
                                    return true;
                                }
                                let b = shortcuts.get(*a);
                                a.label().to_ascii_lowercase().contains(&query)
                                    || a.description().to_ascii_lowercase().contains(&query)
                                    || b.display_label().to_ascii_lowercase().contains(&query)
                            })
                            .collect();

                        if matching.is_empty() {
                            continue;
                        }
                        total_shown += matching.len();

                        ui.label(
                            RichText::new(category.title())
                                .font(brand::font_mono_bold(10.5))
                                .color(brand::HIGHLIGHT),
                        );
                        ui.add_space(4.0);

                        Frame::new()
                            .fill(brand::PANEL)
                            .stroke(Stroke::new(1.0, brand::BORDER))
                            .corner_radius(CornerRadius::same(12))
                            .inner_margin(Margin::symmetric(14, 10))
                            .show(ui, |ui| {
                                let inner_w = max_w - 28.0;
                                let slot_w = if inner_w < 680.0 { 126.0 } else { 156.0 };
                                ui.set_width(inner_w);

                                // Column header row
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("Action")
                                            .font(brand::font_mono_bold(10.0))
                                            .color(brand::MUTED),
                                    );
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.add_space(34.0); // space for Reset icon button
                                        ui.allocate_ui_with_layout(
                                            Vec2::new(slot_w, 16.0),
                                            Layout::centered_and_justified(
                                                egui::Direction::LeftToRight,
                                            ),
                                            |ui| {
                                                ui.label(
                                                    RichText::new("Alt Key")
                                                        .font(brand::font_mono_bold(10.0))
                                                        .color(brand::MUTED),
                                                );
                                            },
                                        );
                                        ui.add_space(8.0);
                                        ui.allocate_ui_with_layout(
                                            Vec2::new(slot_w, 16.0),
                                            Layout::centered_and_justified(
                                                egui::Direction::LeftToRight,
                                            ),
                                            |ui| {
                                                ui.label(
                                                    RichText::new("Primary Key")
                                                        .font(brand::font_mono_bold(10.0))
                                                        .color(brand::MUTED),
                                                );
                                            },
                                        );
                                    });
                                });
                                ui.separator();

                                for (idx, action) in matching.iter().enumerate() {
                                    draw_shortcut_editor_row(
                                        ui,
                                        *action,
                                        inner_w,
                                        slot_w,
                                        captured_this_frame,
                                        shortcuts,
                                        state,
                                        actions,
                                    );
                                    if idx + 1 < matching.len() {
                                        ui.add_space(3.0);
                                        let (r, _) = ui.allocate_exact_size(
                                            Vec2::new(ui.available_width(), 1.0),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().line_segment(
                                            [
                                                egui::Pos2::new(r.left(), r.center().y),
                                                egui::Pos2::new(r.right(), r.center().y),
                                            ],
                                            Stroke::new(
                                                1.0,
                                                Color32::from_rgba_unmultiplied(45, 42, 66, 120),
                                            ),
                                        );
                                        ui.add_space(3.0);
                                    }
                                }
                            });

                        ui.add_space(12.0);
                    }

                    if total_shown == 0 {
                        ui.add_space(24.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new("No shortcuts match your filter.")
                                    .size(13.0)
                                    .color(brand::MUTED),
                            );
                        });
                    }
                    ui.add_space(16.0);
                });
            });
        });
}

#[allow(clippy::too_many_arguments)]
fn draw_shortcut_editor_row(
    ui: &mut Ui,
    action: ShortcutAction,
    row_w: f32,
    slot_w: f32,
    captured_this_frame: bool,
    shortcuts: &mut KeyBindings,
    state: &mut ShortcutsPageState,
    actions: &mut ShortcutsPageActions,
) {
    let binding = shortcuts.get(action);
    let is_modified = shortcuts.is_action_modified(action);
    let right_controls_w = slot_w * 2.0 + 54.0;
    let left_max_w = (row_w - right_controls_w).max(140.0);

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_max_width(left_max_w);
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(
                    RichText::new(action.label())
                        .font(brand::font_ui_semibold(12.5))
                        .color(brand::TEXT),
                );
                if is_modified {
                    Frame::new()
                        .fill(brand::primary_tint(40))
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(Margin::symmetric(5, 1))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("Custom")
                                    .font(brand::font_mono_bold(9.5))
                                    .color(brand::HIGHLIGHT),
                            );
                        });
                }
            });
            ui.add(
                egui::Label::new(
                    RichText::new(action.description())
                        .size(11.0)
                        .color(brand::MUTED),
                )
                .wrap(),
            );
        });

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;

            // Per-row Reset button
            ui.add_enabled_ui(is_modified, |ui| {
                if brand::stroke_button(ui, Some(StrokeIcon::Reset), "", false, 26.0)
                    .on_hover_text(format!(
                        "Reset to default ({})",
                        action.default_binding().display_label()
                    ))
                    .clicked()
                {
                    state.capturing = None;
                    let conflicts = shortcuts.reset_action(action);
                    state.dirty = true;
                    actions.bindings_changed = true;
                    if conflicts.is_empty() {
                        state.notice = Some((
                            format!("Reset \"{}\" to default", action.label()),
                            false,
                        ));
                    } else {
                        let cleared_names: Vec<&str> =
                            conflicts.iter().map(|(a, _)| a.label()).collect();
                        state.notice = Some((
                            format!(
                                "Reset \"{}\" to default (cleared conflict from {})",
                                action.label(),
                                cleared_names.join(", ")
                            ),
                            true,
                        ));
                    }
                }
            });

            // Secondary (Alt) key slot
            draw_key_slot_control(
                ui,
                action,
                BindingSlot::Secondary,
                binding.secondary,
                slot_w,
                captured_this_frame,
                shortcuts,
                state,
                actions,
            );

            ui.add_space(2.0);

            // Primary key slot
            draw_key_slot_control(
                ui,
                action,
                BindingSlot::Primary,
                binding.primary,
                slot_w,
                captured_this_frame,
                shortcuts,
                state,
                actions,
            );
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn draw_key_slot_control(
    ui: &mut Ui,
    action: ShortcutAction,
    slot: BindingSlot,
    chord: Option<KeyChord>,
    slot_w: f32,
    captured_this_frame: bool,
    shortcuts: &mut KeyBindings,
    state: &mut ShortcutsPageState,
    actions: &mut ShortcutsPageActions,
) {
    let is_recording = state.capturing == Some((action, slot));

    ui.allocate_ui_with_layout(
        Vec2::new(slot_w, 28.0),
        Layout::right_to_left(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 3.0;

            // Small clear button when bound (and not currently recording)
            if chord.is_some() && !is_recording {
                let clr = egui::Button::new(
                    RichText::new("×")
                        .font(brand::font_mono_bold(12.0))
                        .color(brand::MUTED),
                )
                .fill(brand::SURFACE)
                .stroke(Stroke::new(1.0, brand::BORDER))
                .corner_radius(CornerRadius::same(6));
                if ui
                    .add_sized([22.0, 26.0], clr)
                    .on_hover_text("Clear this key binding")
                    .clicked()
                {
                    shortcuts.clear_binding(action, slot);
                    state.dirty = true;
                    actions.bindings_changed = true;
                    state.notice = Some((format!("Cleared key for \"{}\"", action.label()), false));
                }
            }

            let btn_w = ui.available_width().max(80.0);
            let (label, fill, stroke, text_col) = if is_recording {
                (
                    "Press key...".to_string(),
                    brand::PRIMARY,
                    Stroke::new(1.2, brand::LAVENDER),
                    brand::TEXT,
                )
            } else if let Some(c) = chord {
                (
                    c.display_label(),
                    brand::SURFACE,
                    Stroke::new(1.0, brand::BORDER),
                    brand::LAVENDER,
                )
            } else {
                (
                    "+ Set key".to_string(),
                    brand::CANVAS,
                    Stroke::new(1.0, Color32::from_rgb(38, 35, 56)),
                    Color32::from_rgb(112, 108, 136),
                )
            };

            let btn = egui::Button::new(
                RichText::new(label)
                    .font(brand::font_mono_bold(11.5))
                    .color(text_col),
            )
            .fill(fill)
            .stroke(stroke)
            .corner_radius(CornerRadius::same(7));

            let hover = if is_recording {
                "Press any key combination to bind, or Esc to cancel"
            } else {
                "Click to record a new key binding"
            };

            let resp = ui.add_sized([btn_w, 26.0], btn).on_hover_text(hover);
            if resp.clicked() && !captured_this_frame {
                ui.memory_mut(|m| m.surrender_focus(resp.id));
                if is_recording {
                    state.capturing = None;
                } else {
                    state.capturing = Some((action, slot));
                }
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_bindings_toml_roundtrip_preserves_all_defaults() {
        let original = KeyBindings::default();
        let text = toml::to_string_pretty(&original).expect("serialize KeyBindings");
        let parsed: KeyBindings = toml::from_str(&text).expect("deserialize KeyBindings");
        assert_eq!(original, parsed);
        assert!(!parsed.is_any_modified());
    }

    #[test]
    fn key_chord_config_string_roundtrip() {
        let samples = [
            KeyChord::plain(Key::Space),
            KeyChord::plain(Key::M),
            KeyChord::plain(Key::Num1),
            KeyChord::plain(Key::ArrowLeft),
            KeyChord::shift(Key::ArrowRight),
            KeyChord::cmd(Key::Z),
            KeyChord::cmd_shift(Key::Z),
            KeyChord::new(Key::F5, true, false, true),
            KeyChord::plain(Key::Comma),
            KeyChord::plain(Key::OpenBracket),
            KeyChord::plain(Key::Minus),
            KeyChord::plain(Key::Slash),
            KeyChord::plain(Key::Delete),
            KeyChord::plain(Key::Backspace),
            KeyChord::plain(Key::Escape),
        ];
        for chord in samples {
            let encoded = chord.to_config_string();
            let decoded = KeyChord::parse_config_string(&encoded)
                .unwrap_or_else(|| panic!("failed to parse '{encoded}'"));
            assert_eq!(chord, decoded);
        }
    }

    #[test]
    fn reset_action_clears_conflicts_and_lenient_toml_skips_invalid_entries() {
        let mut kb = KeyBindings::default();
        // Move M to ToggleMetronome, then reset AddRegularBeat back to M -> ToggleMetronome loses M
        kb.set_binding(
            ShortcutAction::ToggleMetronome,
            BindingSlot::Primary,
            KeyChord::plain(Key::M),
        );
        let cleared = kb.reset_action(ShortcutAction::AddRegularBeat);
        assert_eq!(
            cleared,
            vec![(ShortcutAction::ToggleMetronome, BindingSlot::Primary)]
        );
        assert_eq!(
            kb.get(ShortcutAction::AddRegularBeat).primary,
            Some(KeyChord::plain(Key::M))
        );
        assert_eq!(kb.get(ShortcutAction::ToggleMetronome).primary, None);

        // Lenient TOML deserialization ignores unknown/malformed entries without wiping valid ones
        let raw = r#"
[bindings.ToggleMetronome]
primary = "K"

[bindings.NotARealAction]
primary = "Z"

[bindings.PlayPause]
primary = "TotallyInvalidKeyName"
"#;
        let parsed: KeyBindings = toml::from_str(raw).expect("lenient deserialize");
        assert_eq!(
            parsed.get(ShortcutAction::ToggleMetronome).primary,
            Some(KeyChord::plain(Key::K))
        );
        // Invalid PlayPause fell back cleanly to Space
        assert_eq!(
            parsed.get(ShortcutAction::PlayPause).primary,
            Some(KeyChord::plain(Key::Space))
        );
    }

    #[test]
    fn rebinding_resolves_conflicts_and_reset_restores_defaults() {
        let mut kb = KeyBindings::default();
        // M is initially bound to AddRegularBeat; rebind M to ToggleMetronome
        let cleared = kb.set_binding(
            ShortcutAction::ToggleMetronome,
            BindingSlot::Primary,
            KeyChord::plain(Key::M),
        );
        assert_eq!(
            cleared,
            vec![(ShortcutAction::AddRegularBeat, BindingSlot::Primary)]
        );
        assert_eq!(
            kb.get(ShortcutAction::ToggleMetronome).primary,
            Some(KeyChord::plain(Key::M))
        );
        // AddRegularBeat lost Primary (M) but kept Secondary (3)
        assert_eq!(kb.get(ShortcutAction::AddRegularBeat).primary, None);
        assert_eq!(
            kb.get(ShortcutAction::AddRegularBeat).secondary,
            Some(KeyChord::plain(Key::Num3))
        );
        assert!(kb.is_action_modified(ShortcutAction::ToggleMetronome));
        assert!(kb.is_any_modified());

        kb.reset_all();
        assert!(!kb.is_any_modified());
        assert_eq!(
            kb.get(ShortcutAction::AddRegularBeat).primary,
            Some(KeyChord::plain(Key::M))
        );
        assert_eq!(
            kb.get(ShortcutAction::ToggleMetronome).primary,
            Some(KeyChord::plain(Key::C))
        );
    }
}
