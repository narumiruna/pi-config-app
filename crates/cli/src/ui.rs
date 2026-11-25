use std::io;
use std::path::PathBuf;

use rune::{Cx, El, LiveRegion, ScrollSpec, TextEdit, TextStyle, col, row, text};
use rune_kit::{
    Button, ButtonVariant, ControlSize, Dialog, Kit, Select, SelectAppearance, TextInput,
};
use serde_json::Value;

use crate::settings::{Changes, SaveError, Settings, settings_path};

const DEFAULT: &str = "Use Pi default";
const UNSUPPORTED: &str = "Unsupported value";
const THEME: &str = "theme";
const THINKING: &str = "defaultThinkingLevel";
const HIDE: &str = "hideThinkingBlock";
const LEVELS: &[&str] = &[
    DEFAULT, "off", "minimal", "low", "medium", "high", "xhigh", "max",
];
const VISIBILITY: &[&str] = &[DEFAULT, "Show", "Hide"];

#[derive(Default)]
enum Field<T> {
    #[default]
    Unset,
    Value(T),
    Unsupported,
}

impl<T> Field<T> {
    fn read(settings: &Settings, key: &str, decode: impl FnOnce(Value) -> Option<T>) -> Self {
        match settings.raw(key) {
            None => Self::Unset,
            Some(raw) => serde_json::from_str(raw)
                .ok()
                .and_then(decode)
                .map_or(Self::Unsupported, Self::Value),
        }
    }

    fn value(&self) -> Option<&T> {
        match self {
            Self::Value(value) => Some(value),
            _ => None,
        }
    }

    fn supported(&self) -> bool {
        !matches!(self, Self::Unsupported)
    }
}

#[derive(Default)]
struct Fields {
    theme: Field<String>,
    thinking: Field<usize>,
    hide: Field<usize>,
}

