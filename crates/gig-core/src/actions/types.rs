use serde::ser::Serializer;
use serde::Serialize;
use std::fmt;

/// Stable string-backed identifier for a gig action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActionId(&'static str);

impl ActionId {
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for ActionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl Serialize for ActionId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.0)
    }
}

/// Coarse execution class used by CLI adapters and GUI confirmation UX.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Read,
    Mutate,
    ExternalIo,
    Dangerous,
}

/// Primitive field schema exposed to adapters before action payloads are wired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ActionField {
    pub name: &'static str,
    pub label: &'static str,
    pub kind: ActionFieldKind,
    pub required: bool,
    pub repeated: bool,
    pub help: Option<&'static str>,
}

impl ActionField {
    pub const fn required(name: &'static str, label: &'static str, kind: ActionFieldKind) -> Self {
        Self {
            name,
            label,
            kind,
            required: true,
            repeated: false,
            help: None,
        }
    }

    pub const fn optional(name: &'static str, label: &'static str, kind: ActionFieldKind) -> Self {
        Self {
            name,
            label,
            kind,
            required: false,
            repeated: false,
            help: None,
        }
    }

    pub const fn repeated(name: &'static str, label: &'static str, kind: ActionFieldKind) -> Self {
        Self {
            name,
            label,
            kind,
            required: true,
            repeated: true,
            help: None,
        }
    }

    pub const fn with_help(mut self, help: &'static str) -> Self {
        self.help = Some(help);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionFieldKind {
    String,
    Integer,
    Decimal,
    Boolean,
    Path,
    PathList,
    Enum { values: &'static [&'static str] },
}

/// Core-level confirmation contract. Core actions return this metadata instead
/// of blocking on stdin or opening editors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConfirmationPolicy {
    None,
    Required { prompt: &'static str },
}

/// Optional preflight result for future executable actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActionPreflight {
    pub confirmation: ConfirmationPolicy,
    pub warnings: Vec<String>,
    pub blocked_reason: Option<String>,
}

impl ActionPreflight {
    pub fn ready(confirmation: ConfirmationPolicy) -> Self {
        Self {
            confirmation,
            warnings: Vec::new(),
            blocked_reason: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffect {
    ReadsDatabase,
    WritesDatabase,
    ReadsFiles,
    WritesFiles,
    MovesFiles,
    DeletesFiles,
    OpensEditor,
    ExternalNetwork,
    UploadsFiles,
    StartsLocalServer,
    PrintsCompletion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionOutputKind {
    Json,
    Text,
    File,
    FilePath,
    LocalServer,
}

/// Static action metadata shared by adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ActionMeta {
    pub id: ActionId,
    pub cli_path: &'static [&'static str],
    pub label: &'static str,
    pub description: &'static str,
    pub kind: ActionKind,
    pub fields: &'static [ActionField],
    pub confirmation: ConfirmationPolicy,
    pub side_effects: &'static [SideEffect],
    pub output: ActionOutputKind,
    pub json_supported: bool,
    pub workflow_critical: bool,
}
