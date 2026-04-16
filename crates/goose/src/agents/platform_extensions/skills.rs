use super::{parse_frontmatter, Source, SourceKind};
use crate::agents::builtin_skills;
use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait};
use crate::agents::tool_execution::ToolCallContext;
use crate::config::paths::Paths;
use async_trait::async_trait;
use rmcp::model::{
    CallToolResult, Content, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ServerCapabilities, ServerNotification, Tool,
};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::warn;

pub static EXTENSION_NAME: &str = "skills";

#[derive(Debug, Deserialize)]
struct SkillMetadata {
    name: String,
    description: String,
}

fn parse_skill_content(content: &str, path: PathBuf) -> Option<Source> {
    let (metadata, body): (SkillMetadata, String) = match parse_frontmatter(content) {
        Ok(Some(parsed)) => parsed,
        Ok(None) => return None,
        Err(e) => {
            warn!("Failed to parse skill frontmatter: {}", e);
            return None;
        }
    };

    if metadata.name.contains('/') {
        warn!("Skill name '{}' contains '/', skipping", metadata.name);
        return None;
    }

    Some(Source {
        name: metadata.name,
        kind: SourceKind::Skill,
        description: metadata.description,
        path,
        content: body,
        supporting_files: Vec::new(),
    })
}

fn should_skip_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".git") | Some(".hg") | Some(".svn")
    )
}

fn walk_files_recursively<F, G>(
    dir: &Path,
    visited_dirs: &mut HashSet<PathBuf>,
    should_descend: &mut G,
    visit_file: &mut F,
) where
    F: FnMut(&Path),
    G: FnMut(&Path) -> bool,
{
    let canonical_dir = match std::fs::canonicalize(dir) {
        Ok(path) => path,
        Err(_) => return,
    };

    if !visited_dirs.insert(canonical_dir) {
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if should_descend(&path) {
                walk_files_recursively(&path, visited_dirs, should_descend, visit_file);
            }
        } else if path.is_file() {
            visit_file(&path);
        }
    }
}

fn scan_skills_from_dir(dir: &Path, seen: &mut HashSet<String>) -> Vec<Source> {
    let mut skill_files = Vec::new();
    let mut visited_dirs = HashSet::new();

    walk_files_recursively(
        dir,
        &mut visited_dirs,
        &mut |path| !should_skip_dir(path),
        &mut |path| {
            if path.file_name().and_then(|name| name.to_str()) == Some("SKILL.md") {
                skill_files.push(path.to_path_buf());
            }
        },
    );

    let mut sources = Vec::new();
    for skill_file in skill_files {
        let Some(skill_dir) = skill_file.parent() else {
            continue;
        };
        let content = match std::fs::read_to_string(&skill_file) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read skill file {}: {}", skill_file.display(), e);
                continue;
            }
        };

        if let Some(mut source) = parse_skill_content(&content, skill_dir.to_path_buf()) {
            if !seen.contains(&source.name) {
                // Find supporting files in the skill directory
                let mut files = Vec::new();
                let mut visited_support_dirs = HashSet::new();
                walk_files_recursively(
                    skill_dir,
                    &mut visited_support_dirs,
                    &mut |path| !should_skip_dir(path) && !path.join("SKILL.md").is_file(),
                    &mut |path| {
                        if path.file_name().and_then(|n| n.to_str()) != Some("SKILL.md") {
                            files.push(path.to_path_buf());
                        }
                    },
                );
                source.supporting_files = files;

                seen.insert(source.name.clone());
                sources.push(source);
            }
        }
    }
    sources
}

