use std::io;
use std::path::PathBuf;

use rune::{Cx, El, LiveRegion, ScrollSpec, TextEdit, TextStyle, col, row, text};
use rune_kit::{
    Button, ButtonVariant, ControlSize, Dialog, Kit, Select, SelectAppearance, TextInput,
};
use serde_json::Value;

use crate::fields::{DEFAULT, FIELDS, GROUPS, Group, Input, Kind, Spec};
use crate::settings::{Changes, SaveError, Settings, settings_path};

const UNSUPPORTED: &str = "Unsupported value";
const FORM_SCROLL: &str = "settings-form";

enum Draft {
    Unsupported,
    Choice(usize),
    Input { edit: Box<TextEdit>, present: bool },
}

struct Field {
    initial: Option<Value>,
    draft: Draft,
}

impl Field {
    fn read(spec: &Spec, settings: &io::Result<Settings>) -> Self {
        let initial = match settings {
            Ok(settings) => match settings.value(spec.path) {
                Ok(value) => value,
                Err(_) => {
                    return Self {
                        initial: None,
                        draft: Draft::Unsupported,
                    };
                }
            },
            Err(_) => None,
        };
        let draft = match spec.kind {
            Kind::Choice(options) => match initial.as_ref() {
                None => Draft::Choice(0),
                Some(value) => options
                    .iter()
                    .position(|(_, atom)| &atom.value() == value)
                    .map_or(Draft::Unsupported, |index| Draft::Choice(index + 1)),
            },
            Kind::Input(input) => match initial.as_ref() {
                None => Draft::Input {
                    edit: Box::default(),
                    present: false,
                },
                Some(value) => {
                    input
                        .display(value)
                        .map_or(Draft::Unsupported, |value| Draft::Input {
                            edit: Box::new(TextEdit::new(value)),
                            present: true,
                        })
                }
            },
        };
        let initial = if matches!(draft, Draft::Unsupported) {
            None
        } else {
            initial
        };
        Self { initial, draft }
    }

    fn value(&self, spec: &Spec) -> Result<Option<Value>, String> {
        match (&self.draft, spec.kind) {
            (Draft::Unsupported, _) => Ok(None),
            (Draft::Choice(index), Kind::Choice(options)) => {
                Ok(index.checked_sub(1).map(|i| options[i].1.value()))
            }
            (
                Draft::Input {
                    edit,
                    present: true,
                },
                Kind::Input(input),
            ) => input.parse(&edit.text),
            (Draft::Input { present: false, .. }, Kind::Input(_)) => Ok(None),
            _ => unreachable!("draft must match the field spec"),
        }
    }
}

