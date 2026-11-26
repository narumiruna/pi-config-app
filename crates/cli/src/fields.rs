use serde_json::{Value, json};

pub const DEFAULT: &str = "Use Pi default";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Models,
    Interaction,
    Appearance,
    Updates,
}

pub const GROUPS: &[(Group, &str, &str)] = &[
    (Group::Models, "Models", "group-models"),
    (Group::Interaction, "Interaction", "group-interaction"),
    (Group::Appearance, "Appearance", "group-appearance"),
    (Group::Updates, "Updates", "group-updates"),
];

#[derive(Clone, Copy)]
pub enum Atom {
    Text(&'static str),
    Bool(bool),
    Number(u64),
}

impl Atom {
    pub fn value(self) -> Value {
        match self {
            Self::Text(value) => json!(value),
            Self::Bool(value) => json!(value),
            Self::Number(value) => json!(value),
        }
    }
}

#[derive(Clone, Copy)]
pub enum Input {
    Text { allow_empty: bool },
    Whole { min: u64, max: u64 },
    WheelLines,
}

impl Input {
    pub fn display(self, value: &Value) -> Option<String> {
        let text = match value {
            Value::String(value) => value.to_owned(),
            Value::Number(value) => value.to_string(),
            _ => return None,
        };
        (self.parse(&text).ok().flatten().as_ref() == Some(value)).then_some(text)
    }

    pub fn parse(self, text: &str) -> Result<Option<Value>, String> {
        let number = |min, max| {
            text.parse::<u64>()
                .ok()
                .filter(|n| (min..=max).contains(n))
                .map(|n| Some(json!(n)))
        };
        match self {
            Self::Text { allow_empty } => {
                if text.contains(['\r', '\n']) {
                    Err("Enter a single line of text.".into())
                } else {
                    Ok((allow_empty || !text.is_empty()).then(|| json!(text)))
                }
            }
            Self::Whole { min, max } => {
                number(min, max).ok_or_else(|| format!("Enter a whole number from {min} to {max}."))
            }
            Self::WheelLines if text == "auto" => Ok(Some(json!("auto"))),
            Self::WheelLines => {
                number(1, 100).ok_or_else(|| "Enter auto or a whole number from 1 to 100.".into())
            }
        }
    }
}

#[derive(Clone, Copy)]
pub enum Kind {
    Choice(&'static [(&'static str, Atom)]),
    Input(Input),
}

pub struct Spec {
    pub path: &'static str,
    pub label: &'static str,
    pub group: Group,
    pub kind: Kind,
}

impl Spec {
    pub fn control_id(&self) -> &'static str {
        match self.path {
            "defaultThinkingLevel" => "thinking",
            "hideThinkingBlock" => "hide",
            path => path,
        }
    }
}

use Atom::{Bool as B, Number as N, Text as S};
use Group::*;
use Input::{Text, WheelLines, Whole};
use Kind::{Choice, Input as Entry};

const ON_OFF: Kind = Choice(&[("On", B(true)), ("Off", B(false))]);
const AUTO_BOOL: Kind = Choice(&[("On", B(true)), ("Off", B(false)), ("auto", S("auto"))]);
const TEXT: Kind = Entry(Text { allow_empty: false });

macro_rules! choices {
    ($($value:literal),+ $(,)?) => { Choice(&[$(($value, S($value))),+]) };
}

macro_rules! field {
    ($group:ident, $path:literal, $label:literal, $kind:expr) => {
        Spec {
            group: $group,
            path: $path,
            label: $label,
            kind: $kind,
        }
    };
}

const QUEUE: Kind = choices!("all", "one-at-a-time");

