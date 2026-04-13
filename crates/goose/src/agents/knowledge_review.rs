//! Background knowledge review and pre-compression memory flush.
//!
//! Two mechanisms for autonomous memory extraction:
//!
//! 1. **Background review**: After a reply is delivered, if enough user turns
//!    have elapsed, spawn a background task that reviews the conversation
//!    and calls memory tools to extract durable facts.
//!
//! 2. **Pre-compression flush**: Before context compaction, give the model
//!    one cheap API call with only memory tools to save anything worth
//!    keeping before it gets summarized away.

use std::sync::Arc;

use crate::agents::extension_manager::ExtensionManager;
use crate::agents::tool_execution::ToolCallContext;
use crate::conversation::message::{Message, MessageContent};
use crate::conversation::Conversation;
use crate::providers::base::Provider;
use rmcp::model::Tool;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

/// Default interval: review memory every N user turns.
pub const DEFAULT_MEMORY_REVIEW_INTERVAL: u32 = 5;

/// Maximum tool calls the review agent can make per review.
const MAX_REVIEW_TOOL_CALLS: usize = 8;

/// The prompt injected for background memory review.
const MEMORY_REVIEW_PROMPT: &str = r#"Review the conversation above and extract any durable facts worth saving to persistent memory. Focus on:

1. User identity/preferences: name, role, timezone, coding style, communication preferences, pet peeves
   → Save these with target "user"

2. Environment/project facts: OS, installed tools, project structure, build commands, API quirks, tool behaviors
   → Save these with target "memory"

3. Corrections: if the user corrected the agent's approach, save the correct approach

4. Lessons learned: non-obvious things discovered through trial and error
   → Save with target "memory"

Rules:
- Do NOT save task progress, session outcomes, or temporary state
- Do NOT save things that are obvious or easily re-discovered
- Keep entries concise — one fact per entry
- If nothing is worth saving, just say so.

Make your memory tool calls now."#;

/// The prompt injected for pre-compression flush.
const FLUSH_PROMPT: &str = "[System: The session context is being compressed. Save anything worth remembering permanently — prioritize user preferences, corrections, environment facts, and recurring patterns over task-specific details. This is your last chance before earlier conversation turns are summarized away.]";

/// Spawn a background task to review the conversation for memory-worthy facts.
///
/// Runs AFTER the reply is delivered. The user never sees this.
pub fn spawn_background_review(
    provider: Arc<dyn Provider>,
    extension_manager: Arc<ExtensionManager>,
    conversation: Conversation,
    session_id: String,
    working_dir: std::path::PathBuf,
) {
    tokio::spawn(async move {
        if let Err(e) = run_memory_extraction(
            provider.as_ref(),
            &extension_manager,
            &conversation,
            &session_id,
            &working_dir,
            MEMORY_REVIEW_PROMPT,
            "review",
        )
        .await
        {
            warn!("Background memory review failed: {}", e);
        }
    });
}

/// Run the pre-compression flush synchronously before compaction.
pub async fn flush_memories_before_compaction(
    provider: &dyn Provider,
    extension_manager: &ExtensionManager,
    conversation: &Conversation,
    session_id: &str,
    working_dir: &std::path::Path,
) -> anyhow::Result<()> {
    info!("Flushing memories before context compaction");

    let user_msg_count = conversation
        .messages()
        .iter()
        .filter(|m| matches!(m.role, rmcp::model::Role::User) && m.is_agent_visible())
        .count();

    if user_msg_count < 3 {
        debug!(
            "Skipping memory flush: too few user messages ({})",
            user_msg_count
        );
        return Ok(());
    }

    run_memory_extraction(
        provider,
        extension_manager,
        conversation,
        session_id,
        working_dir,
        FLUSH_PROMPT,
        "flush",
    )
    .await
}

/// Core extraction logic shared by background review and flush.
///
/// 1. Build messages from conversation + extraction prompt
/// 2. Find memory tools from extension manager
/// 3. Call complete_fast() with only memory tools
/// 4. Execute any memory tool calls from the response
async fn run_memory_extraction(
    provider: &dyn Provider,
    extension_manager: &ExtensionManager,
    conversation: &Conversation,
    session_id: &str,
    working_dir: &std::path::Path,
    extraction_prompt: &str,
    task_name: &str,
) -> anyhow::Result<()> {
    // Find memory tools
    let all_tools = extension_manager
        .get_prefixed_tools(session_id, None)
        .await
        .unwrap_or_default();

    let memory_tools: Vec<Tool> = all_tools
        .into_iter()
        .filter(|t| {
            let name: &str = &t.name;
            name == "memory" || name.ends_with("__memory")
        })
        .collect();

    if memory_tools.is_empty() {
        debug!("No memory tools found, skipping {} extraction", task_name);
        return Ok(());
    }

    // Build messages: conversation snapshot + extraction prompt
    let mut messages: Vec<Message> = conversation
        .messages()
        .iter()
        .filter(|m| m.is_agent_visible())
        .cloned()
        .collect();

    messages.push(Message::user().with_text(extraction_prompt));

    let system_prompt =
        "You are reviewing a conversation to extract durable facts for persistent memory. \
         You have access to the memory tool. Use it to save important facts. \
         Be selective — only save things that will matter in future sessions.";

    // Mini agent loop: call model, execute tool calls, repeat
    let mut tool_calls_made = 0;

    loop {
        if tool_calls_made >= MAX_REVIEW_TOOL_CALLS {
            break;
        }

        let result = provider
            .complete_fast(session_id, system_prompt, &messages, &memory_tools)
            .await;

        let (response_message, _usage) = match result {
            Ok(r) => r,
            Err(e) => {
                debug!("Memory {} model call failed: {}", task_name, e);
                break;
            }
        };

        // Extract tool requests from response
        let tool_requests: Vec<_> = response_message
            .content
            .iter()
            .filter_map(|c| {
                if let MessageContent::ToolRequest(tr) = c {
                    Some(tr.clone())
                } else {
                    None
                }
            })
            .collect();

        if tool_requests.is_empty() {
            break;
        }

        messages.push(response_message);

        // Execute each tool call
        for tool_request in &tool_requests {
            tool_calls_made += 1;

            let tool_call = match &tool_request.tool_call {
                Ok(call) => call.clone(),
                Err(_) => continue,
            };

            let ctx = ToolCallContext::new(
                session_id.to_string(),
                Some(working_dir.to_path_buf()),
                None,
            );

            match extension_manager
                .dispatch_tool_call(&ctx, tool_call, CancellationToken::default())
                .await
            {
                Ok(tool_result) => {
                    let call_result = tool_result.result.await;
                    debug!(
                        "Memory {} tool call completed: {:?}",
                        task_name,
                        call_result.is_ok()
                    );
                    let response =
                        Message::user().with_tool_response(tool_request.id.clone(), call_result);
                    messages.push(response);
                }
                Err(e) => {
                    debug!("Memory {} tool dispatch failed: {}", task_name, e);
                }
            }
        }
    }

    info!(
        "Memory {} complete: {} tool calls made",
        task_name, tool_calls_made
    );
    Ok(())
}
