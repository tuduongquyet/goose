use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::config::extensions::ExtensionEntry;
use crate::config::goose_mode::GooseMode;
use crate::slash_commands::SlashCommandMapping;

/// JSON Schema representation of Goose's config.yaml.
///
/// All keys are optional. Unknown keys are allowed (additionalProperties: true)
/// because Goose passes undocumented provider-specific keys through as
/// environment variable overrides.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GooseConfigSchema {
    // === Core Goose Settings ===
    #[serde(rename = "GOOSE_PROVIDER")]
    pub goose_provider: Option<String>,
    #[serde(rename = "GOOSE_MODEL")]
    pub goose_model: Option<String>,
    #[serde(rename = "GOOSE_MODE")]
    pub goose_mode: Option<GooseMode>,
    #[serde(rename = "GOOSE_MAX_TOKENS")]
    pub goose_max_tokens: Option<i32>,
    #[serde(rename = "GOOSE_CONTEXT_LIMIT")]
    pub goose_context_limit: Option<u64>,
    #[serde(rename = "GOOSE_INPUT_LIMIT")]
    pub goose_input_limit: Option<u64>,
    #[serde(rename = "GOOSE_MAX_TURNS")]
    pub goose_max_turns: Option<u32>,
    #[serde(rename = "GOOSE_MAX_ACTIVE_AGENTS")]
    pub goose_max_active_agents: Option<u64>,
    #[serde(rename = "GOOSE_AUTO_COMPACT_THRESHOLD")]
    pub goose_auto_compact_threshold: Option<f64>,
    #[serde(rename = "GOOSE_TOOL_PAIR_SUMMARIZATION")]
    pub goose_tool_pair_summarization: Option<bool>,
    #[serde(rename = "GOOSE_TOOL_CALL_CUTOFF")]
    pub goose_tool_call_cutoff: Option<u64>,
    #[serde(rename = "GOOSE_STREAM_TIMEOUT")]
    pub goose_stream_timeout: Option<u64>,
    #[serde(rename = "GOOSE_SEARCH_PATHS")]
    pub goose_search_paths: Option<Vec<String>>,
    #[serde(rename = "GOOSE_DISABLE_SESSION_NAMING")]
    pub goose_disable_session_naming: Option<bool>,
    #[serde(rename = "GOOSE_DISABLE_KEYRING")]
    pub goose_disable_keyring: Option<bool>,
    #[serde(rename = "GOOSE_TELEMETRY_ENABLED")]
    pub goose_telemetry_enabled: Option<bool>,
    #[serde(rename = "GOOSE_DEFAULT_EXTENSION_TIMEOUT")]
    pub goose_default_extension_timeout: Option<u64>,
    #[serde(rename = "GOOSE_PROMPT_EDITOR")]
    pub goose_prompt_editor: Option<String>,
    #[serde(rename = "GOOSE_PROMPT_EDITOR_ALWAYS")]
    pub goose_prompt_editor_always: Option<bool>,
    #[serde(rename = "GOOSE_ALLOWLIST")]
    pub goose_allowlist: Option<String>,
    #[serde(rename = "GOOSE_SYSTEM_PROMPT_FILE_PATH")]
    pub goose_system_prompt_file_path: Option<String>,
    #[serde(rename = "GOOSE_DEBUG")]
    pub goose_debug: Option<bool>,
    #[serde(rename = "GOOSE_SHOW_FULL_OUTPUT")]
    pub goose_show_full_output: Option<bool>,
    #[serde(rename = "GOOSE_STATUS_HOOK")]
    pub goose_status_hook: Option<String>,
    #[serde(rename = "GOOSE_LOCAL_ENABLE_THINKING")]
    pub goose_local_enable_thinking: Option<bool>,
    #[serde(rename = "GOOSE_DATABRICKS_CLIENT_REQUEST_ID")]
    pub goose_databricks_client_request_id: Option<bool>,
    #[serde(rename = "CONTEXT_FILE_NAMES")]
    pub context_file_names: Option<Vec<String>>,
    #[serde(rename = "EDIT_MODE")]
    pub edit_mode: Option<String>,
    #[serde(rename = "RANDOM_THINKING_MESSAGES")]
    pub random_thinking_messages: Option<bool>,
    #[serde(rename = "CODE_MODE_TOOL_DISCLOSURE")]
    pub code_mode_tool_disclosure: Option<bool>,

    // === mTLS Settings ===
    #[serde(rename = "GOOSE_CLIENT_CERT_PATH")]
    pub goose_client_cert_path: Option<String>,
    #[serde(rename = "GOOSE_CLIENT_KEY_PATH")]
    pub goose_client_key_path: Option<String>,
    #[serde(rename = "GOOSE_CA_CERT_PATH")]
    pub goose_ca_cert_path: Option<String>,

    // === Planner & Subagent Settings ===
    #[serde(rename = "GOOSE_PLANNER_PROVIDER")]
    pub goose_planner_provider: Option<String>,
    #[serde(rename = "GOOSE_PLANNER_MODEL")]
    pub goose_planner_model: Option<String>,
    #[serde(rename = "GOOSE_SUBAGENT_PROVIDER")]
    pub goose_subagent_provider: Option<String>,
    #[serde(rename = "GOOSE_SUBAGENT_MODEL")]
    pub goose_subagent_model: Option<String>,
    #[serde(rename = "GOOSE_SUBAGENT_MAX_TURNS")]
    pub goose_subagent_max_turns: Option<u64>,
    #[serde(rename = "GOOSE_MAX_BACKGROUND_TASKS")]
    pub goose_max_background_tasks: Option<u64>,

    // === Recipe Settings ===
    #[serde(rename = "GOOSE_RECIPE_GITHUB_REPO")]
    pub goose_recipe_github_repo: Option<String>,
    #[serde(rename = "GOOSE_RECIPE_RETRY_TIMEOUT_SECONDS")]
    pub goose_recipe_retry_timeout_seconds: Option<u64>,
    #[serde(rename = "GOOSE_RECIPE_ON_FAILURE_TIMEOUT_SECONDS")]
    pub goose_recipe_on_failure_timeout_seconds: Option<u64>,

    // === CLI Settings ===
    #[serde(rename = "GOOSE_CLI_MIN_PRIORITY")]
    pub goose_cli_min_priority: Option<f32>,
    #[serde(rename = "GOOSE_CLI_THEME")]
    pub goose_cli_theme: Option<String>,
    #[serde(rename = "GOOSE_CLI_LIGHT_THEME")]
    pub goose_cli_light_theme: Option<String>,
    #[serde(rename = "GOOSE_CLI_DARK_THEME")]
    pub goose_cli_dark_theme: Option<String>,
    #[serde(rename = "GOOSE_CLI_SHOW_COST")]
    pub goose_cli_show_cost: Option<bool>,
    #[serde(rename = "GOOSE_CLI_SHOW_THINKING")]
    pub goose_cli_show_thinking: Option<bool>,
    #[serde(rename = "GOOSE_CLI_NEWLINE_KEY")]
    pub goose_cli_newline_key: Option<String>,

    // === AI Agent / Thinking Settings ===
    #[serde(rename = "CLAUDE_CODE_COMMAND")]
    pub claude_code_command: Option<String>,
    #[serde(rename = "GEMINI_CLI_COMMAND")]
    pub gemini_cli_command: Option<String>,
    #[serde(rename = "CURSOR_AGENT_COMMAND")]
    pub cursor_agent_command: Option<String>,
    #[serde(rename = "CODEX_COMMAND")]
    pub codex_command: Option<String>,
    #[serde(rename = "CODEX_REASONING_EFFORT")]
    pub codex_reasoning_effort: Option<String>,
    #[serde(rename = "CODEX_ENABLE_SKILLS")]
    pub codex_enable_skills: Option<String>,
    #[serde(rename = "CODEX_SKIP_GIT_CHECK")]
    pub codex_skip_git_check: Option<String>,
    #[serde(rename = "CHATGPT_CODEX_REASONING_EFFORT")]
    pub chatgpt_codex_reasoning_effort: Option<String>,
    #[serde(rename = "CLAUDE_THINKING_TYPE")]
    pub claude_thinking_type: Option<String>,
    #[serde(rename = "CLAUDE_THINKING_EFFORT")]
    pub claude_thinking_effort: Option<String>,
    #[serde(rename = "CLAUDE_THINKING_BUDGET")]
    pub claude_thinking_budget: Option<i32>,
    #[serde(rename = "GEMINI3_THINKING_LEVEL")]
    pub gemini3_thinking_level: Option<String>,
    #[serde(rename = "GEMINI25_THINKING_BUDGET")]
    pub gemini25_thinking_budget: Option<i32>,

    // === Security Settings ===
    #[serde(rename = "SECURITY_PROMPT_ENABLED")]
    pub security_prompt_enabled: Option<bool>,
    #[serde(rename = "SECURITY_PROMPT_THRESHOLD")]
    pub security_prompt_threshold: Option<f64>,
    #[serde(rename = "SECURITY_PROMPT_CLASSIFIER_ENABLED")]
    pub security_prompt_classifier_enabled: Option<bool>,
    #[serde(rename = "SECURITY_PROMPT_CLASSIFIER_MODEL")]
    pub security_prompt_classifier_model: Option<String>,
    #[serde(rename = "SECURITY_PROMPT_CLASSIFIER_ENDPOINT")]
    pub security_prompt_classifier_endpoint: Option<String>,
    #[serde(rename = "SECURITY_COMMAND_CLASSIFIER_ENABLED")]
    pub security_command_classifier_enabled: Option<bool>,

    // === Provider Settings ===
    #[serde(rename = "OPENAI_HOST")]
    pub openai_host: Option<String>,
    #[serde(rename = "OPENAI_BASE_PATH")]
    pub openai_base_path: Option<String>,
    #[serde(rename = "OPENAI_ORGANIZATION")]
    pub openai_organization: Option<String>,
    #[serde(rename = "OPENAI_PROJECT")]
    pub openai_project: Option<String>,
    #[serde(rename = "OPENAI_TIMEOUT")]
    pub openai_timeout: Option<u64>,
    #[serde(rename = "ANTHROPIC_HOST")]
    pub anthropic_host: Option<String>,
    #[serde(rename = "OLLAMA_HOST")]
    pub ollama_host: Option<String>,
    #[serde(rename = "OLLAMA_TIMEOUT")]
    pub ollama_timeout: Option<u64>,
    #[serde(rename = "OLLAMA_STREAM_TIMEOUT")]
    pub ollama_stream_timeout: Option<u64>,
    #[serde(rename = "OLLAMA_STREAM_USAGE")]
    pub ollama_stream_usage: Option<bool>,
    #[serde(rename = "DATABRICKS_HOST")]
    pub databricks_host: Option<String>,
    #[serde(rename = "DATABRICKS_MAX_RETRIES")]
    pub databricks_max_retries: Option<u64>,
    #[serde(rename = "DATABRICKS_INITIAL_RETRY_INTERVAL_MS")]
    pub databricks_initial_retry_interval_ms: Option<u64>,
    #[serde(rename = "DATABRICKS_BACKOFF_MULTIPLIER")]
    pub databricks_backoff_multiplier: Option<f64>,
    #[serde(rename = "DATABRICKS_MAX_RETRY_INTERVAL_MS")]
    pub databricks_max_retry_interval_ms: Option<u64>,
    #[serde(rename = "AZURE_OPENAI_ENDPOINT")]
    pub azure_openai_endpoint: Option<String>,
    #[serde(rename = "AZURE_OPENAI_DEPLOYMENT_NAME")]
    pub azure_openai_deployment_name: Option<String>,
    #[serde(rename = "AZURE_OPENAI_API_VERSION")]
    pub azure_openai_api_version: Option<String>,
    #[serde(rename = "GOOGLE_HOST")]
    pub google_host: Option<String>,
    #[serde(rename = "GCP_PROJECT_ID")]
    pub gcp_project_id: Option<String>,
    #[serde(rename = "GCP_LOCATION")]
    pub gcp_location: Option<String>,
    #[serde(rename = "GCP_MAX_RETRIES")]
    pub gcp_max_retries: Option<u64>,
    #[serde(rename = "GCP_INITIAL_RETRY_INTERVAL_MS")]
    pub gcp_initial_retry_interval_ms: Option<u64>,
    #[serde(rename = "GCP_BACKOFF_MULTIPLIER")]
    pub gcp_backoff_multiplier: Option<f64>,
    #[serde(rename = "GCP_MAX_RETRY_INTERVAL_MS")]
    pub gcp_max_retry_interval_ms: Option<u64>,
    #[serde(rename = "AWS_REGION")]
    pub aws_region: Option<String>,
    #[serde(rename = "AWS_PROFILE")]
    pub aws_profile: Option<String>,
    #[serde(rename = "BEDROCK_MAX_RETRIES")]
    pub bedrock_max_retries: Option<u64>,
    #[serde(rename = "BEDROCK_INITIAL_RETRY_INTERVAL_MS")]
    pub bedrock_initial_retry_interval_ms: Option<u64>,
    #[serde(rename = "BEDROCK_BACKOFF_MULTIPLIER")]
    pub bedrock_backoff_multiplier: Option<f64>,
    #[serde(rename = "BEDROCK_MAX_RETRY_INTERVAL_MS")]
    pub bedrock_max_retry_interval_ms: Option<u64>,
    #[serde(rename = "BEDROCK_ENABLE_CACHING")]
    pub bedrock_enable_caching: Option<bool>,
    #[serde(rename = "SAGEMAKER_ENDPOINT_NAME")]
    pub sagemaker_endpoint_name: Option<String>,
    #[serde(rename = "LITELLM_HOST")]
    pub litellm_host: Option<String>,
    #[serde(rename = "LITELLM_BASE_PATH")]
    pub litellm_base_path: Option<String>,
    #[serde(rename = "LITELLM_TIMEOUT")]
    pub litellm_timeout: Option<u64>,
    #[serde(rename = "SNOWFLAKE_HOST")]
    pub snowflake_host: Option<String>,
    #[serde(rename = "GITHUB_COPILOT_HOST")]
    pub github_copilot_host: Option<String>,
    #[serde(rename = "GITHUB_COPILOT_CLIENT_ID")]
    pub github_copilot_client_id: Option<String>,
    #[serde(rename = "GITHUB_COPILOT_TOKEN_URL")]
    pub github_copilot_token_url: Option<String>,
    #[serde(rename = "XAI_HOST")]
    pub xai_host: Option<String>,
    #[serde(rename = "OPENROUTER_HOST")]
    pub openrouter_host: Option<String>,
    #[serde(rename = "VENICE_HOST")]
    pub venice_host: Option<String>,
    #[serde(rename = "VENICE_BASE_PATH")]
    pub venice_base_path: Option<String>,
    #[serde(rename = "VENICE_MODELS_PATH")]
    pub venice_models_path: Option<String>,
    #[serde(rename = "TETRATE_HOST")]
    pub tetrate_host: Option<String>,
    #[serde(rename = "AVIAN_HOST")]
    pub avian_host: Option<String>,

    // === Observability Settings (lowercase keys) ===
    pub otel_exporter_otlp_endpoint: Option<String>,
    pub otel_exporter_otlp_timeout: Option<u64>,

    // === Tunnel Settings (lowercase keys) ===
    pub tunnel_auto_start: Option<bool>,

    // === Structured Config (lowercase keys) ===
    pub extensions: Option<HashMap<String, ExtensionEntry>>,
    pub slash_commands: Option<Vec<SlashCommandMapping>>,
    pub experiments: Option<HashMap<String, bool>>,
}

