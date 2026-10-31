//! Typed Render workspace UI requests; headless calls only echo validated inputs.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CheckReport {
    /// Report text, bounded to 65536 UTF-8 bytes by the command handler.
    #[schemars(length(min = 1, max = 65536))]
    pub text: String,
}

/// Explicit provisional-preview action; no implicit toggle or default activation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    On,
    Off,
    Pause,
    Resume,
}

/// A validated UI action echoed unchanged by the headless command handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InteractiveParams {
    pub action: Action,
}
