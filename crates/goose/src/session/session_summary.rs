//! Session summarization utilities for cross-session recall.
//!
//! Ported from Hermes `session_search_tool.py`: instead of dumping raw transcripts
//! into the agent's context, we truncate around query matches and optionally
//! summarize via an LLM call. Used by both `chatrecall` (user-facing recall) and
//! `knowledge_review::build_session_context` (background review).

use crate::conversation::message::{Message, MessageContent};
use crate::conversation::Conversation;
use crate::model::ModelConfig;
use crate::providers::base::Provider;
use rmcp::model::RawContent;
use std::ops::Deref;
use tracing::debug;

/// Maximum characters to keep when truncating a conversation transcript around matches.
const MAX_SESSION_CHARS: usize = 100_000;

/// Maximum characters for the shorter context used by background review.
const MAX_REVIEW_CHARS: usize = 8_000;

/// Format a conversation into a readable transcript for summarization.
///
/// Tool outputs are truncated to 500 chars; tool call names are included
/// on assistant messages so the summarizer knows what happened without
/// reading full payloads.
pub fn format_conversation(conversation: &Conversation) -> String {
    let mut parts = Vec::new();

    for msg in conversation.messages() {
        let role = match msg.role {
            rmcp::model::Role::User => "USER",
            rmcp::model::Role::Assistant => "ASSISTANT",
        };

        for content in &msg.content {
            match content {
                MessageContent::Text(tc) => {
                    parts.push(format!("[{}]: {}", role, tc.text));
                }
                MessageContent::ToolRequest(tr) => {
                    let readable = tr.to_readable_string();
                    parts.push(format!("[ASSISTANT]: [Called: {}]", readable));
                }
                MessageContent::ToolResponse(resp) => {
                    let text = match &resp.tool_result {
                        Ok(result) => {
                            let full: String = result
                                .content
                                .iter()
                                .filter_map(|c| match c.deref() {
                                    RawContent::Text(t) => Some(t.text.as_str()),
                                    _ => None,
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            if full.chars().count() > 500 {
                                let head: String = full.chars().take(250).collect();
                                let tail: String = full
                                    .chars()
                                    .skip(full.chars().count().saturating_sub(250))
                                    .collect();
                                format!("{}...[truncated]...{}", head, tail)
                            } else {
                                full
                            }
                        }
                        Err(e) => format!("[Tool Error: {}]", e),
                    };
                    parts.push(format!("[TOOL]: {}", text));
                }
                MessageContent::Thinking(t) => {
                    parts.push(format!("[THINKING]: {}", t.thinking));
                }
                _ => {}
            }
        }
    }

    parts.join("\n\n")
}

/// Truncate a conversation transcript to `max_chars`, centered around where
/// query terms first appear. Keeps content near matches, trims the edges.
///
/// If the text is already within `max_chars`, returns it unchanged.
/// All indexing is char-based to avoid panicking on multi-byte UTF-8.
pub fn truncate_around_matches(full_text: &str, query: &str, max_chars: usize) -> String {
    let char_count = full_text.chars().count();
    if char_count <= max_chars {
        return full_text.to_string();
    }

    let text_lower = full_text.to_lowercase();
    let query_terms: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();

    // Find the first char-offset occurrence of any query term.
    // We iterate chars to find the position without byte-indexing into the string.
    let first_match_char = query_terms
        .iter()
        .filter_map(|term| {
            text_lower.find(term.as_str()).map(|byte_pos| {
                text_lower
                    .char_indices()
                    .take_while(|(i, _)| *i < byte_pos)
                    .count()
            })
        })
        .min()
        .unwrap_or(0);

    // Center the window around the first match
    let half = max_chars / 2;
    let mut start = first_match_char.saturating_sub(half);
    let end = (start + max_chars).min(char_count);
    if end - start < max_chars {
        start = end.saturating_sub(max_chars);
    }

    let truncated: String = full_text.chars().skip(start).take(end - start).collect();
    let prefix = if start > 0 {
        "...[earlier conversation truncated]...\n\n"
    } else {
        ""
    };
    let suffix = if end < char_count {
        "\n\n...[later conversation truncated]..."
    } else {
        ""
    };

    format!("{}{}{}", prefix, truncated, suffix)
}

/// System prompt for session summarization — mirrors the Hermes pattern.
const SUMMARIZE_SYSTEM_PROMPT: &str = "\
You are reviewing a past conversation transcript to help recall what happened. \
Summarize the conversation with a focus on the search topic. Include:\n\
1. What the user asked about or wanted to accomplish\n\
2. What actions were taken and what the outcomes were\n\
3. Key decisions, solutions found, or conclusions reached\n\
4. Any specific commands, files, URLs, or technical details that were important\n\
5. Anything left unresolved or notable\n\n\
Be thorough but concise. Preserve specific details (commands, paths, error messages) \
that would be useful to recall. Write in past tense as a factual recap.";

/// A summarized session result returned by the summarization pipeline.
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub session_id: String,
    pub working_dir: String,
    pub description: String,
    pub summary: String,
}

/// Summarize a single session conversation focused on a search query.
///
/// Loads the full conversation, formats it, truncates around matches,
/// then calls the provider for a focused summary.
async fn summarize_single_session(
    provider: &dyn Provider,
    model_config: &ModelConfig,
    session_id: &str,
    query: &str,
    conversation_text: &str,
    working_dir: &str,
) -> Option<String> {
    let user_prompt = format!(
        "Search topic: {}\nSession working directory: {}\n\n\
         CONVERSATION TRANSCRIPT:\n{}\n\n\
         Summarize this conversation with focus on: {}",
        query, working_dir, conversation_text, query
    );

    let messages = vec![Message::user().with_text(user_prompt)];

    match provider
        .complete(
            model_config,
            session_id,
            SUMMARIZE_SYSTEM_PROMPT,
            &messages,
            &[],
        )
        .await
    {
        Ok((response, _usage)) => {
            let text: String = response
                .content
                .iter()
                .filter_map(|c| {
                    if let MessageContent::Text(t) = c {
                        Some(t.text.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            if text.trim().is_empty() {
                debug!(
                    "Session summarization returned empty for session {}",
                    session_id
                );
                None
            } else {
                Some(text)
            }
        }
        Err(e) => {
            debug!(
                "Session summarization failed for session {}: {}",
                session_id, e
            );
            None
        }
    }
}

/// Summarize multiple sessions in parallel, returning a summary per session.
///
/// Used by `chatrecall` in search mode when summarization is requested.
/// Sessions that fail summarization get a raw-preview fallback so they
/// are never silently dropped.
pub async fn summarize_matched_sessions(
    provider: &dyn Provider,
    model_config: &ModelConfig,
    sessions: Vec<SessionForSummary>,
    query: &str,
) -> Vec<SessionSummary> {
    // Pre-truncate each session so the futures own their data.
    let truncated_texts: Vec<String> = sessions
        .iter()
        .map(|s| truncate_around_matches(&s.conversation_text, query, MAX_SESSION_CHARS))
        .collect();

    let futures: Vec<_> = sessions
        .iter()
        .zip(truncated_texts.iter())
        .map(|(s, text)| {
            summarize_single_session(
                provider,
                model_config,
                &s.session_id,
                query,
                text,
                &s.working_dir,
            )
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    sessions
        .into_iter()
        .zip(results)
        .map(|(session, summary_opt)| {
            let summary = summary_opt.unwrap_or_else(|| {
                let preview: String = session.conversation_text.chars().take(500).collect();
                format!("[Raw preview — summarization unavailable]\n{}…", preview)
            });
            SessionSummary {
                session_id: session.session_id,
                working_dir: session.working_dir,
                description: session.description,
                summary,
            }
        })
        .collect()
}

/// Input for summarization — a session's pre-formatted conversation text + metadata.
#[derive(Debug)]
pub struct SessionForSummary {
    pub session_id: String,
    pub description: String,
    pub working_dir: String,
    pub conversation_text: String,
}

/// Build cross-session context for the background knowledge review.
///
/// Replaces the old `build_session_context` that dumped raw 200-char snippets.
/// Now formats full conversations, truncates around a broad query, and returns
/// a compact context block. No LLM calls — just intelligent truncation.
///
/// For the background review, we use a tighter character budget than chatrecall
/// since this feeds into the review prompt alongside the current conversation.
pub async fn build_review_context(
    session_manager: &crate::session::SessionManager,
    exclude_session_id: &str,
) -> String {
    use crate::session::session_manager::SessionType;

    let results = session_manager
        .search_chat_history(
            "*",
            Some(5),
            None,
            None,
            Some(exclude_session_id.to_string()),
            vec![SessionType::User],
        )
        .await;

    let Ok(results) = results else {
        return String::new();
    };

    if results.results.is_empty() {
        return String::new();
    }

    let mut context = String::new();
    let budget_per_session = MAX_REVIEW_CHARS / 3;

    for session in results.results.iter().take(3) {
        if session.messages.is_empty() {
            continue;
        }

        // Load the full conversation for this session
        let conversation = match session_manager.get_session(&session.session_id, true).await {
            Ok(s) => s.conversation,
            Err(_) => continue,
        };

        let Some(conversation) = conversation else {
            continue;
        };

        let formatted = format_conversation(&conversation);
        // Truncate to per-session budget, centered on recent content (no specific query)
        let truncated: String = if formatted.chars().count() > budget_per_session {
            // For broad context, take from the end (most recent messages are most relevant)
            let skip = formatted.chars().count().saturating_sub(budget_per_session);
            let snippet: String = formatted.chars().skip(skip).collect();
            format!("...[earlier truncated]...\n{}", snippet)
        } else {
            formatted
        };

        context.push_str(&format!(
            "\n--- Session: {} ({}) ---\nDir: {}\n{}\n",
            session.session_description,
            session.last_activity.format("%Y-%m-%d"),
            session.session_working_dir,
            truncated
        ));
    }

    // Hard cap on total context
    if context.len() > MAX_REVIEW_CHARS {
        let truncated: String = context.chars().take(MAX_REVIEW_CHARS).collect();
        format!("{}\n[...truncated]", truncated)
    } else {
        context
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_passthrough() {
        let text = "hello world";
        assert_eq!(truncate_around_matches(text, "hello", 100), text);
    }

    #[test]
    fn truncate_centers_on_match() {
        // Build a string where the match is in the middle
        let before = "a".repeat(500);
        let match_text = "FINDME";
        let after = "b".repeat(500);
        let full = format!("{}{}{}", before, match_text, after);

        let result = truncate_around_matches(&full, "FINDME", 200);
        assert!(result.contains("FINDME"));
        assert!(result.len() <= 200 + 80); // allow for prefix/suffix markers
    }

    #[test]
    fn truncate_no_match_takes_start() {
        let text = "a".repeat(1000);
        let result = truncate_around_matches(&text, "NOTFOUND", 200);
        // Should take from the start since no match found
        assert!(result.starts_with("aaa"));
        assert!(result.contains("later conversation truncated"));
    }

    #[test]
    fn truncate_match_near_start() {
        let match_text = "FINDME";
        let after = "b".repeat(1000);
        let full = format!("{}{}", match_text, after);

        let result = truncate_around_matches(&full, "FINDME", 200);
        assert!(result.contains("FINDME"));
        // Should not have "earlier truncated" prefix since match is at start
        assert!(!result.contains("earlier conversation truncated"));
    }

    #[test]
    fn truncate_match_near_end() {
        let before = "a".repeat(1000);
        let match_text = "FINDME";
        let full = format!("{}{}", before, match_text);

        let result = truncate_around_matches(&full, "FINDME", 200);
        assert!(result.contains("FINDME"));
        assert!(result.contains("earlier conversation truncated"));
    }

    #[test]
    fn truncate_multiple_query_terms_finds_earliest() {
        let part1 = "a".repeat(200);
        let first_match = "ALPHA";
        let part2 = "b".repeat(200);
        let second_match = "BETA";
        let part3 = "c".repeat(200);
        let full = format!("{}{}{}{}{}", part1, first_match, part2, second_match, part3);

        let result = truncate_around_matches(&full, "BETA ALPHA", 100);
        // Should center on ALPHA (earlier match)
        assert!(result.contains("ALPHA"));
    }

    #[test]
    fn truncate_case_insensitive_match() {
        let before = "a".repeat(500);
        let match_text = "FindMe";
        let after = "b".repeat(500);
        let full = format!("{}{}{}", before, match_text, after);

        let result = truncate_around_matches(&full, "findme", 200);
        assert!(result.contains("FindMe"));
    }

    #[test]
    fn format_conversation_handles_text_messages() {
        let conv = Conversation::new_unvalidated(vec![
            Message::user().with_text("Hello"),
            Message::assistant().with_text("Hi there"),
        ]);

        let formatted = format_conversation(&conv);
        assert!(formatted.contains("[USER]: Hello"));
        assert!(formatted.contains("[ASSISTANT]: Hi there"));
    }
}
