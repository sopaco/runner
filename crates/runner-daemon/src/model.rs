pub use runner_core::protocol::model::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchDefinition {
    pub command: String,
    pub args: Vec<String>,
    pub env: std::collections::BTreeMap<String, String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub codex_speed: Option<CodexSpeed>,
    pub system_prompt: Option<String>,
}