pub const FIELDS: &[Spec] = &[
    field!(Models, "defaultProvider", "Default provider", TEXT),
    field!(Models, "defaultModel", "Default model ID", TEXT),
    field!(
        Models,
        "defaultThinkingLevel",
        "Default thinking level",
        choices!("off", "minimal", "low", "medium", "high", "xhigh", "max")
    ),
    field!(
        Models,
        "hideThinkingBlock",
        "Thinking block",
        Choice(&[("Show", B(false)), ("Hide", B(true))])
    ),
    field!(Models, "showCacheMissNotices", "Cache miss notices", ON_OFF),
    field!(Interaction, "steeringMode", "Steering messages", QUEUE),
    field!(Interaction, "followUpMode", "Follow-up messages", QUEUE),
    field!(
        Interaction,
        "doubleEscapeAction",
        "Double Escape action",
        choices!("tree", "fork", "none")
    ),
    field!(
        Interaction,
        "treeFilterMode",
        "Initial tree filter",
        choices!("default", "no-tools", "user-only", "labeled-only", "all")
    ),
    field!(Appearance, "theme", "Theme", TEXT),
    field!(
        Appearance,
        "quietStartup",
        "Quiet startup",
        Choice(&[
            ("Hide startup", B(true)),
            ("Show startup", B(false)),
            ("Header only", S("header"))
        ])
    ),
    field!(
        Appearance,
        "tuiMode",
        "Terminal UI mode",
        choices!("regular", "fullscreen")
    ),
    field!(
        Appearance,
        "fullscreenExitOutput",
        "Fullscreen exit output",
        choices!("transcript", "resume-hint")
    ),
    field!(
        Appearance,
        "fullscreenScrollbar",
        "Fullscreen scrollbar",
        choices!("auto", "always", "hidden")
    ),
    field!(
        Appearance,
        "fullscreenCopyOnSelect",
        "Fullscreen copy on select",
        ON_OFF
    ),
    field!(
        Appearance,
        "fullscreenWheelScrollLines",
        "Fullscreen wheel lines",
        Entry(WheelLines)
    ),
    field!(
        Appearance,
        "editorPaddingX",
        "Editor padding (cells)",
        Entry(Whole { min: 0, max: 3 })
    ),
    field!(
        Appearance,
        "outputPad",
        "Transcript padding (cells)",
        Choice(&[("0", N(0)), ("1", N(1))])
    ),
    field!(
        Appearance,
        "autocompleteMaxVisible",
        "Maximum autocomplete entries",
        Entry(Whole { min: 3, max: 20 })
    ),
    field!(Appearance, "showHardwareCursor", "Terminal cursor", ON_OFF),
    field!(
        Appearance,
        "terminal.showImages",
        "Show images in terminal",
        ON_OFF
    ),
    field!(
        Appearance,
        "terminal.imageWidthCells",
        "Image width (cells)",
        Entry(Whole {
            min: 1,
            max: 9_007_199_254_740_991
        })
    ),
    field!(
        Appearance,
        "terminal.clearOnShrink",
        "Clear terminal on shrink",
        ON_OFF
    ),
    field!(
        Appearance,
        "terminal.showTerminalProgress",
        "Show terminal progress",
        ON_OFF
    ),
    field!(
        Appearance,
        "terminal.hyperlinks",
        "Terminal hyperlinks",
        AUTO_BOOL
    ),
    field!(
        Appearance,
        "terminal.images",
        "Terminal image protocol",
        Choice(&[
            ("kitty", S("kitty")),
            ("iterm2", S("iterm2")),
            ("auto", S("auto")),
            ("Off", B(false))
        ])
    ),
    field!(
        Appearance,
        "terminal.trueColor",
        "Terminal true color",
        AUTO_BOOL
    ),
    field!(
        Appearance,
        "images.autoResize",
        "Resize images sent to models",
        ON_OFF
    ),
    field!(
        Appearance,
        "images.blockImages",
        "Block images sent to models",
        ON_OFF
    ),
    field!(
        Appearance,
        "markdown.codeBlockIndent",
        "Markdown code block indent",
        Entry(Text { allow_empty: true })
    ),
    field!(
        Appearance,
        "markdown.mermaid",
        "Mermaid rendering",
        choices!("off", "final", "streaming")
    ),
    field!(
        Updates,
        "collapseChangelog",
        "Condense the update changelog",
        ON_OFF
    ),
];
