#[allow(dead_code)]
mod common_tests;
use async_trait::async_trait;
use common_tests::fixtures::server::AcpServerConnection;
use common_tests::fixtures::{
    Connection, OpenAiFixture, Session, SessionData, TestConnectionConfig, run_test,
};
use common_tests::{
    run_close_session, run_config_mcp, run_config_option_mode_set, run_config_option_model_set,
    run_delete_session, run_fs_read_text_file_true, run_fs_write_text_file_false,
    run_fs_write_text_file_true, run_initialize_doesnt_hit_provider, run_list_sessions,
    run_load_mode, run_load_model, run_load_session_error, run_load_session_mcp, run_mode_set,
    run_model_list, run_model_set, run_model_set_error_session_not_found,
    run_new_session_returns_initial_config, run_permission_persistence, run_prompt_basic,
    run_prompt_codemode, run_prompt_error, run_prompt_image, run_prompt_image_attachment,
    run_prompt_mcp, run_prompt_model_mismatch, run_prompt_skill, run_shell_terminal_false,
    run_shell_terminal_true,
};
use fs_err as fs;
use goose::conversation::message::{Message, MessageContent};
use goose::model::ModelConfig;
use goose::providers::base::{MessageStream, Provider, ProviderUsage, Usage};
use goose::providers::errors::ProviderError;
use goose_acp::server::AcpProviderFactory;
use goose_test_support::{IgnoreSessionId, McpFixture, TEST_MODEL};
use rmcp::model::{CallToolRequestParams, Tool};
use sacp::schema::{ContentBlock, PromptRequest, SessionUpdate, TextContent};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Notify;

tests_config_option_set_error!(AcpServerConnection);
tests_mode_set_error!(AcpServerConnection);

const FALLBACK_CLOSE_PROMPT: &str = "Use the get_code tool ten times, then finish.";
const FALLBACK_CLOSE_MEMORY: &str =
    "transport-close fallback must wait for prompt teardown before draining review work";
const FALLBACK_CLOSE_SKILL: &str = "fallback-close-review";
const FALLBACK_CLOSE_SKILL_CONTENT: &str = "---\nname: fallback-close-review\ndescription: Preserve review writes during ACP transport-close fallback\n---\nWait for prompt teardown before transport-close cleanup drains background review work.\n";

struct FallbackCloseProviderState {
    allow_main_prompt_finish: Arc<Notify>,
    main_prompt_waiting: Arc<Notify>,
    main_prompt_final_calls: AtomicUsize,
    review_calls: AtomicUsize,
}

impl FallbackCloseProviderState {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            allow_main_prompt_finish: Arc::new(Notify::new()),
            main_prompt_waiting: Arc::new(Notify::new()),
            main_prompt_final_calls: AtomicUsize::new(0),
            review_calls: AtomicUsize::new(0),
        })
    }
}

struct FallbackCloseProvider {
    model_config: ModelConfig,
    state: Arc<FallbackCloseProviderState>,
}

struct GoosePathRootGuard;

impl Drop for GoosePathRootGuard {
    fn drop(&mut self) {
        std::env::remove_var("GOOSE_PATH_ROOT");
    }
}

impl FallbackCloseProvider {
    fn stream_once(message: Message) -> MessageStream {
        Box::pin(futures::stream::iter(vec![Ok((Some(message), None))]))
    }

    fn has_main_tool_responses(messages: &[Message]) -> bool {
        messages
            .iter()
            .flat_map(|message| message.content.iter())
            .filter(|content| match content {
                MessageContent::ToolResponse(response) => response.id.starts_with("main-tool-"),
                _ => false,
            })
            .count()
            >= 10
    }

    fn has_review_tool_responses(messages: &[Message]) -> bool {
        messages
            .iter()
            .flat_map(|message| message.content.iter())
            .any(|content| {
                matches!(
                    content,
                    MessageContent::ToolResponse(response)
                        if response.id == "review-memory" || response.id == "review-skill"
                )
            })
    }

    fn review_tool_requests() -> Message {
        let memory_args = serde_json::json!({
            "action": "add",
            "target": "memory",
            "content": FALLBACK_CLOSE_MEMORY,
        })
        .as_object()
        .unwrap()
        .clone();
        let skill_args = serde_json::json!({
            "name": FALLBACK_CLOSE_SKILL,
            "content": FALLBACK_CLOSE_SKILL_CONTENT,
        })
        .as_object()
        .unwrap()
        .clone();

        Message::assistant()
            .with_tool_request(
                "review-memory",
                Ok(CallToolRequestParams::new("memory").with_arguments(memory_args)),
            )
            .with_tool_request(
                "review-skill",
                Ok(CallToolRequestParams::new("create_skill").with_arguments(skill_args)),
            )
    }
}

#[async_trait]
impl Provider for FallbackCloseProvider {
    fn get_name(&self) -> &str {
        "mock"
    }