impl GooseConfigSchema {
    /// All user-facing config keys that get `config_value!` typed accessors.
    /// Category B keys (extensions, slash_commands, experiments) are in the struct
    /// for schema generation but NOT here — they use dedicated module helpers.
    pub const ALL_KEYS: &[&str] = &[
        // Core Goose Settings
        "GOOSE_PROVIDER",
        "GOOSE_MODEL",
        "GOOSE_MODE",
        "GOOSE_MAX_TOKENS",
        "GOOSE_CONTEXT_LIMIT",
        "GOOSE_INPUT_LIMIT",
        "GOOSE_MAX_TURNS",
        "GOOSE_MAX_ACTIVE_AGENTS",
        "GOOSE_AUTO_COMPACT_THRESHOLD",
        "GOOSE_TOOL_PAIR_SUMMARIZATION",
        "GOOSE_TOOL_CALL_CUTOFF",
        "GOOSE_STREAM_TIMEOUT",
        "GOOSE_SEARCH_PATHS",
        "GOOSE_DISABLE_SESSION_NAMING",
        "GOOSE_DISABLE_KEYRING",
        "GOOSE_TELEMETRY_ENABLED",
        "GOOSE_DEFAULT_EXTENSION_TIMEOUT",
        "GOOSE_PROMPT_EDITOR",
        "GOOSE_PROMPT_EDITOR_ALWAYS",
        "GOOSE_ALLOWLIST",
        "GOOSE_SYSTEM_PROMPT_FILE_PATH",
        "GOOSE_DEBUG",
        "GOOSE_SHOW_FULL_OUTPUT",
        "GOOSE_STATUS_HOOK",
        "GOOSE_LOCAL_ENABLE_THINKING",
        "GOOSE_DATABRICKS_CLIENT_REQUEST_ID",
        "CONTEXT_FILE_NAMES",
        "EDIT_MODE",
        "RANDOM_THINKING_MESSAGES",
        "CODE_MODE_TOOL_DISCLOSURE",
        // mTLS Settings
        "GOOSE_CLIENT_CERT_PATH",
        "GOOSE_CLIENT_KEY_PATH",
        "GOOSE_CA_CERT_PATH",
        // Planner & Subagent Settings
        "GOOSE_PLANNER_PROVIDER",
        "GOOSE_PLANNER_MODEL",
        "GOOSE_SUBAGENT_PROVIDER",
        "GOOSE_SUBAGENT_MODEL",
        "GOOSE_SUBAGENT_MAX_TURNS",
        "GOOSE_MAX_BACKGROUND_TASKS",
        // Recipe Settings
        "GOOSE_RECIPE_GITHUB_REPO",
        "GOOSE_RECIPE_RETRY_TIMEOUT_SECONDS",
        "GOOSE_RECIPE_ON_FAILURE_TIMEOUT_SECONDS",
        // CLI Settings
        "GOOSE_CLI_MIN_PRIORITY",
        "GOOSE_CLI_THEME",
        "GOOSE_CLI_LIGHT_THEME",
        "GOOSE_CLI_DARK_THEME",
        "GOOSE_CLI_SHOW_COST",
        "GOOSE_CLI_SHOW_THINKING",
        "GOOSE_CLI_NEWLINE_KEY",
        // AI Agent / Thinking Settings
        "CLAUDE_CODE_COMMAND",
        "GEMINI_CLI_COMMAND",
        "CURSOR_AGENT_COMMAND",
        "CODEX_COMMAND",
        "CODEX_REASONING_EFFORT",
        "CODEX_ENABLE_SKILLS",
        "CODEX_SKIP_GIT_CHECK",
        "CHATGPT_CODEX_REASONING_EFFORT",
        "CLAUDE_THINKING_TYPE",
        "CLAUDE_THINKING_EFFORT",
        "CLAUDE_THINKING_BUDGET",
        "GEMINI3_THINKING_LEVEL",
        "GEMINI25_THINKING_BUDGET",
        // Security Settings
        "SECURITY_PROMPT_ENABLED",
        "SECURITY_PROMPT_THRESHOLD",
        "SECURITY_PROMPT_CLASSIFIER_ENABLED",
        "SECURITY_PROMPT_CLASSIFIER_MODEL",
        "SECURITY_PROMPT_CLASSIFIER_ENDPOINT",
        "SECURITY_COMMAND_CLASSIFIER_ENABLED",
        // Provider Settings
        "OPENAI_HOST",
        "OPENAI_BASE_PATH",
        "OPENAI_ORGANIZATION",
        "OPENAI_PROJECT",
        "OPENAI_TIMEOUT",
        "ANTHROPIC_HOST",
        "OLLAMA_HOST",
        "OLLAMA_TIMEOUT",
        "OLLAMA_STREAM_TIMEOUT",
        "OLLAMA_STREAM_USAGE",
        "DATABRICKS_HOST",
        "DATABRICKS_MAX_RETRIES",
        "DATABRICKS_INITIAL_RETRY_INTERVAL_MS",
        "DATABRICKS_BACKOFF_MULTIPLIER",
        "DATABRICKS_MAX_RETRY_INTERVAL_MS",
        "AZURE_OPENAI_ENDPOINT",
        "AZURE_OPENAI_DEPLOYMENT_NAME",
        "AZURE_OPENAI_API_VERSION",
        "GOOGLE_HOST",
        "GCP_PROJECT_ID",
        "GCP_LOCATION",
        "GCP_MAX_RETRIES",
        "GCP_INITIAL_RETRY_INTERVAL_MS",
        "GCP_BACKOFF_MULTIPLIER",
        "GCP_MAX_RETRY_INTERVAL_MS",
        "AWS_REGION",
        "AWS_PROFILE",
        "BEDROCK_MAX_RETRIES",
        "BEDROCK_INITIAL_RETRY_INTERVAL_MS",
        "BEDROCK_BACKOFF_MULTIPLIER",
        "BEDROCK_MAX_RETRY_INTERVAL_MS",
        "BEDROCK_ENABLE_CACHING",
        "SAGEMAKER_ENDPOINT_NAME",
        "LITELLM_HOST",
        "LITELLM_BASE_PATH",
        "LITELLM_TIMEOUT",
        "SNOWFLAKE_HOST",
        "GITHUB_COPILOT_HOST",
        "GITHUB_COPILOT_CLIENT_ID",
        "GITHUB_COPILOT_TOKEN_URL",
        "XAI_HOST",
        "OPENROUTER_HOST",
        "VENICE_HOST",
        "VENICE_BASE_PATH",
        "VENICE_MODELS_PATH",
        "TETRATE_HOST",
        "AVIAN_HOST",
        // Observability Settings
        "otel_exporter_otlp_endpoint",
        "otel_exporter_otlp_timeout",
        // Tunnel Settings
        "tunnel_auto_start",
    ];

