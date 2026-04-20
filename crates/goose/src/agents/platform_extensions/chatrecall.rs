use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait};
use crate::agents::tool_execution::ToolCallContext;
use crate::session::session_manager::SessionType;
use crate::session::session_summary::{
    format_conversation, summarize_matched_sessions, SessionForSummary,
};
use anyhow::Result;
use async_trait::async_trait;
use indoc::indoc;
use rmcp::model::{
    CallToolResult, Content, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ServerCapabilities, Tool, ToolAnnotations,
};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use tracing::debug;

pub static EXTENSION_NAME: &str = "chatrecall";

/// Maximum sessions to summarize in a single search call.
/// Capped to limit LLM cost/latency (mirrors Hermes cap of 5).
const MAX_SUMMARIZE_SESSIONS: usize = 5;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ChatRecallParams {
    /// Search keywords. Use multiple related terms/synonyms (e.g., 'database postgres sql'). Mutually exclusive with session_id.
    #[serde(skip_serializing_if = "Option::is_none")]
    query: Option<String>,
    /// Session ID to load. Returns first/last 3 messages. Mutually exclusive with query.
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    /// Max results (default: 10, max: 50). Search mode only.
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<i64>,
    /// ISO 8601 date (e.g., '2025-10-01T00:00:00Z'). Search mode only.
    #[serde(skip_serializing_if = "Option::is_none")]
    after_date: Option<String>,
    /// ISO 8601 date (e.g., '2025-10-15T23:59:59Z'). Search mode only.
    #[serde(skip_serializing_if = "Option::is_none")]
    before_date: Option<String>,
    /// Summarize matched sessions via LLM instead of returning raw messages. Default true.
    #[serde(skip_serializing_if = "Option::is_none")]
    summarize: Option<bool>,
}

pub struct ChatRecallClient {
    info: InitializeResult,
    context: PlatformExtensionContext,
}