    async fn stream(
        &self,
        _model_config: &ModelConfig,
        _session_id: &str,
        system: &str,
        messages: &[Message],
        _tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        if system.starts_with("Summarize this tool call") {
            return Ok(Self::stream_once(
                Message::assistant().with_text("get code"),
            ));
        }

        if system.starts_with("You are a security reviewer.") {
            return Ok(Self::stream_once(
                Message::assistant().with_text("ALLOW\nsafe skill"),
            ));
        }

        if system.starts_with("You are reviewing a conversation to extract durable knowledge.") {
            self.state.review_calls.fetch_add(1, Ordering::SeqCst);
            if Self::has_review_tool_responses(messages) {
                return Ok(Self::stream_once(
                    Message::assistant().with_text("Nothing to save."),
                ));
            }
            return Ok(Self::stream_once(Self::review_tool_requests()));
        }

        if Self::has_main_tool_responses(messages) {
            let state = Arc::clone(&self.state);
            state.main_prompt_final_calls.fetch_add(1, Ordering::SeqCst);
            return Ok(Box::pin(async_stream::try_stream! {
                yield (Some(Message::assistant().with_text("done")), None);
                state.main_prompt_waiting.notify_waiters();
                state.allow_main_prompt_finish.notified().await;
            }));
        }

        let mut message = Message::assistant();
        for idx in 0..10 {
            let id = format!("main-tool-{idx}");
            message = message.with_tool_request(
                id,
                Ok(
                    CallToolRequestParams::new("mcp-fixture__get_code".to_string())
                        .with_arguments(serde_json::Map::new()),
                ),
            );
        }
        Ok(Self::stream_once(message))
    }

    async fn complete_fast(
        &self,
        _session_id: &str,
        _system: &str,
        _messages: &[Message],
        _tools: &[Tool],
    ) -> Result<(Message, ProviderUsage), ProviderError> {
        Ok((
            Message::assistant().with_text("get code"),
            ProviderUsage::new("mock".to_string(), Usage::default()),
        ))
    }

    fn get_model_config(&self) -> ModelConfig {
        self.model_config.clone()
    }

    async fn fetch_recommended_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec![self.model_config.model_name.clone()])
    }
}

fn fallback_close_provider_factory(state: Arc<FallbackCloseProviderState>) -> AcpProviderFactory {
    Arc::new(move |_provider_name, model_config, _extensions| {
        let state = Arc::clone(&state);
        Box::pin(async move {
            Ok(Arc::new(FallbackCloseProvider {
                model_config,
                state,
            }) as Arc<dyn Provider>)
        })
    })
}

#[test]
fn test_config_mcp() {
    run_test(async { run_config_mcp::<AcpServerConnection>().await });
}

#[test]
fn test_config_option_mode_set() {
    run_test(async { run_config_option_mode_set::<AcpServerConnection>().await });
}

#[test]
fn test_list_sessions() {
    run_test(async { run_list_sessions::<AcpServerConnection>().await });
}

#[test]
fn test_close_session() {
    run_test(async { run_close_session::<AcpServerConnection>().await });
}

#[test]
fn test_transport_close_drains_review_writes_without_explicit_session_close() {
    let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", None::<&str>)]);
    let temp_dir = tempfile::tempdir().unwrap();
    std::env::set_var("GOOSE_PATH_ROOT", temp_dir.path());
    let _path_guard = GoosePathRootGuard;

    run_test(async move {
        let state = FallbackCloseProviderState::new();
        let openai = OpenAiFixture::new(vec![], Arc::new(IgnoreSessionId)).await;
        let mcp = McpFixture::new(Arc::new(IgnoreSessionId)).await;
        let config = TestConnectionConfig {
            builtins: vec!["adaptive_memory".to_string()],
            data_root: temp_dir.path().to_path_buf(),
            mcp_servers: vec![sacp::schema::McpServer::Http(
                sacp::schema::McpServerHttp::new("mcp-fixture", &mcp.url),
            )],
            provider_factory: Some(fallback_close_provider_factory(Arc::clone(&state))),
            current_model: TEST_MODEL.to_string(),
            ..Default::default()
        };

        let mut conn = AcpServerConnection::new(config, openai).await;
        let SessionData { session, .. } = conn.new_session().await.unwrap();
        let session_id = session.session_id().clone();
        let prompt_cx = conn.cx().clone();

        let prompt_task = tokio::spawn(async move {
            prompt_cx
                .send_request(PromptRequest::new(
                    session_id,
                    vec![ContentBlock::Text(TextContent::new(FALLBACK_CLOSE_PROMPT))],
                ))
                .block_task()
                .await
        });

        conn.wait_for_updates(|updates| {
            updates.iter().any(|notification| {
                matches!(
                    &notification.update,
                    SessionUpdate::AgentMessageChunk(chunk)
                        if matches!(&chunk.content, ContentBlock::Text(text) if text.text == "done")
                )
            })
        })
        .await;
        state.main_prompt_waiting.notified().await;

        let disconnect_task = tokio::spawn(async move {
            conn.disconnect_transport().await;
        });
        tokio::task::yield_now().await;
        state.allow_main_prompt_finish.notify_waiters();
        disconnect_task.await.unwrap();

        let memory_path = temp_dir.path().join("config/memory/MEMORY.md");
        let skill_path = temp_dir
            .path()
            .join(format!("config/skills/{FALLBACK_CLOSE_SKILL}/SKILL.md"));

        let write_result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if memory_path.is_file() && skill_path.is_file() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await;
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), prompt_task).await;

        assert!(
            write_result.is_ok(),
            "expected fallback close to finish durable review writes: main_final_calls={}, review_calls={}, memory_exists={}, skill_exists={}",
            state.main_prompt_final_calls.load(Ordering::SeqCst),
            state.review_calls.load(Ordering::SeqCst),
            memory_path.is_file(),
            skill_path.is_file(),
        );

        assert!(
            memory_path.is_file(),
            "expected fallback close to persist memory review output"
        );
        assert!(
            skill_path.is_file(),
            "expected fallback close to persist skill review output"
        );
        assert!(
            fs::read_to_string(&memory_path)
                .unwrap()
                .contains(FALLBACK_CLOSE_MEMORY)
        );
        assert!(
            fs::read_to_string(&skill_path)
                .unwrap()
                .contains(FALLBACK_CLOSE_SKILL)
        );
    });
}