pub struct Editor {
    path: Option<PathBuf>,
    settings: io::Result<Settings>,
    fields: Vec<Field>,
    group: Group,
    menu: Option<usize>,
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
            fields: Vec::new(),
            group: Group::Models,
            menu: None,
            confirm_reload: false,
            notice: None,
        };
        editor.reload();
        editor
    }

    fn read_fields(&mut self) {
        self.fields = FIELDS
            .iter()
            .map(|spec| Field::read(spec, &self.settings))
            .collect();
    }

    fn reload(&mut self) {
        self.confirm_reload = false;
        self.menu = None;
        self.notice = None;
        if let Some(path) = &self.path {
            self.settings = Settings::load(path);
        }
        self.read_fields();
    }

    fn changes(&self) -> Result<Changes, ()> {
        let mut changes = Changes::new();
        for (spec, field) in FIELDS.iter().zip(&self.fields) {
            let value = field.value(spec).map_err(|_| ())?;
            if value != field.initial {
                changes.insert(spec.path, value);
            }
        }
        Ok(changes)
    }

    fn save(&mut self) {
        let Ok(changes) = self.changes() else {
            return;
        };
        let (Some(path), Ok(settings)) = (&self.path, &mut self.settings) else {
            return;
        };
        if changes.is_empty() {
            return;
        }
        self.notice = Some(match settings.save(path, changes) {
            Ok(()) => {
                self.read_fields();
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

    fn status(&self, dirty: bool, invalid: bool) -> (bool, String) {
        if invalid {
            return (true, "Fix invalid settings before saving.".into());
        }
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

fn focus_row(control: El, spec: &Spec) -> El {
    let key = format!("field-{}", spec.path);
    control.on_focus_change_any(move |focused, tx| {
        if focused {
            tx.scroll_into_view_in(FORM_SCROLL, &key);
        }
    })
}

fn field_row(editor: &Editor, kit: &Kit, layer: &mut Vec<El>, index: usize, spec: &Spec) -> El {
    let field = &editor.fields[index];
    let unsupported = matches!(field.draft, Draft::Unsupported);
    let disabled = editor.settings.is_err() || unsupported;
    let issue = field.value(spec).err();
    let id = spec.control_id();
    let control = match spec.kind {
        Kind::Choice(options) => {
            let labels: Vec<_> = std::iter::once(DEFAULT)
                .chain(options.iter().map(|&(label, _)| label))
                .collect();
            let current = match field.draft {
                Draft::Choice(n) => labels[n],
                _ => UNSUPPORTED,
            };
            focus_row(
                Select::new(id, &labels)
                    .label(spec.label)
                    .current(Some(current))
                    .width(230.0)
                    .appearance(SelectAppearance::Field)
                    .disabled(disabled)
                    .open(editor.menu == Some(index))
                    .on_toggle(move |s: &mut Editor| {
                        s.menu = (s.menu != Some(index)).then_some(index)
                    })
                    .on_close(|s: &mut Editor| s.menu = None)
                    .on_choose(move |s: &mut Editor, value, _| {
                        if let Some(n) = labels.iter().position(|&label| label == value) {
                            s.fields[index].draft = Draft::Choice(n);
                            s.edit();
                        }
                        s.menu = None;
                    })
                    .build(kit, layer),
                spec,
            )
        }
        Kind::Input(input) => {
            let blank = TextEdit::default();
            let (edit, present) = match &field.draft {
                Draft::Input { edit, present } => (edit.as_ref(), *present),
                _ => (&blank, false),
            };
            let placeholder = if unsupported {
                UNSUPPORTED
            } else if present && matches!(input, Input::Text { allow_empty: true }) {
                "Empty string"
            } else {
                DEFAULT
            };
            let input_control = TextInput::new(edit)
                .label(spec.label)
                .placeholder(placeholder)
                .disabled(disabled)
                .invalid(issue.is_some())
                .width(None)
                .on_edit(move |s: &mut Editor| match &mut s.fields[index].draft {
                    Draft::Input { edit, .. } => edit,
                    _ => unreachable!("only enabled inputs can be edited"),
                })
                .on_change(move |s: &mut Editor, _| {
                    if let Draft::Input { present, .. } = &mut s.fields[index].draft {
                        *present = true;
                    }
                    s.edit();
                })
                .build(kit)
                .key(format!("{id}-input"))
                .grow(1.0)
                .min_w(0.0);
            let set_empty =
                matches!(input, Input::Text { allow_empty: true }) && !present && !disabled;
            let action = Button::new(if set_empty { "Set empty" } else { "Clear" })
                .help(if set_empty {
                    "Set an explicit empty string"
                } else {
                    "Remove the setting and use Pi's default"
                })
                .disabled(disabled || (!present && !set_empty))
                .on_press(move |s: &mut Editor| {
                    s.fields[index].draft = Draft::Input {
                        edit: Box::default(),
                        present: set_empty,
                    };
                    s.edit();
                })
                .build(kit)
                .key(format!(
                    "{id}-{}",
                    if set_empty { "empty" } else { "clear" }
                ))
                .press_on_return();
            row()
                .w(230.0)
                .gap(8.0)
                .center_y()
                .child(focus_row(input_control, spec))
                .child(focus_row(action, spec))
        }
    };
    let mut title = col().grow(1.0).min_w(0.0).gap(4.0).child(
        text(spec.label)
            .font(kit.ty.ui_medium)
            .color(kit.c.text_strong)
            .wrap()
            .w_full(),
    );
    if let Some(issue) = issue {
        title = title.child(
            text(issue)
                .font(kit.ty.ui_compact)
                .color(kit.c.danger.fg)
                .wrap()
                .w_full(),
        );
    } else if unsupported {
        title = title.child(
            text("Existing value is kept as-is")
                .font(kit.ty.ui_compact)
                .color(kit.c.text_muted)
                .wrap()
                .w_full(),
        );
    }
    row()
        .key(format!("field-{}", spec.path))
        .w_full()
        .gap(12.0)
        .center_y()
        .py(6.0)
        .child(title)
        .child(control)
}

pub fn view(editor: &Editor, cx: &mut Cx<Editor>) -> El {
    let kit = Kit::neutral(cx.system_dark());
    let mut layer = Vec::new();
    let changes = editor.changes();
    let invalid = changes.is_err();
    let dirty = !matches!(changes, Ok(changes) if changes.is_empty());
    let (error, status) = editor.status(dirty, invalid);
    let path = editor.path.as_ref().map_or_else(
        || "Path unavailable".to_owned(),
        |path| path.display().to_string(),
    );
    let navigation = GROUPS
        .iter()
        .fold(row().gap(6.0), |nav, &(group, label, key)| {
            let active = editor.group == group;
            nav.child(
                Button::new(label)
                    .variant(if active {
                        ButtonVariant::Contrast
                    } else {
                        ButtonVariant::default()
                    })
                    .build(&kit)
                    .key(key)
                    .role("tab")
                    .focus_selected(active)
                    .on_press_tx(move |s: &mut Editor, tx| {
                        if group != s.group {
                            s.group = group;
                            s.menu = None;
                            tx.scroll_to_instant(FORM_SCROLL, 0.0, 0.0);
                        }
                    })
                    .press_on_return(),
            )
        })
        .role("tablist")
        .label("Settings groups")
        .focus_group();
    let header = col()
        .w_full()
        .px(20.0)
        .py(12.0)
        .gap(8.0)
        .child(
            text("Pi Config App")
                .role("heading")
                .font(TextStyle::new(24.0, 650.0))
                .color(kit.c.text_strong),
        )
        .child(
            text("USER SETTINGS FILE")
                .font(kit.ty.ui_compact)
                .color(kit.c.text_muted),
        )
        .child(
            text(path)
                .key("settings-path")
                .font(kit.ty.ui)
                .color(kit.c.text_strong)
                .wrap_anywhere()
                .w_full()
                .selectable(true),
        )
        .child(navigation);
    let fields = FIELDS
        .iter()
        .enumerate()
        .filter(|(_, spec)| spec.group == editor.group)
        .fold(
            col().w_full().px(14.0).radius(8.0).fill(kit.c.layer_01),
            |rows, (i, spec)| rows.child(field_row(editor, &kit, &mut layer, i, spec)),
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
        .child(header)
        .child(
            col()
                .key(FORM_SCROLL)
                .grow(1.0)
                .min_h(0.0)
                .w_full()
                .scroll(ScrollSpec::default())
                .child(col().w_full().px(20.0).pb(10.0).child(fields)),
        )
        .child(
            col()
                .w_full()
                .px(20.0)
                .py(12.0)
                .gap(8.0)
                .fill(kit.c.layer_01)
                .child(
                    text(status)
                        .key("status")
                        .font(kit.ty.ui)
                        .wrap()
                        .w_full()
                        .color(if error {
                            kit.c.danger.fg
                        } else {
                            kit.c.text_muted
                        })
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
                                .on_press(|s: &mut Editor| match s.changes() {
                                    Ok(changes) if changes.is_empty() => s.reload(),
                                    _ => s.confirm_reload = true,
                                })
                                .build(&kit)
                                .key("reload")
                                .press_on_return(),
                        )
                        .child(
                            Button::new("Save")
                                .control_size(ControlSize::Large)
                                .variant(ButtonVariant::Contrast)
                                .disabled(editor.settings.is_err() || !dirty || invalid)
                                .on_press(Editor::save)
                                .build(&kit)
                                .key("save")
                                .press_on_return(),
                        ),
                ),
        )
        .children(layer)
}