fn discover_skills(working_dir: &Path) -> Vec<Source> {
    let mut sources = Vec::new();
    let mut seen = HashSet::new();

    let home = dirs::home_dir();
    let config = Paths::config_dir();

    let local_dirs = vec![
        working_dir.join(".goose/skills"),
        working_dir.join(".claude/skills"),
        working_dir.join(".agents/skills"),
    ];

    let global_dirs: Vec<PathBuf> = [
        home.as_ref().map(|h| h.join(".agents/skills")),
        Some(config.join("skills")),
        home.as_ref().map(|h| h.join(".claude/skills")),
        home.as_ref().map(|h| h.join(".config/agents/skills")),
    ]
    .into_iter()
    .flatten()
    .collect();

    for dir in local_dirs {
        sources.extend(scan_skills_from_dir(&dir, &mut seen));
    }
    for dir in global_dirs {
        sources.extend(scan_skills_from_dir(&dir, &mut seen));
    }

    for content in builtin_skills::get_all() {
        if let Some(source) = parse_skill_content(content, PathBuf::new()) {
            if !seen.contains(&source.name) {
                seen.insert(source.name.clone());
                sources.push(Source {
                    kind: SourceKind::BuiltinSkill,
                    ..source
                });
            }
        }
    }

    sources
}

pub fn list_installed_skills(working_dir: Option<&Path>) -> Vec<Source> {
    let dir = working_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    discover_skills(&dir)
}

pub struct SkillsClient {
    info: InitializeResult,
    working_dir: PathBuf,
}

impl SkillsClient {
    pub fn new(context: PlatformExtensionContext) -> anyhow::Result<Self> {
        let working_dir = context
            .session
            .as_ref()
            .map(|s| s.working_dir.clone())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

        let mut instructions = String::new();
        if context.session.is_some() {
            let sources = discover_skills(&working_dir);
            let mut skills: Vec<&Source> = sources
                .iter()
                .filter(|s| s.kind == SourceKind::Skill || s.kind == SourceKind::BuiltinSkill)
                .collect();
            skills.sort_by(|a, b| (&a.name, &a.path).cmp(&(&b.name, &b.path)));

            if !skills.is_empty() {
                instructions.push_str(
                    "\n\nYou have these skills at your disposal, when it is clear they can help you solve a problem or you are asked to use them:",
                );
                for skill in &skills {
                    instructions.push_str(&format!("\n• {} - {}", skill.name, skill.description));
                }
            }
        }

        let info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(EXTENSION_NAME, "1.0.0").with_title("Skills"))
            .with_instructions(instructions);

        Ok(Self { info, working_dir })
    }
}

pub async fn handle_create_skill(arguments: Option<JsonObject>) -> Result<CallToolResult, Error> {
    let name = arguments
        .as_ref()
        .and_then(|a| a.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let content = arguments
        .as_ref()
        .and_then(|a| a.get("content"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if name.is_empty() || content.is_empty() {
        return Ok(CallToolResult::error(vec![Content::text(
            "Missing required parameters: name and content",
        )]));
    }

    if name.len() > 64
        || name.contains('/')
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '-' || c == '_' || c.is_ascii_digit())
    {
        return Ok(CallToolResult::error(vec![Content::text(
            "Invalid skill name. Use lowercase letters, hyphens, underscores, digits. Max 64 chars. No slashes.",
        )]));
    }

    // Validate frontmatter: must have opening/closing delimiters and required fields
    let parsed = parse_skill_content(content, PathBuf::new());
    if parsed.is_none() {
        return Ok(CallToolResult::error(vec![Content::text(
            "Invalid skill content. Must have valid YAML frontmatter with 'name' and 'description' fields:\n---\nname: my-skill\ndescription: What this skill does\n---\nInstructions here...",
        )]));
    }

    // Enforce that frontmatter name matches the directory name argument
    if let Some(ref source) = parsed {
        if source.name != name {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Frontmatter name '{}' does not match skill name argument '{}'. They must match.",
                source.name, name
            ))]));
        }
    }

    // Scan content for prompt injection / exfiltration patterns
    if let Some(err) = super::adaptive_memory::scan_content(content) {
        return Ok(CallToolResult::error(vec![Content::text(format!(
            "Rejected skill content: {}",
            err
        ))]));
    }

    let skill_dir = Paths::config_dir().join("skills").join(name);
    let skill_path = skill_dir.join("SKILL.md");

    let name = name.to_string();
    let content = content.to_string();
    let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
        std::fs::create_dir_all(&skill_dir)
            .map_err(|e| format!("Failed to create skill directory: {}", e))?;

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&skill_path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    format!(
                        "Skill '{}' already exists. Use patch_skill to update it.",
                        name
                    )
                } else {
                    format!("Failed to create skill: {}", e)
                }
            })?;

        use std::io::Write;
        file.write_all(content.as_bytes())
            .map_err(|e| format!("Failed to write skill: {}", e))?;

        Ok(format!(
            "Created skill '{}' at {}",
            name,
            skill_path.display()
        ))
    })
    .await
    .unwrap_or_else(|e| Err(format!("Internal error: {}", e)));

    match result {
        Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
        Err(msg) => Ok(CallToolResult::error(vec![Content::text(msg)])),
    }
}