#[test]
fn test_config_option_model_set() {
    run_test(async { run_config_option_model_set::<AcpServerConnection>().await });
}

#[test]
fn test_delete_session() {
    run_test(async { run_delete_session::<AcpServerConnection>().await });
}

#[test]
fn test_fs_read_text_file_true() {
    run_test(async { run_fs_read_text_file_true::<AcpServerConnection>().await });
}

#[test]
fn test_fs_write_text_file_false() {
    run_test(async { run_fs_write_text_file_false::<AcpServerConnection>().await });
}

#[test]
fn test_fs_write_text_file_true() {
    run_test(async { run_fs_write_text_file_true::<AcpServerConnection>().await });
}

#[test]
fn test_initialize_doesnt_hit_provider() {
    run_test(async { run_initialize_doesnt_hit_provider::<AcpServerConnection>().await });
}

#[test]
fn test_load_mode() {
    run_test(async { run_load_mode::<AcpServerConnection>().await });
}

#[test]
fn test_load_model() {
    run_test(async { run_load_model::<AcpServerConnection>().await });
}

#[test]
fn test_load_session_error_session_not_found() {
    run_test(async { run_load_session_error::<AcpServerConnection>().await });
}

#[test]
fn test_load_session_mcp() {
    run_test(async { run_load_session_mcp::<AcpServerConnection>().await });
}

#[test]
fn test_mode_set() {
    run_test(async { run_mode_set::<AcpServerConnection>().await });
}

#[test]
fn test_model_list() {
    run_test(async { run_model_list::<AcpServerConnection>().await });
}

#[test]
fn test_new_session_returns_initial_config() {
    run_test(async { run_new_session_returns_initial_config::<AcpServerConnection>().await });
}

#[test]
fn test_model_set() {
    run_test(async { run_model_set::<AcpServerConnection>().await });
}

#[test]
fn test_model_set_error_session_not_found() {
    run_test(async { run_model_set_error_session_not_found::<AcpServerConnection>().await });
}

#[test]
fn test_permission_persistence() {
    run_test(async { run_permission_persistence::<AcpServerConnection>().await });
}

#[test]
fn test_prompt_basic() {
    run_test(async { run_prompt_basic::<AcpServerConnection>().await });
}

#[test]
fn test_prompt_codemode() {
    run_test(async { run_prompt_codemode::<AcpServerConnection>().await });
}

#[test]
fn test_prompt_error_session_not_found() {
    run_test(async { run_prompt_error::<AcpServerConnection>().await });
}

#[test]
fn test_prompt_image() {
    run_test(async { run_prompt_image::<AcpServerConnection>().await });
}

#[test]
fn test_prompt_image_attachment() {
    run_test(async { run_prompt_image_attachment::<AcpServerConnection>().await });
}

#[test]
fn test_prompt_mcp() {
    run_test(async { run_prompt_mcp::<AcpServerConnection>().await });
}

#[test]
fn test_prompt_model_mismatch() {
    run_test(async { run_prompt_model_mismatch::<AcpServerConnection>().await });
}

#[test]
fn test_prompt_skill() {
    run_test(async { run_prompt_skill::<AcpServerConnection>().await });
}

#[test]
fn test_shell_terminal_false() {
    run_test(async { run_shell_terminal_false::<AcpServerConnection>().await });
}

#[test]
fn test_shell_terminal_true() {
    run_test(async { run_shell_terminal_true::<AcpServerConnection>().await });
}