impl ChatRecallClient {
    pub fn new(context: PlatformExtensionContext) -> Result<Self> {
        let info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new(EXTENSION_NAME.to_string(), "1.0.0".to_string())
                    .with_title("Chat Recall"),
            )
            .with_instructions(indoc! {r#"
                Chat Recall — search your long-term memory of past conversations, or load session details.

                Two modes:
                1. Search mode (query): Keywords/synonyms to find relevant sessions. Returns LLM-generated
                   summaries of matching sessions by default — clean, focused recaps instead of raw transcripts.
                   Set summarize=false to get raw messages instead.
                2. Load mode (session_id): Get first and last messages of a specific session.

                Use this proactively when:
                - The user says "we did this before", "remember when", "last time"
                - The user asks about a topic you worked on before but don't have in current context
                - You want to check if you've solved a similar problem before
            "#}.to_string());

        Ok(Self { info, context })
    }

    fn search_session_types(&self) -> Vec<SessionType> {
        match self.context.session.as_ref().map(|s| s.session_type) {
            Some(SessionType::Acp) => vec![SessionType::Acp],
            _ => vec![SessionType::User, SessionType::Scheduled],
        }
    }

    /// Get the provider from the extension manager weak reference.
    fn get_provider(&self) -> Option<std::sync::Arc<dyn crate::providers::base::Provider>> {
        let em = self.context.extension_manager.as_ref()?.upgrade()?;
        let guard = em.get_provider().try_lock().ok()?;
        guard.clone()
    }

    #[allow(clippy::too_many_lines)]
    async fn handle_chatrecall(
        &self,
        current_session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let arguments = arguments.ok_or("Missing arguments")?;

        let target_session_id = arguments
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if let Some(sid) = target_session_id {
            self.handle_load_mode(&sid).await
        } else {
            self.handle_search_mode(current_session_id, &arguments)
                .await
        }
    }

    async fn handle_load_mode(&self, sid: &str) -> Result<Vec<Content>, String> {
        match self.context.session_manager.get_session(sid, true).await {
            Ok(loaded_session) => {
                let conversation = loaded_session.conversation.as_ref();

                if conversation.is_none() {
                    return Ok(vec![Content::text(format!(
                        "Session {} has no conversation.",
                        sid
                    ))]);
                }

                let msgs = conversation.unwrap().messages();
                let total = msgs.len();

                if total == 0 {
                    return Ok(vec![Content::text(format!(
                        "Session {} has no messages.",
                        sid
                    ))]);
                }

                let mut output = format!(
                    "Session: {} (ID: {})\nWorking Dir: {}\nTotal Messages: {}\n\n",
                    loaded_session.name,
                    sid,
                    loaded_session.working_dir.display(),
                    total
                );

                let first_count = std::cmp::min(3, total);
                output.push_str("--- First Few Messages ---\n\n");
                for (idx, msg) in msgs.iter().take(first_count).enumerate() {
                    output.push_str(&format!("{}. [{:?}] ", idx + 1, msg.role));
                    for content in &msg.content {
                        if let Some(text) = content.as_text() {
                            output.push_str(text);
                            output.push('\n');
                        }
                    }
                    output.push('\n');
                }

                if total > first_count {
                    output.push_str("--- Last Few Messages ---\n\n");
                    let last_count = std::cmp::min(3, total);
                    let skip_count = total.saturating_sub(last_count);
                    for (idx, msg) in msgs.iter().skip(skip_count).enumerate() {
                        output.push_str(&format!("{}. [{:?}] ", skip_count + idx + 1, msg.role));
                        for content in &msg.content {
                            if let Some(text) = content.as_text() {
                                output.push_str(text);
                                output.push('\n');
                            }
                        }
                        output.push('\n');
                    }
                }

                Ok(vec![Content::text(output)])
            }
            Err(e) => Err(format!("Failed to load session: {}", e)),
        }
    }

    async fn handle_search_mode(
        &self,
        current_session_id: &str,
        arguments: &JsonObject,
    ) -> Result<Vec<Content>, String> {
        let query = arguments
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: query or session_id")?
            .to_string();

        let limit = arguments
            .get("limit")
            .and_then(|v| v.as_i64())
            .map(|l| l as usize)
            .unwrap_or(10)
            .min(50);

        let summarize = arguments
            .get("summarize")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let after_date = arguments
            .get("after_date")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc));