pub async fn handle_patch_skill(
    working_dir: &Path,
    arguments: Option<JsonObject>,
) -> Result<CallToolResult, Error> {
    let name = arguments
        .as_ref()
        .and_then(|a| a.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let old_text = arguments
        .as_ref()
        .and_then(|a| a.get("old_text"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let new_text = arguments
        .as_ref()
        .and_then(|a| a.get("new_text"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if name.is_empty() || old_text.is_empty() || new_text.is_empty() {
        return Ok(CallToolResult::error(vec![Content::text(
            "Missing required parameters: name, old_text, and new_text. To delete a section, use remove instead.",
        )]));
    }

    let skills = discover_skills(working_dir);
    let goose_skills_prefix = Paths::config_dir().join("skills");
    // Prefer the goose-managed copy when a local skill shadows it by name
    let skill = skills
        .iter()
        .filter(|s| s.name == name)
        .find(|s| s.path.starts_with(&goose_skills_prefix))
        .or_else(|| skills.iter().find(|s| s.name == name));

    let Some(skill) = skill else {
        return Ok(CallToolResult::error(vec![Content::text(format!(
            "Skill '{}' not found",
            name
        ))]));
    };

    if matches!(skill.kind, SourceKind::BuiltinSkill) {
        return Ok(CallToolResult::error(vec![Content::text(
            "Cannot patch builtin skills. Create a new skill instead.",
        )]));
    }

    let goose_skills_dir = Paths::config_dir().join("skills");
    let canonical_skills_dir = std::fs::canonicalize(&goose_skills_dir).unwrap_or(goose_skills_dir);
    let canonical_skill_path =
        std::fs::canonicalize(&skill.path).unwrap_or_else(|_| skill.path.clone());
    if !canonical_skill_path.starts_with(&canonical_skills_dir) {
        return Ok(CallToolResult::error(vec![Content::text(
            "Cannot patch externally installed skills. Create a new skill in goose's directory instead.",
        )]));
    }

    let skill_path = skill.path.join("SKILL.md");

    // Reject symlinked SKILL.md files — the directory check passed but the file
    // itself could be a symlink pointing outside the goose-managed skills directory.
    if skill_path.is_symlink() {
        return Ok(CallToolResult::error(vec![Content::text(
            "Cannot patch a symlinked SKILL.md. Create a new skill instead.",
        )]));
    }

    let old_text = old_text.to_string();
    let new_text = new_text.to_string();
    let name = name.to_string();

    let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
        let content = std::fs::read_to_string(&skill_path)
            .map_err(|e| format!("Failed to read skill: {}", e))?;

        let matches: Vec<_> = content.match_indices(old_text.as_str()).collect();
        if matches.is_empty() {
            return Err(
                "old_text not found in skill. Load the skill first to see current content."
                    .to_string(),
            );
        }
        if matches.len() > 1 {
            return Err(format!(
                "old_text matches {} locations. Use a more specific string.",
                matches.len()
            ));
        }

        // Scan new_text for prompt injection / exfiltration patterns
        if let Some(err) = super::adaptive_memory::scan_content(&new_text) {
            return Err(format!("Rejected patch: {}", err));
        }

        let new_content = content.replacen(old_text.as_str(), &new_text, 1);
        std::fs::write(&skill_path, &new_content)
            .map_err(|e| format!("Failed to write skill: {}", e))?;

        Ok(format!(
            "Patched skill '{}' — replaced {} chars with {} chars",
            name,
            old_text.len(),
            new_text.len()
        ))
    })
    .await
    .unwrap_or_else(|e| Err(format!("Internal error: {}", e)));

    match result {
        Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
        Err(msg) => Ok(CallToolResult::error(vec![Content::text(msg)])),
    }
}

#[async_trait]
impl McpClientTrait for SkillsClient {
    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Name of the skill to load. Use \"skill-name/path\" to load a supporting file."
                }
            }
        });

        let tool = Tool::new(
            "load_skill",
            "Load a skill's full content into your context so you can follow its instructions.\n\n\
             Skills are listed in your system instructions. When you need to use one, \
             load it first to get the detailed instructions.\n\n\
             Examples:\n\
             - load_skill(name: \"gdrive\") → Loads the gdrive skill instructions\n\
             - load_skill(name: \"my-skill/template.md\") → Loads a supporting file"
                .to_string(),
            schema.as_object().unwrap().clone(),
        );

        Ok(ListToolsResult {
            tools: vec![tool],
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        _ctx: &ToolCallContext,
        name: &str,
        arguments: Option<JsonObject>,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        match name {
            "load_skill" => {}
            _ => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Unknown tool: {}",
                    name
                ))]));
            }
        }

        let skill_name = arguments
            .as_ref()
            .and_then(|args| args.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if skill_name.is_empty() {
            return Ok(CallToolResult::error(vec![Content::text(
                "Missing required parameter: name",
            )]));
        }

        let skills = discover_skills(&self.working_dir);

        // Direct skill match
        if let Some(skill) = skills.iter().find(|s| s.name == skill_name) {
            let mut output = format!(
                "# Loaded Skill: {} ({})\n\n{}\n",
                skill.name,
                skill.kind,
                skill.to_load_text()
            );

            if !skill.supporting_files.is_empty() {
                output.push_str(&format!(
                    "\n## Supporting Files\n\nSkill directory: {}\n\n",
                    skill.path.display()
                ));
                for file in &skill.supporting_files {
                    if let Ok(relative) = file.strip_prefix(&skill.path) {
                        let rel_str = relative.to_string_lossy().replace('\\', "/");
                        output.push_str(&format!(
                            "- {} → load_skill(name: \"{}/{}\")\n",
                            rel_str, skill.name, rel_str
                        ));
                    }
                }
            }

            output.push_str("\n---\nThis knowledge is now available in your context.");
            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        // Supporting file match (skill_name contains '/')
        if let Some((parent_skill_name, raw_relative_path)) = skill_name.split_once('/') {
            let relative_path = raw_relative_path.replace('\\', "/");
            if let Some(skill) = skills.iter().find(|s| {
                s.name == parent_skill_name
                    && matches!(s.kind, SourceKind::Skill | SourceKind::BuiltinSkill)
            }) {
                let canonical_skill_dir = skill
                    .path
                    .canonicalize()
                    .unwrap_or_else(|_| skill.path.clone());

                for file_path in &skill.supporting_files {
                    let Ok(rel) = file_path.strip_prefix(&skill.path) else {
                        continue;
                    };
                    if rel.to_string_lossy().replace('\\', "/") != relative_path {
                        continue;
                    }

                    return Ok(match file_path.canonicalize() {
                        Ok(canonical) if canonical.starts_with(&canonical_skill_dir) => {
                            match std::fs::read_to_string(&canonical) {
                                Ok(content) => {
                                    CallToolResult::success(vec![Content::text(format!(
                                        "# Loaded: {}\n\n{}\n\n---\nFile loaded into context.",
                                        skill_name, content
                                    ))])
                                }
                                Err(e) => CallToolResult::error(vec![Content::text(format!(
                                    "Failed to read '{}': {}",
                                    skill_name, e
                                ))]),
                            }
                        }
                        Ok(_) => CallToolResult::error(vec![Content::text(format!(
                            "Refusing to load '{}': resolves outside the skill directory",
                            skill_name
                        ))]),
                        Err(e) => CallToolResult::error(vec![Content::text(format!(
                            "Failed to resolve '{}': {}",
                            skill_name, e
                        ))]),
                    });
                }

                let available: Vec<String> = skill
                    .supporting_files
                    .iter()
                    .filter_map(|f| {
                        f.strip_prefix(&skill.path)
                            .ok()
                            .map(|r| r.to_string_lossy().replace('\\', "/"))
                    })
                    .take(10)
                    .collect();

                return Ok(if available.is_empty() {
                    CallToolResult::error(vec![Content::text(format!(
                        "Skill '{}' has no supporting files.",
                        skill.name
                    ))])
                } else {
                    CallToolResult::error(vec![Content::text(format!(
                        "File '{}' not found. Available: {}",
                        skill_name,
                        available.join(", ")
                    ))])
                });
            }
        }

        // No match — suggest similar skills
        let suggestions: Vec<&str> = skills
            .iter()
            .filter(|s| {
                s.name.to_lowercase().contains(&skill_name.to_lowercase())
                    || skill_name.to_lowercase().contains(&s.name.to_lowercase())
            })
            .take(3)
            .map(|s| s.name.as_str())
            .collect();

        Ok(if suggestions.is_empty() {
            CallToolResult::error(vec![Content::text(format!(
                "Skill '{}' not found.",
                skill_name
            ))])
        } else {
            CallToolResult::error(vec![Content::text(format!(
                "Skill '{}' not found. Did you mean: {}?",
                skill_name,
                suggestions.join(", ")
            ))])
        })
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }

    async fn subscribe(&self) -> mpsc::Receiver<ServerNotification> {
        let (_tx, rx) = mpsc::channel(1);
        rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Arc;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_load_skill_from_filesystem() {
        let temp_dir = TempDir::new().unwrap();
        let skill_dir = temp_dir.path().join(".goose/skills/my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-skill\ndescription: A test skill\n---\nDo the thing.",
        )
        .unwrap();

        let session = std::sync::Arc::new(crate::session::Session {
            working_dir: temp_dir.path().to_path_buf(),
            ..crate::session::Session::default()
        });
        let client = SkillsClient::new(PlatformExtensionContext {
            extension_manager: None,
            session_manager: Arc::new(crate::session::SessionManager::instance()),
            session: Some(session),
        })
        .unwrap();

        let ctx = ToolCallContext::new("test".to_string(), None, None);
        let args: JsonObject =
            serde_json::from_value(serde_json::json!({"name": "my-skill"})).unwrap();
        let result = client
            .call_tool(&ctx, "load_skill", Some(args), CancellationToken::new())
            .await
            .unwrap();

        assert!(!result.is_error.unwrap_or(false));
        let text = match &result.content[0].raw {
            rmcp::model::RawContent::Text(t) => &t.text,
            _ => panic!("expected text"),
        };
        assert!(text.contains("my-skill"));
        assert!(text.contains("Do the thing"));
    }

    #[tokio::test]
    async fn test_load_skill_not_found_returns_error() {
        let client = SkillsClient::new(PlatformExtensionContext {
            extension_manager: None,
            session_manager: Arc::new(crate::session::SessionManager::instance()),
            session: None,
        })
        .unwrap();

        let ctx = ToolCallContext::new("test".to_string(), None, None);
        let args: JsonObject =
            serde_json::from_value(serde_json::json!({"name": "nonexistent"})).unwrap();
        let result = client
            .call_tool(&ctx, "load_skill", Some(args), CancellationToken::new())
            .await
            .unwrap();

        assert!(result.is_error.unwrap_or(false));
    }

    // Helper to clean up test skills from real config dir
    struct TestSkillGuard(&'static str);
    impl TestSkillGuard {
        fn new(name: &'static str) -> Self {
            // Clean before test in case previous run panicked
            let _ = fs::remove_dir_all(Paths::config_dir().join("skills").join(name));
            Self(name)
        }
    }
    impl Drop for TestSkillGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(Paths::config_dir().join("skills").join(self.0));
        }
    }

    #[tokio::test]
    async fn test_create_skill_writes_to_disk() {
        let _guard = TestSkillGuard::new("_test-create-skill");

        let args: JsonObject = serde_json::from_value(serde_json::json!({
            "name": "_test-create-skill",
            "content": "---\nname: _test-create-skill\ndescription: A test\n---\nDo stuff."
        }))
        .unwrap();

        let result = handle_create_skill(Some(args)).await.unwrap();

        assert!(!result.is_error.unwrap_or(false));
        let text = match &result.content[0].raw {
            rmcp::model::RawContent::Text(t) => &t.text,
            _ => panic!("expected text"),
        };
        assert!(text.contains("Created skill"));

        let skill_path = Paths::config_dir().join("skills/_test-create-skill/SKILL.md");
        assert!(skill_path.exists());
        let content = fs::read_to_string(&skill_path).unwrap();
        assert!(content.contains("Do stuff."));
    }

    #[tokio::test]
    async fn test_create_skill_rejects_invalid_name() {
        let args: JsonObject = serde_json::from_value(serde_json::json!({
            "name": "BadName",
            "content": "---\nname: bad\ndescription: bad\n---\n"
        }))
        .unwrap();
        let result = handle_create_skill(Some(args)).await.unwrap();
        assert!(result.is_error.unwrap_or(false));

        let args: JsonObject = serde_json::from_value(serde_json::json!({
            "name": "bad/name",
            "content": "---\nname: bad\ndescription: bad\n---\n"
        }))
        .unwrap();
        let result = handle_create_skill(Some(args)).await.unwrap();
        assert!(result.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn test_create_skill_rejects_missing_frontmatter() {
        let args: JsonObject = serde_json::from_value(serde_json::json!({
            "name": "no-frontmatter",
            "content": "Just some text without frontmatter"
        }))
        .unwrap();
        let result = handle_create_skill(Some(args)).await.unwrap();
        assert!(result.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn test_patch_skill_updates_content() {
        let _guard = TestSkillGuard::new("_test-patch-skill");
        let temp_dir = TempDir::new().unwrap();

        let create_args: JsonObject = serde_json::from_value(serde_json::json!({
            "name": "_test-patch-skill",
            "content": "---\nname: _test-patch-skill\ndescription: Test\n---\nStep 1: Do old thing.\nStep 2: Done."
        }))
        .unwrap();
        let create_result = handle_create_skill(Some(create_args)).await.unwrap();
        assert!(!create_result.is_error.unwrap_or(false));

        let patch_args: JsonObject = serde_json::from_value(serde_json::json!({
            "name": "_test-patch-skill",
            "old_text": "Do old thing.",
            "new_text": "Do new thing."
        }))
        .unwrap();
        let result = handle_patch_skill(temp_dir.path(), Some(patch_args))
            .await
            .unwrap();

        assert!(!result.is_error.unwrap_or(false));

        let skill_path = Paths::config_dir().join("skills/_test-patch-skill/SKILL.md");
        let content = fs::read_to_string(&skill_path).unwrap();
        assert!(content.contains("Do new thing."));
        assert!(!content.contains("Do old thing."));
        assert!(content.contains("Step 2: Done."));
    }

    #[tokio::test]
    async fn test_patch_skill_rejects_ambiguous_match() {
        let temp_dir = TempDir::new().unwrap();
        let skill_dir = temp_dir.path().join(".goose/skills/ambig-test");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: ambig-test\ndescription: Test\n---\nfoo bar\nfoo baz",
        )
        .unwrap();

        let args: JsonObject = serde_json::from_value(serde_json::json!({
            "name": "ambig-test",
            "old_text": "foo",
            "new_text": "replaced"
        }))
        .unwrap();
        let result = handle_patch_skill(temp_dir.path(), Some(args))
            .await
            .unwrap();

        assert!(result.is_error.unwrap_or(false));
    }
}
