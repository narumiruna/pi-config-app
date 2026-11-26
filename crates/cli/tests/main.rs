use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, UNIX_EPOCH};

use serde_json::{Value, json, value::RawValue};
use tempfile::{TempDir, tempdir, tempdir_in};

const DEFAULT: &str = "Use Pi default";
const EXE: &str = env!("CARGO_BIN_EXE_pi-config-app");

struct Fixture {
    _root: TempDir,
    root: PathBuf,
    home: PathBuf,
    cwd: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let home = root.join("home");
        let cwd = root.join("project");
        for path in [&home, &cwd, &root.join("runtime")] {
            fs::create_dir(path).unwrap();
        }
        Self {
            _root: dir,
            root,
            home,
            cwd,
        }
    }

    fn settings(&self) -> PathBuf {
        self.home.join(".pi/agent/settings.json")
    }

    fn run(&self, agent_dir: Option<&Path>, script: Script) -> Tree {
        self.prepare(agent_dir, script).finish()
    }

    fn run_tall(&self, script: Script) -> Tree {
        let mut run = self.prepare(None, script);
        run.command.args(["--size", "900x1700"]);
        run.finish()
    }

    fn prepare(&self, agent_dir: Option<&Path>, script: Script) -> Run {
        let mut command = Command::new(EXE);
        let files = tempdir_in(&self.root).unwrap();
        let inputs = files.path().join("script.json");
        let tree = files.path().join("tree.json");
        fs::write(inputs.as_path(), serde_json::to_vec(&script.end()).unwrap()).unwrap();
        command
            .env_clear()
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("XDG_RUNTIME_DIR", self.root.join("runtime"))
            .env("LP_NUM_THREADS", "2")
            .current_dir(&self.cwd)
            .args(["--scale", "1", "--fps", "20", "--at", "mark:end", "--tree"])
            .arg(&tree)
            .arg("--script")
            .arg(inputs);
        if let Some(dir) = agent_dir {
            command.env("PI_CODING_AGENT_DIR", dir);
        }
        Run {
            files,
            tree,
            command,
        }
    }
}

struct Run {
    files: TempDir,
    tree: PathBuf,
    command: Command,
}

impl Run {
    fn finish(mut self) -> Tree {
        let output = self.command.output().expect("launch pi-config-app");
        self.read_tree(&output)
    }

    fn on_external_mark(mut self, change_file: impl FnOnce()) -> Tree {
        // Keep stderr off the pipe so logging cannot block replay while we wait for the mark.
        let stderr = self.files.path().join("stderr");
        let mut child = self
            .command
            .arg("--realtime")
            .stdout(Stdio::piped())
            .stderr(File::create(&stderr).unwrap())
            .spawn()
            .expect("launch pi-config-app");
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let mut log = String::new();
        let marked = loop {
            let start = log.len();
            if stdout.read_line(&mut log).unwrap() == 0 {
                break false;
            }
            if log[start..].starts_with("rune-mark external ") {
                change_file();
                break true;
            }
        };
        stdout.read_to_string(&mut log).unwrap();
        let mut output = child.wait_with_output().unwrap();
        output.stdout = log.into_bytes();
        output.stderr = fs::read(stderr).unwrap();
        let tree = self.read_tree(&output);
        assert!(marked, "Rune did not reach the external change mark");
        tree
    }

    fn read_tree(&self, output: &Output) -> Tree {
        assert!(
            output.status.success(),
            "pi-config-app exited with {}\nstdout: {}\nstderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let bytes = fs::read(&self.tree).expect("Rune wrote the UI tree");
        Tree(serde_json::from_slice(&bytes).unwrap())
    }
}

#[derive(Clone, Default)]
struct Script {
    ticks: u32,
    events: Vec<Value>,
}

impl Script {
    fn event(&mut self, mut event: Value) {
        self.ticks += 2;
        event["t"] = json!(f64::from(self.ticks) / 10.0);
        self.events.push(event);
    }

    fn tap(&mut self, key: &str) {
        self.event(json!({"type": "tap", "key": key}));
    }

    fn in_group(group: &str) -> Self {
        let mut script = Self::default();
        script.group(group);
        script
    }

    fn group(&mut self, group: &str) {
        self.tap(&format!("group-{group}"));
    }

    fn scroll_form(&mut self, dy: i32) {
        self.event(json!({"type": "wheel", "key": "settings-form", "dy": dy}));
    }

    fn replace_input(&mut self, key: &str, value: &str) {
        self.tap(&format!("{key}-input"));
        self.event(json!({"type": "key", "name": "a", "cmd": true}));
        self.event(json!({"paste": value}));
    }

    fn replace_theme(&mut self, value: &str) {
        self.group("appearance");
        self.replace_input("theme", value);
    }

    fn select(&mut self, key: &str, option: usize) {
        self.tap(&format!("{key}-trigger"));
        self.tap(&format!("option{option}"));
    }

    fn external_mark(&mut self) {
        // Rune flushes this mark only after launch. Leave a wall-clock interval for the
        // parent process to change the fixture before Rune delivers the next input.
        self.event(json!({"type": "mark", "name": "external"}));
        self.ticks += 10;
    }

    fn end(mut self) -> Vec<Value> {
        self.event(json!({"type": "mark", "name": "end"}));
        self.events
    }
}

#[derive(Debug)]
struct Tree(Value);

impl Tree {
    fn elements(&self) -> &[Value] {
        self.0["elements"].as_array().unwrap()
    }

    fn node(&self, key: &str) -> &Value {
        self.elements()
            .iter()
            .find(|e| e["key"] == key && e["exiting"] != true)
            .unwrap_or_else(|| panic!("no {key:?} in UI: {self:#?}"))
    }

    fn subtree(&self, key: &str) -> impl Iterator<Item = &Value> {
        let path = self.node(key)["path"].as_str().unwrap();
        self.elements().iter().filter(move |e| {
            let child = e["path"].as_str().unwrap();
            e["exiting"] != true
                && (child == path
                    || child
                        .strip_prefix(path)
                        .is_some_and(|suffix| suffix.starts_with('/')))
        })
    }

