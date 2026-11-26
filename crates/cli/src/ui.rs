use std::cell::Cell;
use std::io;
use std::path::PathBuf;
use std::rc::Rc;

use rune::{Align, Cursor, Cx, El, LiveRegion, ScrollSpec, TextEdit, Tx, col, el, row, text};
use rune_kit::{
    Button, ButtonVariant, ControlSize, Dialog, Kit, ScrollBar, ScrollBarStyle, Select,
    SelectAppearance, TextInput,
};
use serde_json::Value;

use crate::fields::{Atom, DEFAULT, FIELDS, GROUPS, Group, Input, Kind, Spec};
use crate::settings::{Changes, SaveError, Settings, settings_path};

const UNSUPPORTED: &str = "Unsupported value";
const FORM_SCROLL: &str = "settings-form";
const MARKER_GUTTER: f32 = 10.0;
pub const TITLEBAR_HEIGHT: f32 = 32.0;

fn group_index(group: Group) -> usize {
    GROUPS
        .iter()
        .position(|&(item, _, _)| item == group)
        .expect("every settings group has a tab")
}

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
    // Rune's layout callback observes scrolling without rebuilding the editor.
    scroll_offsets: [Rc<Cell<f32>>; GROUPS.len()],
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
            scroll_offsets: Default::default(),
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

    fn show_group(&mut self, group: Group, tx: &mut Tx) {
        if self.group != group {
            self.group = group;
            self.menu = None;
            tx.scroll_to_y(FORM_SCROLL, self.scroll_offsets[group_index(group)].get());
        }
    }

    fn reveal_invalid(&mut self, spec: &Spec, tx: &mut Tx) {
        self.group = spec.group;
        self.menu = None;
        tx.scroll_center_in_instant(FORM_SCROLL, format!("field-{}", spec.path));
        tx.focus_when_shown(format!("{}-input", spec.control_id()), true);
    }

    fn changes(&self) -> Result<Changes, &'static Spec> {
        let mut changes = Changes::new();
        for (spec, field) in FIELDS.iter().zip(&self.fields) {
            let value = field.value(spec).map_err(|_| spec)?;
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

    fn status(&self, dirty: bool, invalid: Option<&Spec>) -> (bool, String) {
        if let Some(spec) = invalid {
            let (_, group, _) = GROUPS[group_index(spec.group)];
            return (
                true,
                format!("Fix {} in {} before saving.", spec.label, group),
            );
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
    let value = field.value(spec);
    let non_default =
        matches!(&value, Ok(Some(value)) if spec.default.map(Atom::value).as_ref() != Some(value));
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
                .invalid(value.is_err())
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
    let label = text(spec.label)
        .key(format!("label-{}", spec.path))
        .font(kit.ty.ui_medium)
        .color(kit.c.text_strong)
        .wrap()
        .w_full();
    let label = if non_default {
        label.description("Non-default value")
    } else {
        label
    };
    let title = col().grow(1.0).min_w(0.0).gap(4.0).child(label);
    let title = match value {
        Err(issue) => title.child(
            text(issue)
                .font(kit.ty.ui_compact)
                .color(kit.c.danger.fg)
                .wrap()
                .w_full(),
        ),
        _ if unsupported => title.child(
            text("Existing value is kept as-is")
                .font(kit.ty.ui_compact)
                .color(kit.c.text_muted)
                .wrap()
                .w_full(),
        ),
        _ => title,
    };
    let gutter = row().w(MARKER_GUTTER).children(non_default.then(|| {
        el().key("non-default-marker")
            .w(4.0)
            .h(22.0)
            .radius(2.0)
            .fill(kit.c.text_faint)
            .a11y_hidden()
    }));
    row()
        .key(format!("field-{}", spec.path))
        .w_full()
        .center_y()
        .py(6.0)
        .child(gutter)
        .child(
            row()
                .grow(1.0)
                .min_w(0.0)
                .gap(12.0)
                .center_y()
                .child(title)
                .child(control),
        )
}

fn appearance_section(kit: &Kit, path: &str) -> Option<El> {
    let label = match path {
        "theme" => "Terminal",
        "fullscreenExitOutput" => "Fullscreen",
        "editorPaddingX" => "Editor and transcript",
        "terminal.showImages" => "Terminal capabilities",
        "images.autoResize" => "Images",
        "markdown.codeBlockIndent" => "Markdown",
        _ => return None,
    };
    let heading = col()
        .w_full()
        .pl(MARKER_GUTTER)
        .pt(10.0)
        .pb(4.0)
        .gap(4.0)
        .child(
            text(label)
                .role("heading")
                .font(kit.ty.ui_medium)
                .color(kit.c.text_muted),
        );
    Some(if path == "fullscreenExitOutput" {
        heading.child(
            text("These settings only apply in fullscreen mode.")
                .font(kit.ty.ui_compact)
                .color(kit.c.text_muted)
                .w_full()
                .wrap(),
        )
    } else {
        heading
    })
}

pub fn view(editor: &Editor, cx: &mut Cx<Editor>) -> El {
    let kit = Kit::neutral(cx.system_dark());
    let mut layer = Vec::new();
    let changes = editor.changes();
    let invalid = changes.as_ref().err().copied();
    let dirty = !matches!(changes, Ok(changes) if changes.is_empty());
    let (error, status) = editor.status(dirty, invalid);
    let path = match &editor.path {
        Some(path) => {
            let path = path.display().to_string();
            text(path.clone())
                .role("button")
                .label(path.clone())
                .description("Copy the full settings file path")
                .help("Click to copy the full path")
                .cursor(Cursor::Pointer)
                .focusable(true)
                .on_press_tx(move |_: &mut Editor, tx| tx.copy(path.clone()))
                .press_on_return()
        }
        None => text("Path unavailable"),
    }
    .key("settings-path")
    .font(kit.ty.ui_compact)
    .color(kit.c.text_muted)
    .align(Align::End)
    .truncate_head();
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
                    .on_press_tx(move |s: &mut Editor, tx| s.show_group(group, tx))
                    .press_on_return(),
            )
        })
        .role("tablist")
        .label("Settings groups")
        .focus_group();
    let insets = cx.chrome_insets();
    let titlebar = row()
        .key("titlebar")
        .w_full()
        .h(TITLEBAR_HEIGHT)
        .pl(insets.leading + 20.0)
        .pr(insets.trailing + 20.0)
        .center_y()
        .window_drag()
        .child(el().grow(1.0).min_w(48.0))
        .child(path);
    let header = col()
        .w_full()
        .child(titlebar)
        .child(col().w_full().px(20.0).pb(12.0).child(navigation));
    let fields = FIELDS
        .iter()
        .enumerate()
        .filter(|(_, spec)| spec.group == editor.group)
        .fold(
            col()
                .w_full()
                .pl(14.0 - MARKER_GUTTER)
                .pr(14.0)
                .radius(8.0)
                .fill(kit.c.layer_01),
            |rows, (i, spec)| {
                rows.children(appearance_section(&kit, spec.path))
                    .child(field_row(editor, &kit, &mut layer, i, spec))
            },
        );
    let offset = Rc::clone(&editor.scroll_offsets[group_index(editor.group)]);

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
                .scroll_margin(6.0)
                .on_scroll_offset(move |pos| offset.set(pos.offset[1]))
                .child(col().w_full().px(20.0).pb(10.0).child(fields))
                .scroll_bar_styled(
                    ScrollBarStyle::from_kit(&kit)
                        .always_visible()
                        .thumb(kit.c.text_faint)
                        .thumb_hover(kit.c.text_muted),
                ),
        )
        .child(
            row()
                .key("settings-footer")
                .w_full()
                .px(20.0)
                .py(12.0)
                .gap(12.0)
                .center_y()
                .fill(kit.c.layer_01)
                .child(
                    text(status)
                        .key("status")
                        .font(kit.ty.ui)
                        .wrap()
                        .grow(1.0)
                        .min_w(0.0)
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
                        .children(invalid.map(|spec| {
                            Button::new("Go to field")
                                .control_size(ControlSize::Large)
                                .build(&kit)
                                .key("jump-to-invalid")
                                .on_press_tx(move |s: &mut Editor, tx| s.reveal_invalid(spec, tx))
                                .press_on_return()
                        }))
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
                                .disabled(editor.settings.is_err() || !dirty || invalid.is_some())
                                .on_press(Editor::save)
                                .build(&kit)
                                .key("save")
                                .press_on_return(),
                        ),
                ),
        )
        .children(layer)
}