    pub const fn has_key(key: &str) -> bool {
        let key_bytes = key.as_bytes();
        let mut i = 0;
        while i < Self::ALL_KEYS.len() {
            let candidate = Self::ALL_KEYS[i].as_bytes();
            if candidate.len() == key_bytes.len() {
                let mut j = 0;
                let mut eq = true;
                while j < key_bytes.len() {
                    if candidate[j] != key_bytes[j] {
                        eq = false;
                        break;
                    }
                    j += 1;
                }
                if eq {
                    return true;
                }
            }
            i += 1;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::schema_for;

    #[test]
    fn all_keys_matches_struct_fields() {
        let schema = schema_for!(GooseConfigSchema);
        let obj = schema.as_object().expect("schema should be an object");
        let properties = obj
            .get("properties")
            .and_then(|p| p.as_object())
            .expect("schema should have properties");

        let schema_keys: std::collections::HashSet<&str> =
            properties.keys().map(|k| k.as_str()).collect();

        for key in GooseConfigSchema::ALL_KEYS {
            assert!(
                schema_keys.contains(key),
                "ALL_KEYS contains '{key}' but GooseConfigSchema has no field with serde(rename = \"{key}\")"
            );
        }

        // Category B keys are in the struct but NOT in ALL_KEYS — that's intentional
        let category_b = ["extensions", "slash_commands", "experiments"];
        for key in &category_b {
            assert!(
                schema_keys.contains(key),
                "Category B key '{key}' should be in the schema struct for IDE autocomplete"
            );
            assert!(
                !GooseConfigSchema::has_key(key),
                "Category B key '{key}' should NOT be in ALL_KEYS"
            );
        }
    }

    #[test]
    fn no_untyped_config_access_in_production_code() {
        use std::path::Path;

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let crates_dir = manifest_dir.parent().unwrap();

        // Files that legitimately use raw get_param/set_param with string keys
        let allowlist: &[&str] = &[
            // Category B: internal persisted state with dedicated module helpers
            "config/base.rs",
            "config/extensions.rs",
            "config/experiments.rs",
            "config/migrations.rs",
            "slash_commands.rs",
            "gateway/manager.rs",
            "gateway/pairing.rs",
            "dictation/whisper.rs",
            "providers/local_inference.rs",
            "providers/kimicode.rs",
            "providers/toolshim.rs",
            "providers/snowflake.rs",
            // Category C: dynamic key access from runtime variables
            "goose-acp/src/server.rs",
            "routes/config_management.rs",
            "routes/utils.rs",
            "routes/dictation.rs",
            "config/declarative_providers.rs",
            "security/scanner.rs",
            "oauth/persist.rs",
            "commands/configure.rs",
            "recipes/recipe.rs",
        ];

        fn is_allowlisted(path: &Path, allowlist: &[&str]) -> bool {
            let path_str = path.to_string_lossy();
            allowlist.iter().any(|a| path_str.ends_with(a))
        }

        fn _is_test_code(_line: &str, in_test_module: bool) -> bool {
            in_test_module
        }

        fn collect_rs_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        collect_rs_files(&path, out);
                    } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                        out.push(path);
                    }
                }
            }
        }

        let mut rs_files = Vec::new();
        collect_rs_files(crates_dir, &mut rs_files);

        let mut violations = Vec::new();

        for path in &rs_files {
            if is_allowlisted(path, allowlist) {
                continue;
            }

            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let mut in_test_module = false;
            for (line_num, line) in content.lines().enumerate() {
                if line.contains("#[cfg(test") {
                    in_test_module = true;
                }
                if in_test_module {
                    continue;
                }
                // Skip comments and doc strings
                let trimmed = line.trim();
                if trimmed.starts_with("//") || trimmed.starts_with("///") {
                    continue;
                }

                // Detect get_param/set_param with literal string arguments
                if (trimmed.contains("get_param(\"") || trimmed.contains("set_param(\""))
                    && !trimmed.contains("get_param(&")
                {
                    let relative = path.strip_prefix(crates_dir).unwrap_or(path);
                    violations.push(format!(
                        "  {}:{}: {}",
                        relative.display(),
                        line_num + 1,
                        trimmed
                    ));
                }
            }
        }

        assert!(
            violations.is_empty(),
            "Found untyped config access with literal string keys outside allowlisted files.\n\
             Use a typed accessor (e.g., config.get_key()) instead.\n\
             If the key is new, add it to GooseConfigSchema + config_value! first.\n\
             If the file legitimately needs raw access, add it to the allowlist with a comment.\n\n\
             Violations:\n{}",
            violations.join("\n")
        );
    }
}