    fn accessible_node(&self, key: &str) -> &Value {
        fn find<'a>(node: &'a Value, key: &str) -> Option<&'a Value> {
            if node["key"] == key {
                return Some(node);
            }
            node["children"]
                .as_array()?
                .iter()
                .find_map(|child| find(child, key))
        }
        find(&self.0["accessibility"], key).unwrap_or_else(|| panic!("no accessible {key:?} in UI"))
    }

    fn rect(&self, key: &str) -> [f64; 4] {
        std::array::from_fn(|i| self.node(key)["rect"][i].as_f64().unwrap())
    }

    fn assert_in_bounds(&self, key: &str, [left, top, width, height]: [f64; 4]) {
        let [x, y, w, h] = self.rect(key);
        // Rune rounds the individual coordinates to hundredths of a point.
        let tolerance = 0.02;
        assert!(
            w > 0.0
                && h > 0.0
                && x >= left - tolerance
                && y >= top - tolerance
                && x + w <= left + width + tolerance
                && y + h <= top + height + tolerance,
            "{key} at {:?} is outside {:?}",
            [x, y, w, h],
            [left, top, width, height]
        );
    }

    fn assert_value(&self, key: &str, expected: &str) {
        assert!(
            self.subtree(key).any(|e| e["text"] == expected),
            "expected {expected:?} in {key:?}: {:#?}",
            self.subtree(key).collect::<Vec<_>>()
        );
    }

    fn assert_path(&self, path: &Path) {
        let displayed = self.node("settings-path")["text"].as_str().unwrap();
        assert!(displayed.contains(path.to_str().unwrap()), "{displayed:?}");
    }

    fn assert_message(&self, message: &str) {
        let texts = self
            .elements()
            .iter()
            .filter_map(|e| e["text"].as_str())
            .collect::<Vec<_>>();
        assert!(
            texts.iter().any(|text| text.contains(message)),
            "missing {message:?} in UI text: {texts:?}"
        );
    }

    fn assert_options(&self, key: &str, expected: &[&str]) {
        let labels: Vec<_> = self
            .subtree(key)
            .filter(|e| e["role"] == "option")
            .map(|e| e["label"].as_str().unwrap())
            .collect();
        assert_eq!(labels, expected);
    }

    fn assert_save_disabled(&self) {
        assert_eq!(self.node("save")["focusable"], false, "Save is enabled");
    }

    fn assert_non_default(&self, path: &str, expected: bool) {
        let key = format!("field-{path}");
        let marker = self
            .subtree(&key)
            .find(|node| node["key"] == "non-default-marker");
        assert_eq!(marker.is_some(), expected, "Non-default marker for {path}");
        assert!(!self.subtree(&key).any(|node| node["text"] == "Non-default"));
        if let Some(marker) = marker {
            let label = self.rect(&format!("label-{path}"));
            let [x, _, width, height] =
                std::array::from_fn(|i| marker["rect"][i].as_f64().unwrap());
            assert!(width > 0.0 && height > width && x + width <= label[0]);
        }
    }

    fn assert_row_message(&self, path: &str, message: &str) {
        let row = format!("field-{path}");
        let texts: Vec<_> = self
            .subtree(&row)
            .filter_map(|e| e["text"].as_str())
            .collect();
        assert!(
            texts.iter().any(|text| text.contains(message)),
            "missing {message:?} in {row}: {texts:?}"
        );
    }
}