impl Fields {
    fn read(settings: &Settings) -> Self {
        Self {
            theme: Field::read(settings, THEME, |value| match value {
                Value::String(name) if !name.is_empty() && !name.contains(['\r', '\n']) => {
                    Some(name)
                }
                _ => None,
            }),
            thinking: Field::read(settings, THINKING, |value| {
                LEVELS
                    .iter()
                    .enumerate()
                    .skip(1)
                    .find_map(|(i, &name)| (value.as_str() == Some(name)).then_some(i))
            }),
            hide: Field::read(settings, HIDE, |value| {
                value.as_bool().map(|hide| if hide { 2 } else { 1 })
            }),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    Thinking,
    Hide,
}

pub struct Editor {
    path: Option<PathBuf>,
    settings: io::Result<Settings>,
    initial: Fields,
    theme: TextEdit,
    thinking: usize,
    hide: usize,
    menu: Option<Menu>,
    confirm_reload: bool,
    notice: Option<(bool, String)>,
}

impl Editor {
    pub fn new() -> Self {
        let (path, settings) = match settings_path() {
            Ok(path) => (Some(path), Err(io::Error::other("Not loaded"))),
            Err(error) => (None, Err(error)),
        };
        let mut editor = Self {
            path,
            settings,
            initial: Fields::default(),
            theme: TextEdit::default(),
            thinking: 0,
            hide: 0,
            menu: None,
            confirm_reload: false,
            notice: None,
        };
        editor.reload();
        editor
    }

    fn reload(&mut self) {
        self.confirm_reload = false;
        self.menu = None;
        self.notice = None;
        if let Some(path) = &self.path {
            self.settings = Settings::load(path);
        }
        self.initial = self.settings.as_ref().map(Fields::read).unwrap_or_default();
        self.theme = TextEdit::new(self.initial.theme.value().cloned().unwrap_or_default());
        self.thinking = self.initial.thinking.value().copied().unwrap_or(0);
        self.hide = self.initial.hide.value().copied().unwrap_or(0);
    }

    fn changes(&self) -> Changes {
        let mut changes = Changes::new();
        let theme = self.theme.text.as_str();
        if self.initial.theme.supported()
            && theme != self.initial.theme.value().map_or("", String::as_str)
        {
            changes.insert(THEME, (!theme.is_empty()).then(|| Value::from(theme)));
        }
        if self.initial.thinking.supported()
            && self.thinking != self.initial.thinking.value().copied().unwrap_or(0)
        {
            changes.insert(
                THINKING,
                (self.thinking != 0).then(|| Value::from(LEVELS[self.thinking])),
            );
        }
        if self.initial.hide.supported()
            && self.hide != self.initial.hide.value().copied().unwrap_or(0)
        {
            changes.insert(HIDE, (self.hide != 0).then(|| Value::from(self.hide == 2)));
        }
        changes
    }

    fn save(&mut self) {
        let changes = self.changes();
        let (Some(path), Ok(settings)) = (&self.path, &mut self.settings) else {
            return;
        };
        if changes.is_empty() {
            return;
        }
        self.notice = Some(match settings.save(path, changes) {
            Ok(()) => {
                self.initial = Fields::read(settings);
                (
                    false,
                    "Saved. Run /reload in an open Pi session to reload settings.".into(),
                )
            }
            Err(SaveError::Changed) => (
                true,
                "settings.json changed on disk. Reload before saving.".into(),
            ),
            Err(SaveError::Io(error)) => (true, format!("Could not save settings.json: {error}")),
        });
    }

    fn edit(&mut self) {
        if self.notice.as_ref().is_some_and(|(error, _)| !error) {
            self.notice = None;
        }
    }

    fn status(&self, dirty: bool) -> (bool, String) {
        if let Some(notice) = &self.notice {
            return notice.clone();
        }
        match &self.settings {
            Err(error) => (true, format!("Could not read settings.json: {error}")),
            Ok(settings) if settings.missing() => (
                false,
                "Not created yet. Saving a change will create settings.json.".into(),
            ),
            Ok(_) if dirty => (false, "Unsaved changes".into()),
            Ok(_) => (false, "No unsaved changes".into()),
        }
    }
}

fn field_row(kit: &Kit, label: &'static str, hint: &'static str, control: El) -> El {
    row()
        .w_full()
        .gap(16.0)
        .center_y()
        .py(12.0)
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(4.0)
                .child(text(label).font(kit.ty.ui_medium).color(kit.c.text_strong))
                .child(
                    text(hint)
                        .font(kit.ty.ui_compact)
                        .color(kit.c.text_muted)
                        .wrap()
                        .w_full(),
                ),
        )
        .child(control)
}

fn menu(editor: &Editor, kit: &Kit, layer: &mut Vec<El>, kind: Menu, label: &'static str) -> El {
    let (key, options, index, supported) = match kind {
        Menu::Thinking => (
            "thinking",
            LEVELS,
            editor.thinking,
            editor.initial.thinking.supported(),
        ),
        Menu::Hide => (
            "hide",
            VISIBILITY,
            editor.hide,
            editor.initial.hide.supported(),
        ),
    };
    Select::new(key, options)
        .label(label)
        .current(Some(if supported {
            options[index]
        } else {
            UNSUPPORTED
        }))
        .width(210.0)
        .appearance(SelectAppearance::Field)
        .disabled(editor.settings.is_err() || !supported)
        .open(editor.menu == Some(kind))
        .on_toggle(move |s: &mut Editor| s.menu = (s.menu != Some(kind)).then_some(kind))
        .on_close(|s: &mut Editor| s.menu = None)
        .on_choose(move |s: &mut Editor, value, _| {
            if let Some(index) = options.iter().position(|&option| option == value) {
                match kind {
                    Menu::Thinking => s.thinking = index,
                    Menu::Hide => s.hide = index,
                }
                s.edit();
            }
            s.menu = None;
        })
        .build(kit, layer)
}

pub fn view(editor: &Editor, cx: &mut Cx<Editor>) -> El {
    let kit = Kit::neutral(cx.system_dark());
    let mut layer = Vec::new();
    let ready = editor.settings.is_ok();
    let dirty = !editor.changes().is_empty();
    let (error, status) = editor.status(dirty);
    let muted = kit.c.text_muted;
    let strong = kit.c.text_strong;

    let clear_theme = Button::new("Clear")
        .help("Remove the theme setting and use Pi's default")
        .disabled(!ready || !editor.initial.theme.supported() || editor.theme.text.is_empty())
        .on_press(|s: &mut Editor| {
            s.theme = TextEdit::default();
            s.edit();
        })
        .build(&kit)
        .key("theme-clear")
        .press_on_return();
    let theme = TextInput::new(&editor.theme)
        .label("Theme")
        .placeholder(if editor.initial.theme.supported() {
            DEFAULT
        } else {
            UNSUPPORTED
        })
        .disabled(!ready || !editor.initial.theme.supported())
        .width(None)
        .on_edit(|s: &mut Editor| &mut s.theme)
        .on_change(|s: &mut Editor, _| s.edit())
        .build(&kit)
        .key("theme-input")
        .grow(1.0)
        .min_w(0.0);
    let path = editor.path.as_ref().map_or_else(
        || "Path unavailable".to_owned(),
        |path| path.display().to_string(),
    );
    let content = col()
        .w_full()
        .padding(24.0)
        .gap(20.0)
        .child(
            col()
                .gap(6.0)
                .w_full()
                .child(
                    text("Pi Config App")
                        .role("heading")
                        .font(TextStyle::new(26.0, 650.0))
                        .color(strong),
                )
                .child(text("User settings").font(kit.ty.ui).color(muted)),
        )
        .child(
            col()
                .padding(14.0)
                .gap(6.0)
                .w_full()
                .radius(8.0)
                .fill(kit.c.layer_01)
                .child(text("SETTINGS FILE").font(kit.ty.ui_compact).color(muted))
                .child(
                    text(path)
                        .key("settings-path")
                        .font(kit.ty.ui)
                        .color(strong)
                        .wrap_anywhere()
                        .w_full()
                        .selectable(true),
                ),
        )
        .child(
            col()
                .px(14.0)
                .w_full()
                .radius(8.0)
                .fill(kit.c.layer_01)
                .child(field_row(
                    &kit,
                    "Theme",
                    if editor.initial.theme.supported() {
                        "Built-in or custom theme name"
                    } else {
                        "Existing value is kept as-is"
                    },
                    row()
                        .w(210.0)
                        .gap(8.0)
                        .center_y()
                        .child(theme)
                        .child(clear_theme),
                ))
                .child(field_row(
                    &kit,
                    "Default thinking level",
                    if editor.initial.thinking.supported() {
                        "Thinking level for new Pi sessions"
                    } else {
                        "Existing value is kept as-is"
                    },
                    menu(
                        editor,
                        &kit,
                        &mut layer,
                        Menu::Thinking,
                        "Default thinking level",
                    ),
                ))
                .child(field_row(
                    &kit,
                    "Thinking block",
                    if editor.initial.hide.supported() {
                        "Show or hide thinking in the transcript"
                    } else {
                        "Existing value is kept as-is"
                    },
                    menu(editor, &kit, &mut layer, Menu::Hide, "Thinking block"),
                )),
        );

    Dialog::new("confirm-reload", "Discard unsaved changes?")
        .open(editor.confirm_reload)
        .fit()
        .width(400.0)
        .description("Reloading replaces your edits with the current settings on disk.")
        .on_close(|s: &mut Editor| s.confirm_reload = false)
        .child(
            row()
                .gap(8.0)
                .padding(16.0)
                .justify_end()
                .child(
                    Button::new("Cancel")
                        .on_press(|s: &mut Editor| s.confirm_reload = false)
                        .build(&kit)
                        .key("cancel-reload")
                        .press_on_return(),
                )
                .child(
                    Button::new("Discard and reload")
                        .variant(ButtonVariant::Contrast)
                        .on_press(Editor::reload)
                        .build(&kit)
                        .key("discard-and-reload")
                        .press_on_return(),
                ),
        )
        .build(&kit, &mut layer);

    col()
        .w_full()
        .h_full()
        .fill(kit.c.bg_base)
        .child(
            col()
                .grow(1.0)
                .min_h(0.0)
                .w_full()
                .scroll(ScrollSpec::default())
                .child(content),
        )
        .child(
            col()
                .w_full()
                .px(24.0)
                .py(14.0)
                .gap(12.0)
                .fill(kit.c.layer_01)
                .child(
                    text(status)
                        .key("status")
                        .font(kit.ty.ui)
                        .wrap()
                        .w_full()
                        .color(if error { kit.c.danger.fg } else { muted })
                        .live_region(if error {
                            LiveRegion::Assertive
                        } else {
                            LiveRegion::Polite
                        }),
                )
                .child(
                    row()
                        .gap(8.0)
                        .justify_end()
                        .child(
                            Button::new("Reload")
                                .control_size(ControlSize::Large)
                                .disabled(editor.path.is_none())
                                .on_press(|s: &mut Editor| {
                                    if s.changes().is_empty() {
                                        s.reload();
                                    } else {
                                        s.confirm_reload = true;
                                    }
                                })
                                .build(&kit)
                                .key("reload")
                                .press_on_return(),
                        )
                        .child(
                            Button::new("Save")
                                .control_size(ControlSize::Large)
                                .variant(ButtonVariant::Contrast)
                                .disabled(!ready || !dirty)
                                .on_press(Editor::save)
                                .build(&kit)
                                .key("save")
                                .press_on_return(),
                        ),
                ),
        )
        .children(layer)
}