        let before_date = arguments
            .get("before_date")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc));

        let exclude_session_id = Some(current_session_id.to_string());

        let results = self
            .context
            .session_manager
            .search_chat_history(
                &query,
                Some(limit),
                after_date,
                before_date,
                exclude_session_id,
                self.search_session_types(),
            )
            .await
            .map_err(|e| format!("Chat recall failed: {}", e))?;

        if results.total_matches == 0 {
            return Ok(vec![Content::text(format!(
                "No results found for query: '{}'",
                query
            ))]);
        }

        // Summarize mode: load full conversations, format, and summarize via LLM
        if summarize {
            if let Some(output) = self.search_with_summaries(&query, &results).await {
                return Ok(vec![Content::text(output)]);
            }
            debug!("Summarization unavailable, falling back to raw results");
        }

        // Raw mode (or fallback): return messages directly
        Ok(vec![Content::text(Self::format_raw_results(
            &query, &results,
        ))])
    }

    /// Search mode with LLM summarization — the Hermes pattern.
    ///
    /// For each matched session, loads the full conversation, formats it into
    /// a readable transcript, then runs parallel LLM summarization focused on
    /// the search query.
    async fn search_with_summaries(
        &self,
        query: &str,
        results: &crate::session::chat_history_search::ChatRecallResults,
    ) -> Option<String> {
        let provider = self.get_provider()?;
        let model_config = provider.get_model_config();

        // Load full conversations for matched sessions (capped)
        let session_limit = results.results.len().min(MAX_SUMMARIZE_SESSIONS);
        let mut sessions_for_summary = Vec::with_capacity(session_limit);

        for result in results.results.iter().take(session_limit) {
            let conversation = match self
                .context
                .session_manager
                .get_session(&result.session_id, true)
                .await
            {
                Ok(s) => s.conversation,
                Err(e) => {
                    debug!(
                        "Failed to load session {} for summarization: {}",
                        result.session_id, e
                    );
                    continue;
                }
            };

            let Some(conversation) = conversation else {
                continue;
            };

            let conversation_text = format_conversation(&conversation);
            sessions_for_summary.push(SessionForSummary {
                session_id: result.session_id.clone(),
                description: result.session_description.clone(),
                working_dir: result.session_working_dir.clone(),
                conversation_text,
            });
        }

        if sessions_for_summary.is_empty() {
            return None;
        }

        let summaries = summarize_matched_sessions(
            provider.as_ref(),
            &model_config,
            sessions_for_summary,
            query,
        )
        .await;

        let mut output = format!(
            "Found {} session(s) matching '{}' — summaries:\n\n",
            summaries.len(),
            query
        );

        for (idx, summary) in summaries.iter().enumerate() {
            output.push_str(&format!(
                "{}. Session: {} (ID: {})\n   Dir: {}\n\n   {}\n\n",
                idx + 1,
                summary.description,
                summary.session_id,
                summary.working_dir,
                summary
                    .summary
                    .lines()
                    .map(|line| format!("   {}", line))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ));
        }

        Some(output)
    }

    fn format_raw_results(
        query: &str,
        results: &crate::session::chat_history_search::ChatRecallResults,
    ) -> String {
        let mut output = format!(
            "Found {} matching message(s) across {} session(s) for query: '{}'\n\n",
            results.total_matches,
            results.results.len(),
            query
        );
        for (idx, result) in results.results.iter().enumerate() {
            output.push_str(&format!(
                "{}. Session: {} (ID: {})\n   Working Dir: {}\n   Last Activity: {}\n   Showing {} of {} total message(s) in session:\n\n",
                idx + 1,
                result.session_description,
                result.session_id,
                result.session_working_dir,
                result.last_activity.format("%Y-%m-%d"),
                result.messages.len(),
                result.total_messages_in_session
            ));

            for (msg_idx, message) in result.messages.iter().enumerate() {
                output.push_str(&format!(
                    "   {}.{} [{}]\n   {}\n\n",
                    idx + 1,
                    msg_idx + 1,
                    message.role,
                    message
                        .content
                        .lines()
                        .map(|line| format!("   {}", line))
                        .collect::<Vec<_>>()
                        .join("\n")
                ));
            }
        }
        output
    }

    fn get_tools() -> Vec<Tool> {
        let schema = schema_for!(ChatRecallParams);
        let schema_value =
            serde_json::to_value(schema).expect("Failed to serialize ChatRecallParams schema");

        let input_schema = schema_value
            .as_object()
            .expect("Schema should be an object")
            .clone();

        vec![Tool::new(
            "chatrecall".to_string(),
            indoc! {r#"
                Search your long-term memory of past conversations, or load session details.

                search mode (query): Use multiple keywords/synonyms. Returns LLM-generated summaries
                of matching sessions by default. Set summarize=false for raw messages. Supports date filters.
                load mode (session_id): Returns first/last 3 messages of a session.

                Use this proactively when the user references past work, says "remember when",
                or asks about something you worked on before.
            "#}
            .to_string(),
            input_schema,
        )
        .annotate(ToolAnnotations::from_raw(
            Some("Recall past conversations".to_string()),
            Some(true),
            Some(false),
            Some(true),
            Some(false),
        ))]
    }
}

#[async_trait]
impl McpClientTrait for ChatRecallClient {
    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        Ok(ListToolsResult {
            tools: Self::get_tools(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        ctx: &ToolCallContext,
        name: &str,
        arguments: Option<JsonObject>,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        let session_id = &ctx.session_id;
        let content = match name {
            "chatrecall" => self.handle_chatrecall(session_id, arguments).await,
            _ => Err(format!("Unknown tool: {}", name)),
        };

        match content {
            Ok(content) => Ok(CallToolResult::success(content)),
            Err(error) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Error: {}",
                error
            ))])),
        }
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }
}