fn write(path: &Path, contents: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn program_attempts_save() -> Script {
    let mut script = Script::default();
    script.tap("save");
    script
}

#[derive(Clone, Copy)]
enum Override {
    Text(&'static str),
    Number(u64),
    // Option index, displayed label, and the literal JSON token expected on disk.
    Choice(usize, &'static str, &'static str),
}

use Override::{Choice, Number, Text};

type Case = (&'static str, Override);

// Independent inventory from the coverage plan, not imported from the application's field list.
const COVERAGE: &[(&str, &[Case])] = &[
    (
        "models",
        &[
            ("defaultProvider", Text("local-provider")),
            ("defaultModel", Text("local/model-v2")),
            ("defaultThinkingLevel", Choice(5, "high", r#""high""#)),
            ("hideThinkingBlock", Choice(2, "Hide", "true")),
            ("showCacheMissNotices", Choice(1, "On", "true")),
        ],
    ),
    (
        "interaction",
        &[
            ("steeringMode", Choice(1, "all", r#""all""#)),
            (
                "followUpMode",
                Choice(2, "one-at-a-time", r#""one-at-a-time""#),
            ),
            ("doubleEscapeAction", Choice(2, "fork", r#""fork""#)),
            ("treeFilterMode", Choice(3, "user-only", r#""user-only""#)),
        ],
    ),
    (
        "appearance",
        &[
            ("theme", Text("light/custom-dark")),
            ("quietStartup", Choice(3, "Header only", r#""header""#)),
            ("tuiMode", Choice(1, "regular", r#""regular""#)),
            (
                "fullscreenExitOutput",
                Choice(2, "resume-hint", r#""resume-hint""#),
            ),
            ("fullscreenScrollbar", Choice(2, "always", r#""always""#)),
            ("fullscreenCopyOnSelect", Choice(2, "Off", "false")),
            ("fullscreenWheelScrollLines", Text("auto")),
            ("editorPaddingX", Number(0)),
            ("outputPad", Choice(1, "0", "0")),
            ("autocompleteMaxVisible", Number(20)),
            ("showHardwareCursor", Choice(1, "On", "true")),
            ("terminal.showImages", Choice(2, "Off", "false")),
            ("terminal.imageWidthCells", Number(72)),
            ("terminal.clearOnShrink", Choice(1, "On", "true")),
            ("terminal.showTerminalProgress", Choice(1, "On", "true")),
            ("terminal.hyperlinks", Choice(3, "auto", r#""auto""#)),
            ("terminal.images", Choice(4, "Off", "false")),
            ("terminal.trueColor", Choice(3, "auto", r#""auto""#)),
            ("images.autoResize", Choice(2, "Off", "false")),
            ("images.blockImages", Choice(1, "On", "true")),
            ("markdown.codeBlockIndent", Text("")),
            ("markdown.mermaid", Choice(2, "final", r#""final""#)),
        ],
    ),
    ("updates", &[("collapseChangelog", Choice(1, "On", "true"))]),
];

fn control_key(path: &str) -> &str {
    match path {
        "defaultThinkingLevel" => "thinking",
        "hideThinkingBlock" => "hide",
        _ => path,
    }
}

impl Override {
    fn enter(self, script: &mut Script, path: &str) {
        let key = control_key(path);
        match self {
            Text("") => script.tap(&format!("{key}-empty")),
            Text(value) => script.replace_input(key, value),
            Number(value) => script.replace_input(key, &value.to_string()),
            Choice(index, _, _) => script.select(key, index),
        }
    }

    fn reset(self, script: &mut Script, path: &str) {
        let key = control_key(path);
        match self {
            Choice(..) => script.select(key, 0),
            Text(_) | Number(_) => script.tap(&format!("{key}-clear")),
        }
    }

    fn json(self) -> Value {
        match self {
            Text(value) => json!(value),
            Number(value) => json!(value),
            Choice(_, _, token) => serde_json::from_str(token).unwrap(),
        }
    }

    fn selector(self, path: &str) -> String {
        let suffix = match self {
            Choice(..) => "trigger",
            Text(_) | Number(_) => "input",
        };
        format!("{}-{suffix}", control_key(path))
    }

    fn assert_loaded(self, tree: &Tree, path: &str) {
        let selector = self.selector(path);
        match self {
            Text(label) | Choice(_, label, _) => {
                tree.assert_value(&selector, label);
                if label.is_empty() {
                    // An explicit empty override can be cleared, unlike an unset input.
                    assert_eq!(tree.node(&format!("{path}-clear"))["focusable"], true);
                }
            }
            Number(value) => tree.assert_value(&selector, &value.to_string()),
        }
    }

    fn assert_default(self, tree: &Tree, path: &str) {
        let selector = self.selector(path);
        match self {
            Choice(..) => {
                assert_eq!(tree.node(&selector)["focusable"], true, "{selector}");
                tree.assert_value(&selector, DEFAULT);
            }
            Text(_) | Number(_) => {
                assert!(
                    tree.subtree(&selector).any(|node| {
                        node["role"] == "textbox" && node["focusable"] == true && node["text"] == ""
                    }),
                    "{selector} should be blank and editable"
                );
                let (action, enabled) = if matches!(self, Text("")) {
                    ("empty", true)
                } else {
                    ("clear", false)
                };
                assert_eq!(
                    tree.node(&format!("{path}-{action}"))["focusable"],
                    enabled,
                    "{path}"
                );
            }
        }
    }
}

fn record_override(document: &mut Value, path: &str, value: Override) {
    if let Some((parent, leaf)) = path.split_once('.') {
        document[parent][leaf] = value.json();
    } else {
        document[path] = value.json();
    }
}

fn assert_field_inventory(tree: &Tree, fields: &[Case]) {
    let mut expected: Vec<_> = fields.iter().map(|&(path, _)| path).collect();
    let mut actual: Vec<_> = tree
        .elements()
        .iter()
        .filter(|e| e["exiting"] != true)
        .filter_map(|e| e["key"].as_str()?.strip_prefix("field-"))
        .collect();
    expected.sort_unstable();
    actual.sort_unstable();
    assert_eq!(actual, expected, "each editable path needs exactly one row");
    for &(path, value) in fields {
        let selector = value.selector(path);
        assert!(
            tree.subtree(&format!("field-{path}"))
                .any(|element| element["key"] == selector),
            "{selector} is not in the row for {path}"
        );
    }
}

#[test]
fn sets_reopens_and_resets_every_setting_across_all_groups() {
    let f = Fixture::new();
    let path = f.settings();
    let mut expected = json!({});
    let mut set = Script::default();
    let mut reset = Script::default();
    for &(group, fields) in COVERAGE {
        set.group(group);
        reset.group(group);
        for &(path, value) in fields {
            value.enter(&mut set, path);
            value.reset(&mut reset, path);
            record_override(&mut expected, path, value);
        }
    }
    assert_eq!(
        COVERAGE
            .iter()
            .map(|(_, fields)| fields.len())
            .sum::<usize>(),
        32
    );
    assert_eq!(expected.as_object().unwrap().len(), 24);

    set.tap("save");
    let saved = f.run_tall(set);
    saved.assert_message("/reload");
    saved.assert_save_disabled();
    assert_eq!(read_json(&path), expected);

    for &(group, fields) in COVERAGE {
        let script = if group == "models" {
            Script::default() // Models must be the initial group.
        } else {
            Script::in_group(group)
        };
        let reopened = f.run_tall(script);
        assert_field_inventory(&reopened, fields);
        for &(path, value) in fields {
            value.assert_loaded(&reopened, path);
        }
        reopened.assert_save_disabled();
    }

    reset.tap("save");
    f.run_tall(reset).assert_message("/reload");
    assert_eq!(
        read_json(&path),
        json!({"terminal": {}, "images": {}, "markdown": {}})
    );
    for &(group, fields) in COVERAGE {
        let reopened = f.run_tall(Script::in_group(group));
        for &(path, value) in fields {
            value.assert_default(&reopened, path);
        }
        reopened.assert_save_disabled();
    }
}

#[test]
fn marks_loaded_values_that_differ_from_pi_defaults() {
    let f = Fixture::new();
    write(
        &f.settings(),
        r#"{
            "defaultProvider": "custom-provider",
            "defaultThinkingLevel": "medium",
            "hideThinkingBlock": true,
            "showCacheMissNotices": "unsupported",
            "theme": "system",
            "editorPaddingX": 0,
            "terminal": {"imageWidthCells": 72, "images": "auto"},
            "markdown": {"codeBlockIndent": ""}
        }"#,
    );
    let models = f.run(None, Script::default());
    models.assert_non_default("defaultProvider", true);
    models.assert_non_default("defaultModel", false);
    models.assert_non_default("defaultThinkingLevel", false);
    models.assert_non_default("hideThinkingBlock", true);
    models.assert_non_default("showCacheMissNotices", false);
    models.assert_row_message("showCacheMissNotices", "Existing value is kept as-is");
    let provider = models.rect("label-defaultProvider");
    let model = models.rect("label-defaultModel");
    assert_eq!(
        [provider[0], provider[2], provider[3]],
        [model[0], model[2], model[3]]
    );
    assert_eq!(
        models.rect("field-defaultProvider")[3],
        models.rect("field-defaultModel")[3]
    );
    assert_eq!(
        models.accessible_node("label-defaultProvider")["description"],
        "Non-default value"
    );
    assert!(models.accessible_node("label-defaultModel")["description"].is_null());
    models.assert_save_disabled();

    let appearance = f.run_tall(Script::in_group("appearance"));
    appearance.assert_non_default("theme", false);
    appearance.assert_non_default("editorPaddingX", false);
    appearance.assert_non_default("terminal.imageWidthCells", true);
    appearance.assert_non_default("terminal.images", false);
    appearance.assert_non_default("markdown.codeBlockIndent", true);
}

#[test]
fn updates_non_default_markers_before_saving_edits_and_resets() {
    let f = Fixture::new();
    let path = f.settings();
    let original = r#"{"defaultProvider":"custom-provider","defaultThinkingLevel":"medium"}"#;
    write(&path, original);

    let mut script = Script::default();
    script.replace_input("defaultModel", "custom-model");
    script.select("thinking", 5);
    let edited = f.run(None, script.clone());
    edited.assert_non_default("defaultModel", true);
    edited.assert_non_default("defaultThinkingLevel", true);
    edited.assert_message("Unsaved changes");

    script.tap("defaultProvider-clear");
    script.select("thinking", 4);
    let explicit_default = f.run(None, script.clone());
    explicit_default.assert_value("thinking-trigger", "medium");
    explicit_default.assert_non_default("defaultThinkingLevel", false);
    explicit_default.assert_non_default("defaultProvider", false);

    script.select("thinking", 5);
    script.select("thinking", 0);
    let reset = f.run(None, script.clone());
    reset.assert_value("thinking-trigger", DEFAULT);
    reset.assert_non_default("defaultThinkingLevel", false);
    assert_eq!(fs::read_to_string(&path).unwrap(), original);

    script.tap("save");
    let saved = f.run(None, script);
    saved.assert_message("/reload");
    saved.assert_non_default("defaultModel", true);
    saved.assert_non_default("defaultThinkingLevel", false);
    assert_eq!(read_json(&path), json!({"defaultModel": "custom-model"}));
}

#[test]
fn appearance_has_section_headings_and_preserves_its_scroll_position_across_tabs() {
    let f = Fixture::new();
    write(&f.settings(), "{}");
    let mut script = Script::in_group("appearance");
    let top = f.run(None, script.clone());
    let headings: Vec<_> = top
        .subtree("settings-form")
        .filter(|node| node["role"] == "heading")
        .map(|node| node["text"].as_str().unwrap())
        .collect();
    assert_eq!(
        headings,
        [
            "Terminal",
            "Fullscreen",
            "Editor and transcript",
            "Terminal capabilities",
            "Images",
            "Markdown"
        ]
    );

    // The scrollbar must already be visible before any scroll or hover on the form.
    let form = top.rect("settings-form");
    for key in ["scroll-bar", "thumb"] {
        assert!(
            top.node(key)["opacity"].as_f64().unwrap() > 0.0,
            "{key} is hidden at rest"
        );
        top.assert_in_bounds(key, form);
    }

    let row = "field-terminal.imageWidthCells";
    script.scroll_form(-500);
    let scrolled = f.run(None, script.clone());
    assert_ne!(scrolled.rect(row), top.rect(row));
    scrolled.assert_in_bounds(row, form);
    assert!(scrolled.rect("thumb")[1] > top.rect("thumb")[1]);

    script.group("models");
    let models = f.run(None, script.clone());
    assert_eq!(models.node("group-models")["itemSelected"], true);
    models.assert_in_bounds("field-defaultProvider", models.rect("settings-form"));
    script.group("appearance");
    let returned = f.run(None, script);
    assert_eq!(returned.node("group-appearance")["itemSelected"], true);
    assert_eq!(returned.rect(row), scrolled.rect(row));
}

#[test]
fn fullscreen_preferences_have_a_persistent_hint_and_can_be_changed_in_regular_mode() {
    let f = Fixture::new();
    let path = f.settings();
    write(&path, r#"{"tuiMode":"regular"}"#);
    let hint = "These settings only apply in fullscreen mode.";
    let assert_regular_with_hint = |tree: &Tree| {
        tree.assert_value("tuiMode-trigger", "regular");
        let mut form = tree.subtree("settings-form");
        assert!(form.any(|node| node["role"] == "heading" && node["text"] == "Fullscreen"));
        assert!(
            form.take_while(|node| node["key"] != "field-fullscreenExitOutput")
                .any(|node| node["text"] == hint),
            "missing hint at the top of the Fullscreen section"
        );
    };
    let mut script = Script::in_group("appearance");
    assert_regular_with_hint(&f.run_tall(script.clone()));

    script.select("fullscreenExitOutput", 2);
    script.select("fullscreenScrollbar", 2);
    script.select("fullscreenCopyOnSelect", 2);
    script.replace_input("fullscreenWheelScrollLines", "7");
    script.tap("save");
    let saved = f.run_tall(script);
    saved.assert_message("/reload");
    assert_regular_with_hint(&saved);
    assert_eq!(
        read_json(&path),
        json!({
            "tuiMode": "regular",
            "fullscreenExitOutput": "resume-hint",
            "fullscreenScrollbar": "always",
            "fullscreenCopyOnSelect": false,
            "fullscreenWheelScrollLines": 7,
        })
    );
}

#[test]
fn switching_groups_keeps_pending_edits_without_writing_or_creating_a_parent() {
    let f = Fixture::new();
    let path = f.settings();
    let original = r#"{ "unrelated" : [1,null] }"#;
    write(&path, original);

    let mut script = Script::in_group("appearance");
    script.select("terminal.showImages", 2);
    script.group("updates");
    script.select("collapseChangelog", 1);
    script.group("appearance");
    let tree = f.run_tall(script);
    tree.assert_value("terminal.showImages-trigger", "Off");
    assert_eq!(tree.node("save")["focusable"], true);
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
}

#[test]
fn mixed_choices_store_the_correct_json_types() {
    let f = Fixture::new();
    let path = f.settings();
    let mut expected = json!({});
    for choices in [
        &[
            ("quietStartup", Choice(1, "Hide startup", "true")),
            ("terminal.hyperlinks", Choice(1, "On", "true")),
            ("terminal.images", Choice(1, "kitty", r#""kitty""#)),
            ("terminal.trueColor", Choice(1, "On", "true")),
            ("outputPad", Choice(2, "1", "1")),
        ][..],
        &[
            ("quietStartup", Choice(2, "Show startup", "false")),
            ("terminal.hyperlinks", Choice(2, "Off", "false")),
            ("terminal.images", Choice(2, "iterm2", r#""iterm2""#)),
            ("terminal.trueColor", Choice(2, "Off", "false")),
        ][..],
        &[("terminal.images", Choice(3, "auto", r#""auto""#))][..],
    ] {
        let mut script = Script::in_group("appearance");
        for &(path, value) in choices {
            value.enter(&mut script, path);
            record_override(&mut expected, path, value);
        }
        script.tap("save");
        f.run_tall(script).assert_message("/reload");
        assert_eq!(read_json(&path), expected);
        let reopened = f.run_tall(Script::in_group("appearance"));
        for &(path, value) in choices {
            value.assert_loaded(&reopened, path);
        }
    }
}

#[test]
fn numeric_inputs_accept_both_inclusive_boundaries() {
    let f = Fixture::new();
    let path = f.settings();
    let boundaries = [
        ("editorPaddingX", 0, 3),
        ("autocompleteMaxVisible", 3, 20),
        ("fullscreenWheelScrollLines", 1, 100),
        ("terminal.imageWidthCells", 1, 9_007_199_254_740_991),
    ];
    for use_max in [false, true] {
        let mut script = Script::in_group("appearance");
        let mut expected = json!({});
        for (field, min, max) in boundaries {
            let value = Number(if use_max { max } else { min });
            value.enter(&mut script, field);
            record_override(&mut expected, field, value);
        }
        script.tap("save");
        f.run_tall(script).assert_message("/reload");
        assert_eq!(read_json(&path), expected);
        let reopened = f.run_tall(Script::in_group("appearance"));
        for (field, min, max) in boundaries {
            Number(if use_max { max } else { min }).assert_loaded(&reopened, field);
        }
    }
}

#[test]
fn invalid_numeric_edits_explain_the_error_and_block_other_pending_edits() {
    let f = Fixture::new();
    let path = f.settings();
    let original = r#"{ "defaultProvider" : "old-provider", "terminal": {"imageWidthCells": 60} }"#;
    write(&path, original);
    for (field, invalid, explanation) in [
        ("editorPaddingX", "-1", "whole number from 0 to 3"),
        ("editorPaddingX", "4", "whole number from 0 to 3"),
        ("editorPaddingX", "1.5", "whole number from 0 to 3"),
        ("autocompleteMaxVisible", "2", "whole number from 3 to 20"),
        ("autocompleteMaxVisible", "21", "whole number from 3 to 20"),
        (
            "fullscreenWheelScrollLines",
            "0",
            "auto or a whole number from 1 to 100",
        ),
        (
            "fullscreenWheelScrollLines",
            "101",
            "auto or a whole number from 1 to 100",
        ),
        (
            "fullscreenWheelScrollLines",
            "many",
            "auto or a whole number from 1 to 100",
        ),
        (
            "terminal.imageWidthCells",
            "0",
            "whole number from 1 to 9007199254740991",
        ),
        (
            "terminal.imageWidthCells",
            "9007199254740992",
            "whole number from 1 to 9007199254740991",
        ),
    ] {
        let mut script = Script::default();
        script.replace_input("defaultProvider", "new-provider");
        script.group("appearance");
        script.replace_input(field, invalid);
        script.tap("save");
        let tree = f.run_tall(script);
        tree.assert_value(&format!("{field}-input"), invalid);
        tree.assert_row_message(field, explanation);
        tree.assert_non_default(field, false);
        tree.assert_save_disabled();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            original,
            "{field}={invalid}"
        );
    }
}

#[test]
fn jumps_from_the_models_save_error_to_the_offscreen_invalid_input() {
    let f = Fixture::new();
    let path = f.settings();
    let original = r#"{"defaultProvider":"old-provider","terminal":{"imageWidthCells":60}}"#;
    write(&path, original);
    let field = "terminal.imageWidthCells";
    let row = "field-terminal.imageWidthCells";
    let input = "terminal.imageWidthCells-input";
    let mut script = Script::default();
    script.replace_input("defaultProvider", "new-provider");
    script.group("appearance");
    script.scroll_form(-500);
    script.replace_input(field, "0");
    // Leave the row offscreen before changing tabs, so restoring Appearance is not enough.
    script.scroll_form(1000);
    let at_top = f.run(None, script.clone());
    let [_, form_y, _, form_h] = at_top.rect("settings-form");
    assert!(
        at_top.rect(row)[1] >= form_y + form_h,
        "the invalid row must start offscreen"
    );
    script.group("models");
    script.tap("save");
    let blocked = f.run(None, script.clone());
    assert_eq!(blocked.node("group-models")["itemSelected"], true);
    blocked.assert_value("defaultProvider-input", "new-provider");
    blocked.assert_save_disabled();
    let status = blocked.node("status")["text"].as_str().unwrap();
    assert!(
        status.contains("Appearance") && status.contains("Image width (cells)"),
        "{status}"
    );
    assert_eq!(blocked.node("jump-to-invalid")["focusable"], true);
    assert_eq!(fs::read_to_string(&path).unwrap(), original);

    script.tap("jump-to-invalid");
    let jumped = f.run(None, script.clone());
    assert_eq!(jumped.node("group-appearance")["itemSelected"], true);
    jumped.assert_in_bounds(row, jumped.rect("settings-form"));
    assert!(
        jumped
            .subtree(input)
            .any(|node| node["focused"] == true && node["role"] == "textbox")
    );
    jumped.assert_value(input, "0");
    jumped.assert_save_disabled();
    assert_eq!(fs::read_to_string(&path).unwrap(), original);

    script.event(json!({"type": "key", "name": "a", "cmd": true}));
    script.event(json!({"paste": "72"}));
    script.tap("save");
    f.run(None, script).assert_message("/reload");
    assert_eq!(
        read_json(&path),
        json!({"defaultProvider": "new-provider", "terminal": {"imageWidthCells": 72}})
    );
}

#[test]
fn can_correct_or_reset_invalid_input_after_switching_groups() {
    let f = Fixture::new();
    let path = f.settings();
    let original = r#"{"editorPaddingX": 1, "unrelated": true}"#;
    write(&path, original);
    let with_invalid_edit = || {
        let mut script = Script::in_group("appearance");
        script.replace_input("editorPaddingX", "4");
        script.group("models");
        script.replace_input("defaultModel", "local/next-model");
        script
    };

    let mut script = with_invalid_edit();
    script.tap("save");
    let tree = f.run_tall(script);
    tree.assert_value("defaultModel-input", "local/next-model");
    tree.assert_save_disabled();
    assert_eq!(fs::read_to_string(&path).unwrap(), original);

    let mut script = with_invalid_edit();
    script.group("appearance");
    script.replace_input("editorPaddingX", "2");
    script.tap("save");
    f.run_tall(script).assert_message("/reload");
    assert_eq!(
        read_json(&path),
        json!({
            "editorPaddingX": 2, "unrelated": true, "defaultModel": "local/next-model"
        })
    );

    let mut script = Script::in_group("appearance");
    script.replace_input("editorPaddingX", "4");
    script.tap("editorPaddingX-clear");
    script.tap("save");
    f.run_tall(script).assert_message("/reload");
    assert_eq!(
        read_json(&path),
        json!({"unrelated": true, "defaultModel": "local/next-model"})
    );
}

fn raw_members(object: &str) -> BTreeMap<String, String> {
    serde_json::from_str::<BTreeMap<String, Box<RawValue>>>(object)
        .unwrap()
        .into_iter()
        .map(|(name, value)| (name, value.get().to_owned()))
        .collect()
}

#[test]
fn nested_edits_preserve_raw_siblings_unsupported_values_and_unsupported_parents() {
    let f = Fixture::new();
    let path = f.settings();
    let original = r#"{
        "terminal": {
            "hyperlinks": null,
            "showTerminalProgress": true,
            "future": {"big": 1234567890123456789012345678901234567890,
                       "decimal": 1.23456789012345678901234567890123456789e+400},
            "large": 9999999999999999999999999999999999999999
        },
        "images": ["future", null],
        "terminal.imageWidthCells": "literal top-level key",
        "unrelated": {"extensions": [1,2]}
    }"#;
    write(&path, original);
    let mut unchanged_roots = raw_members(original);
    let unchanged_terminal = raw_members(&unchanged_roots.remove("terminal").unwrap());
    let verify = |show_images: Option<&str>| {
        let mut roots = raw_members(&fs::read_to_string(&path).unwrap());
        let mut terminal = raw_members(&roots.remove("terminal").unwrap());
        assert_eq!(terminal.remove("imageWidthCells").as_deref(), Some("81"));
        assert_eq!(terminal.remove("showImages").as_deref(), show_images);
        assert_eq!(terminal, unchanged_terminal);
        assert_eq!(roots, unchanged_roots);
    };

    let tree = f.run_tall(Script::in_group("appearance"));
    for unsupported in [
        "images.autoResize",
        "images.blockImages",
        "terminal.hyperlinks",
    ] {
        let key = format!("{unsupported}-trigger");
        tree.assert_value(&key, "Unsupported value");
        assert_eq!(tree.node(&key)["focusable"], false, "{unsupported}");
    }
    tree.assert_save_disabled();

    let mut script = Script::in_group("appearance");
    script.select("terminal.showImages", 2);
    script.replace_input("terminal.imageWidthCells", "81");
    script.tap("save");
    f.run_tall(script).assert_message("/reload");
    verify(Some("false"));

    let mut script = Script::in_group("appearance");
    script.select("terminal.showImages", 0);
    script.tap("save");
    f.run_tall(script).assert_message("/reload");
    verify(None);
}

#[test]
fn resolves_agent_directory_and_displays_the_settings_path() {
    let f = Fixture::new();
    for (dir, path, theme) in [
        (None, f.settings(), "default-home"),
        (Some(Path::new("")), f.settings(), "default-home"),
        (
            Some(f.root.as_path()),
            f.root.join("settings.json"),
            "absolute-dir",
        ),
        (
            Some(Path::new("~/custom")),
            f.home.join("custom/settings.json"),
            "custom-home",
        ),
        (
            Some(Path::new("relative/agent")),
            f.cwd.join("relative/agent/settings.json"),
            "relative-dir",
        ),
    ] {
        write(&path, json!({"theme": theme}).to_string());
        let tree = f.run(dir, Script::in_group("appearance"));
        tree.assert_path(&path);
        tree.assert_value("theme-input", theme);
    }
}

#[test]
fn titlebar_path_keeps_clear_of_the_macos_window_buttons() {
    let f = Fixture::new();
    let mut run = f.prepare(None, Script::default());
    run.command.args(["--chrome", "--size", "500x430"]);
    let output = run.command.output().unwrap();
    let tree = run.read_tree(&output);
    let log = String::from_utf8_lossy(&output.stderr);
    assert!(log.contains("0 overlapping elements"), "{log}");
    assert!(
        tree.elements()
            .iter()
            .all(|node| node["text"] != "Pi Config App" && node["text"] != "USER SETTINGS FILE")
    );
    tree.assert_in_bounds("settings-path", tree.rect("titlebar"));
    let [_, bar_y, _, bar_h] = tree.rect("titlebar");
    assert!(tree.rect("group-models")[1] >= bar_y + bar_h);
}

#[test]
fn hovering_over_the_top_right_path_shows_the_copy_hint() {
    let f = Fixture::new();
    let mut script = Script::default();
    script.event(json!({"type": "move", "key": "settings-path"}));
    script.ticks += 15;
    let tree = f.run(None, script);
    assert_eq!(tree.0["tooltip"]["text"], "Click to copy the full path");
}

#[test]
fn long_path_is_clickable_in_the_titlebar_without_hiding_fields_or_save() {
    let f = Fixture::new();
    let dir = (0..7).fold(f.root.clone(), |dir, _| {
        dir.join("portable-settings-".repeat(6))
    });
    let path = dir.join("settings.json");
    let original = r#"{"defaultProvider":"old-provider"}"#;
    write(&path, original);
    let past = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(past)
        .unwrap();

    let mut script = Script::default();
    script.replace_input("defaultProvider", "unsaved-provider");
    script.tap("settings-path");
    let mut run = f.prepare(Some(&dir), script);
    run.command.args(["--size", "500x430"]);
    let tree = run.finish();

    assert_eq!(tree.0["viewport"], json!([500.0, 430.0]));
    let window = [0.0, 0.0, 500.0, 430.0];
    let path_node = tree.node("settings-path");
    assert_eq!(path_node["text"], path.to_str().unwrap());
    assert_eq!(
        tree.accessible_node("settings-path")["name"],
        path.to_str().unwrap()
    );
    assert_eq!(path_node["rect"][3], path_node["textStyle"]["lineHeight"]);
    assert_eq!(path_node["role"], "button");
    assert_eq!(path_node["focused"], true);
    assert!(
        tree.elements()
            .iter()
            .all(|node| node["text"] != "Copy path")
    );
    tree.assert_in_bounds("settings-path", tree.rect("titlebar"));
    let [path_x, _, path_w, _] = tree.rect("settings-path");
    assert!((path_x + path_w - 480.0).abs() < 0.02);
    for key in ["settings-path", "settings-form", "save"] {
        tree.assert_in_bounds(key, window);
    }
    let form = tree.rect("settings-form");
    for &(field, _) in COVERAGE[0].1 {
        tree.assert_in_bounds(&format!("field-{field}"), form);
    }
    tree.assert_value("defaultProvider-input", "unsaved-provider");
    assert_eq!(tree.node("save")["focusable"], true);
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), past);
}

#[test]
fn footer_keeps_status_and_actions_on_one_row_at_minimum_width() {
    let f = Fixture::new();
    write(&f.settings(), "{}");
    let mut invalid = Script::in_group("appearance");
    invalid.scroll_form(-500);
    invalid.replace_input("terminal.imageWidthCells", "0");
    for (script, message, buttons) in [
        (
            Script::default(),
            "No unsaved changes",
            &["reload", "save"][..],
        ),
        (
            invalid,
            "Fix Image width (cells) in Appearance before saving.",
            &["jump-to-invalid", "reload", "save"][..],
        ),
    ] {
        let mut run = f.prepare(None, script);
        run.command.args(["--size", "500x430"]);
        let tree = run.finish();
        let footer = tree.rect("settings-footer");
        tree.assert_in_bounds("settings-footer", [0.0, 0.0, 500.0, 430.0]);
        tree.assert_in_bounds("status", footer);
        assert_eq!(tree.node("status")["text"], message);
        let [x, y, w, h] = tree.rect("status");
        let mut right = x + w;
        let center_y = y + h / 2.0;
        for &button in buttons {
            tree.assert_in_bounds(button, footer);
            let [x, y, w, h] = tree.rect(button);
            assert!(x >= right, "{button} overlaps the preceding element");
            assert!(
                (y + h / 2.0 - center_y).abs() <= 0.02,
                "{button} is not centered on the status row"
            );
            right = x + w;
        }
    }
}

#[test]
fn a_custom_path_read_failure_does_not_fall_back_to_the_default_file() {
    let f = Fixture::new();
    let good = r#"{"theme":"default-must-not-load"}"#;
    write(&f.settings(), good);
    let custom = f.root.join("custom");
    write(&custom, "This is a file, not a directory");

    let mut script = Script::in_group("appearance");
    script.tap("save");
    let tree = f.run(Some(&custom), script);
    tree.assert_path(&custom.join("settings.json"));
    tree.assert_message("Not a directory");
    tree.assert_save_disabled();
    tree.node("theme-input");
    assert!(
        !tree
            .elements()
            .iter()
            .any(|e| e["text"] == "default-must-not-load")
    );
    assert_eq!(fs::read_to_string(f.settings()).unwrap(), good);
    assert_eq!(
        fs::read_to_string(custom).unwrap(),
        "This is a file, not a directory"
    );
}

#[test]
fn theme_save_and_clear_preserve_unsupported_and_unrelated_settings() {
    let f = Fixture::new();
    let path = f.settings();
    let original = r#"{
        "theme": "local-custom",
        "hideThinkingBlock": "future-mode",
        "packages": ["npm:local-tools", {"source":"git:local/repo","extensions":["one"]}],
        "unknown": {"nested": [true, null, {"name":"keep me"}]},
        "precise": {"integer": 1234567890123456789012345678901234567890,
                    "decimal": 1.23456789012345678901234567890123456789e+400}
    }"#;
    write(&path, original);
    f.run(None, Script::in_group("appearance"))
        .assert_value("theme-input", "local-custom");
    let initial = f.run(None, Script::default());
    initial.assert_value("thinking-trigger", DEFAULT);
    initial.assert_message("Unsupported value");

    let mut script = Script::default();
    script.replace_theme("another-custom");
    script.tap("save");
    f.run(None, script).assert_message("/reload");
    assert_document(&path, original, Some("another-custom"));

    // The next process must read its initial form value from the saved file.
    f.run(None, Script::in_group("appearance"))
        .assert_value("theme-input", "another-custom");
    let mut script = Script::in_group("appearance");
    script.tap("theme-clear");
    script.tap("save");
    f.run(None, script);
    assert_document(&path, original, None);
}

fn assert_document(path: &Path, original: &str, theme: Option<&str>) {
    type RawObject = BTreeMap<String, Box<RawValue>>;
    let mut expected: RawObject = serde_json::from_str(original).unwrap();
    let mut actual: RawObject = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    expected.remove("theme");
    let saved_theme = actual
        .remove("theme")
        .map(|value| serde_json::from_str::<String>(value.get()).unwrap());
    assert_eq!(saved_theme.as_deref(), theme);
    let numbers = |document: &mut RawObject| -> BTreeMap<String, String> {
        serde_json::from_str::<RawObject>(document.remove("precise").unwrap().get())
            .unwrap()
            .into_iter()
            .map(|(key, value)| (key, value.get().to_owned()))
            .collect()
    };
    assert_eq!(numbers(&mut actual), numbers(&mut expected));
    let parse = |document: RawObject| -> BTreeMap<String, Value> {
        document
            .into_iter()
            .map(|(key, value)| (key, serde_json::from_str(value.get()).unwrap()))
            .collect()
    };
    assert_eq!(parse(actual), parse(expected));
}

#[test]
fn select_menus_show_pi_choices_and_save_or_clear_the_corresponding_keys() {
    let f = Fixture::new();
    let path = f.settings();
    write(&path, "{}");
    let mut script = Script::default();
    script.tap("thinking-trigger");
    f.run(None, script).assert_options(
        "thinking-content",
        &[
            DEFAULT, "off", "minimal", "low", "medium", "high", "xhigh", "max",
        ],
    );
    let mut script = Script::default();
    script.tap("hide-trigger");
    f.run(None, script)
        .assert_options("hide-content", &[DEFAULT, "Show", "Hide"]);

    let mut script = Script::default();
    script.select("thinking", 5);
    script.select("hide", 2);
    script.tap("save");
    let tree = f.run(None, script);
    tree.assert_value("thinking-trigger", "high");
    tree.assert_value("hide-trigger", "Hide");
    assert_eq!(
        read_json(&path),
        json!({"defaultThinkingLevel":"high","hideThinkingBlock":true})
    );

    let mut script = Script::default();
    script.select("thinking", 0);
    script.select("hide", 1);
    script.tap("save");
    assert_eq!(f.run(None, script).node("save")["focusable"], false);
    assert_eq!(read_json(&path), json!({"hideThinkingBlock":false}));

    let mut script = Script::default();
    script.select("hide", 0);
    script.tap("save");
    let tree = f.run(None, script);
    tree.assert_value("thinking-trigger", DEFAULT);
    tree.assert_value("hide-trigger", DEFAULT);
    assert_eq!(read_json(&path), json!({}));
}

#[test]
fn neither_a_clean_save_nor_closing_with_edits_writes_the_file() {
    let f = Fixture::new();
    let path = f.settings();
    let original = r#"{ "theme"  : "original" }"#;
    write(&path, original);
    let past = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(past)
        .unwrap();
    let unchanged = || {
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), past);
    };
    f.run(None, program_attempts_save()).assert_save_disabled();
    unchanged();

    let mut script = Script::default();
    script.replace_theme("not-saved");
    script.event(json!({"close_request": true}));
    f.run(None, script);
    unchanged();
}

#[test]
fn creates_missing_file_and_directories_only_when_saving_an_edit() {
    let f = Fixture::new();
    let path = f.settings();
    let missing = f.run(None, program_attempts_save());
    missing.assert_path(&path);
    missing.assert_message("Not created yet");
    missing.assert_save_disabled();
    assert!(!f.home.join(".pi").exists());

    let mut script = Script::default();
    script.replace_theme("first-theme");
    script.tap("save");
    f.run(None, script).assert_message("/reload");
    assert_eq!(read_json(&path), json!({"theme":"first-theme"}));
}

#[test]
fn can_save_twice_without_reloading_after_reading_a_utf8_bom() {
    let f = Fixture::new();
    let path = f.settings();
    write(&path, b"\xef\xbb\xbf{\"theme\":\"original\"}");
    f.run(None, Script::in_group("appearance"))
        .assert_value("theme-input", "original");

    let mut script = Script::default();
    script.replace_theme("first-save");
    script.tap("save");
    script.group("models");
    script.select("hide", 2);
    script.tap("save");
    f.run(None, script).assert_message("/reload");
    assert_eq!(
        read_json(&path),
        json!({"theme":"first-save","hideThinkingBlock":true})
    );
}

#[test]
fn rejects_invalid_json_and_non_object_documents_without_overwriting_them() {
    let f = Fixture::new();
    let path = f.settings();
    for (original, reason) in [
        (r#"{"theme": }"#, "Invalid JSON"),
        (r#"["not an object"]"#, "JSON object"),
    ] {
        write(&path, original);
        let tree = f.run(None, program_attempts_save());
        tree.assert_path(&path);
        tree.assert_message(reason);
        tree.assert_save_disabled();
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }
}

#[cfg(unix)]
#[test]
fn rejects_permission_denied_without_overwriting_the_file() {
    use std::os::unix::fs::PermissionsExt;

    let f = Fixture::new();
    let path = f.settings();
    let original = r#"{"theme":"protected"}"#;
    write(&path, original);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
    if File::open(&path).is_ok() {
        // Privileged users can still read mode-000 files. The custom-path test exercises
        // a read failure even for these users.
        return;
    }
    let tree = f.run(None, program_attempts_save());
    tree.assert_path(&path);
    tree.assert_message("Permission denied");
    tree.assert_save_disabled();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(fs::read_to_string(path).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn rejects_a_symbolic_link_without_replacing_it_or_writing_its_target() {
    use std::os::unix::fs::symlink;

    let f = Fixture::new();
    let path = f.settings();
    let target = f.root.join("dotfiles/settings.json");
    let original = r#"{"theme":"symlinked"}"#;
    write(&target, original);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    symlink(&target, &path).unwrap();

    let tree = f.run(None, program_attempts_save());
    tree.assert_path(&path);
    tree.assert_message("symbolic link");
    tree.assert_save_disabled();
    assert_eq!(fs::read_link(path).unwrap(), target);
    assert_eq!(fs::read_to_string(target).unwrap(), original);
}

#[test]
fn reload_reads_a_file_repaired_after_launch() {
    let f = Fixture::new();
    let path = f.settings();
    write(&path, "{ broken }");
    let mut script = Script::default();
    script.external_mark();
    script.tap("reload");
    script.group("appearance");
    let tree = f
        .prepare(None, script)
        .on_external_mark(|| write(&path, r#"{"theme":"repaired"}"#));
    tree.assert_value("theme-input", "repaired");
    tree.assert_save_disabled();
}

#[test]
fn reload_requires_confirmation_and_cancel_keeps_unsaved_edits() {
    let f = Fixture::new();
    let path = f.settings();
    let original = r#"{"theme":"from-disk"}"#;
    write(&path, original);
    for (action, expected) in [
        ("cancel-reload", "unsaved"),
        ("discard-and-reload", "from-disk"),
    ] {
        let mut script = Script::default();
        script.replace_theme("unsaved");
        script.tap("reload");
        script.tap(action);
        script.group("appearance");
        let tree = f.run(None, script);
        tree.assert_value("theme-input", expected);
        assert_eq!(tree.node("save")["focusable"], action == "cancel-reload");
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }
}

#[test]
fn save_refuses_external_modification_creation_and_deletion() {
    for (initial, external) in [
        (Some(r#"{"theme":"old"}"#), Some(r#"{"theme":"external"}"#)),
        (None, Some(r#"{"theme":"created-externally"}"#)),
        (Some(r#"{"theme":"old"}"#), None),
    ] {
        let f = Fixture::new();
        let path = f.settings();
        if let Some(original) = initial {
            write(&path, original);
        }
        let mut script = Script::default();
        script.replace_theme("our-edit");
        script.external_mark();
        script.tap("save");
        let tree = f.prepare(None, script).on_external_mark(|| match external {
            Some(content) => write(&path, content),
            None => fs::remove_file(&path).unwrap(),
        });
        tree.assert_message("changed on disk");
        tree.assert_value("theme-input", "our-edit");
        match external {
            Some(content) => assert_eq!(fs::read_to_string(&path).unwrap(), content),
            None => assert!(!path.exists()),
        }
    }
}

#[cfg(unix)]
#[test]
fn a_failed_write_preserves_the_original_and_the_unsaved_edit() {
    use std::os::unix::fs::PermissionsExt;

    let f = Fixture::new();
    let path = f.settings();
    let dir = path.parent().unwrap();
    let original = r#"{"theme":"old","unknown":[null,17]}"#;
    write(&path, original);
    // Deny new temporary files but keep the original readable.
    let permissions = fs::metadata(dir).unwrap().permissions();
    fs::set_permissions(dir, fs::Permissions::from_mode(0o500)).unwrap();
    let probe = dir.join("write-probe");
    match File::create(&probe) {
        Ok(_) => {
            // A privileged test process can bypass directory permissions.
            fs::set_permissions(dir, permissions).unwrap();
            return;
        }
        Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied),
    }

    let mut script = Script::default();
    script.replace_theme("unsaved-after-failure");
    script.tap("save");
    let tree = f.run(None, script);
    fs::set_permissions(dir, permissions).unwrap();
    tree.assert_message("Could not save");
    tree.assert_message("Permission denied");
    tree.assert_value("theme-input", "unsaved-after-failure");
    assert_eq!(tree.node("save")["focusable"], true);
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert_eq!(fs::read_dir(dir).unwrap().count(), 1);
}

#[test]
fn saving_only_changes_user_settings_without_running_a_settings_command() {
    let f = Fixture::new();
    let settings = f.settings();
    let auth = settings.with_file_name("auth.json");
    let project = f.cwd.join(".pi/settings.json");
    let canary = f.cwd.join("ran-settings-command");
    let credentials = r#"{"fake-provider":{"key":"test-only-secret"}}"#;
    let project_settings = r#"{"theme":"project-override"}"#;
    let unknown = json!({"command":"touch ran-settings-command"});
    write(&settings, unknown.to_string());
    write(&auth, credentials);
    write(&project, project_settings);
    #[cfg(target_os = "linux")]
    let watches = watch_file_opens(&[&auth, &project]);

    let mut script = Script::default();
    script.replace_theme("user-only");
    script.tap("save");
    f.run(None, script).assert_message("/reload");

    #[cfg(target_os = "linux")]
    {
        let mut byte = [0; 4096];
        assert!(
            matches!((&watches).read(&mut byte), Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock),
            "the app opened auth.json or project settings"
        );
    }
    assert_eq!(fs::read_to_string(auth).unwrap(), credentials);
    assert_eq!(fs::read_to_string(project).unwrap(), project_settings);
    assert!(!canary.exists());
    assert_eq!(
        read_json(&settings),
        json!({"command":unknown["command"], "theme":"user-only"})
    );
    assert_eq!(fs::read_dir(settings.parent().unwrap()).unwrap().count(), 2);
}

#[cfg(target_os = "linux")]
fn watch_file_opens(paths: &[&Path]) -> File {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};

    // SAFETY: inotify_init1 returns a newly owned descriptor on success.
    let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
    assert!(fd >= 0, "{}", std::io::Error::last_os_error());
    // SAFETY: fd is valid and uniquely owned by this test from here on.
    let watches = unsafe { File::from_raw_fd(fd) };
    for path in paths {
        let path = CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: the descriptor stays open and the C string lives through the call.
        let watch =
            unsafe { libc::inotify_add_watch(watches.as_raw_fd(), path.as_ptr(), libc::IN_OPEN) };
        assert!(watch >= 0, "{}", std::io::Error::last_os_error());
    }
    watches
}
