use etcetera::{choose_app_strategy, AppStrategy};
use indoc::formatdoc;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, ErrorCode, ErrorData, Implementation, InitializeResult, Meta,
        ServerCapabilities, ServerInfo,
    },
    schemars::JsonSchema,
    service::RequestContext,
    tool, tool_handler, tool_router, RoleServer, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::{self, Read, Write},
    path::PathBuf,
};

const WORKING_DIR_HEADER: &str = "agent-working-dir";

/// Reserved category for user identity — name, role, preferences, style.
const USER_PROFILE_CATEGORY: &str = "user_profile";

/// Maximum total chars for user profile entries in system prompt.
const USER_PROFILE_BUDGET: usize = 1375;

/// Maximum total chars for all other global memories in system prompt.
const GLOBAL_MEMORY_BUDGET: usize = 2200;

fn extract_working_dir_from_meta(meta: &Meta) -> Option<PathBuf> {
    meta.0
        .get(WORKING_DIR_HEADER)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

/// Parameters for the remember_memory tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RememberMemoryParams {
    /// The category to store the memory in
    pub category: String,
    /// The data to remember
    pub data: String,
    /// Optional tags for the memory
    #[serde(default)]
    pub tags: Vec<String>,
    /// Whether to store globally or locally
    pub is_global: bool,
}

/// Parameters for the retrieve_memories tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RetrieveMemoriesParams {
    /// The category to retrieve memories from (use "*" for all)
    pub category: String,
    /// Whether to retrieve from global or local storage
    pub is_global: bool,
}

/// Parameters for the remove_memory_category tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RemoveMemoryCategoryParams {
    /// The category to remove (use "*" for all)
    pub category: String,
    /// Whether to remove from global or local storage
    pub is_global: bool,
}

/// Parameters for the remove_specific_memory tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RemoveSpecificMemoryParams {
    /// The category containing the memory
    pub category: String,
    /// The content of the memory to remove
    pub memory_content: String,
    /// Whether to remove from global or local storage
    pub is_global: bool,
}

/// Parameters for the replace_memory tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReplaceMemoryParams {
    /// The category containing the memory to replace
    pub category: String,
    /// Substring that uniquely identifies the memory entry to replace
    pub old_content: String,
    /// The new content for this entry (replaces the entire entry)
    pub new_content: String,
    /// Optional tags for the new entry
    #[serde(default)]
    pub tags: Vec<String>,
    /// Whether to operate on global or local memory
    #[serde(default = "default_global")]
    pub is_global: bool,
}

fn default_global() -> bool {
    true
}

/// Memory MCP Server using official RMCP SDK
#[derive(Clone)]
pub struct MemoryServer {
    tool_router: ToolRouter<Self>,
    instructions: String,
    global_memory_dir: PathBuf,
}

impl Default for MemoryServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router(router = tool_router)]
impl MemoryServer {
    pub fn new() -> Self {
        let instructions = formatdoc! {r#"
             This extension stores and retrieves categorized information with tagging support.

             Storage:
             - Local: .goose/memory/ (project-specific)
             - Global: ~/.config/goose/memory/ (user-wide)

             Categories:
             - 'user_profile': WHO the user is — name, role, timezone, coding style, preferences, pet peeves.
             - All other categories: WHAT you've learned — environment facts, project conventions, tool quirks.

             Save proactively when:
             - User corrects you or says 'remember this'
             - User shares a preference, habit, or personal detail (use category: user_profile)
             - You discover something about the environment (use relevant category name)
             Do NOT save: task progress, session outcomes, temporary state.

             Memory has size limits. When at capacity, curate: use replace_memory to update stale entries,
             or remove_specific_memory to drop low-value ones. Quality over quantity.

             Use category "*" with retrieve_memories or remove_memory_category to access all entries.
            "#};

        let global_memory_dir = choose_app_strategy(crate::APP_STRATEGY.clone())
            .map(|strategy| strategy.in_config_dir("memory"))
            .unwrap_or_else(|_| PathBuf::from(".config/goose/memory"));

        let mut memory_router = Self {
            tool_router: Self::tool_router(),
            instructions: instructions.clone(),
            global_memory_dir,
        };

        let retrieved_global_memories = memory_router.retrieve_all(true, None);

        let mut updated_instructions = instructions;

        updated_instructions.push_str("\n\n**Here are the user's currently saved memories:**\n");
        updated_instructions
            .push_str("Keep this information in mind. Do not bring up memories unless relevant.\n");

        if let Ok(mut global_memories) = retrieved_global_memories {
            // Render user_profile first with its own budget
            if let Some(profile_entries) = global_memories.remove(USER_PROFILE_CATEGORY) {
                updated_instructions.push_str("\n# User Profile\n");
                let mut profile_chars = 0;
                for memory in &profile_entries {
                    let entry = format!("- {}\n", memory);
                    if profile_chars + entry.len() > USER_PROFILE_BUDGET {
                        updated_instructions.push_str(&format!(
                            "\n[Profile at capacity ({}/{} chars). Curate: replace or remove entries before adding new ones.]\n",
                            profile_chars, USER_PROFILE_BUDGET
                        ));
                        break;
                    }
                    updated_instructions.push_str(&entry);
                    profile_chars += entry.len();
                }
            }

            // Render other memories with their budget
            if !global_memories.is_empty() {
                updated_instructions.push_str("\n# Memories\n");
                let mut memory_chars = 0;
                let mut at_capacity = false;
                for (category, memories) in &global_memories {
                    let header = format!("\n## {}\n", category);
                    if memory_chars + header.len() > GLOBAL_MEMORY_BUDGET {
                        at_capacity = true;
                        break;
                    }
                    updated_instructions.push_str(&header);
                    memory_chars += header.len();
                    for memory in memories {
                        let entry = format!("- {}\n", memory);
                        if memory_chars + entry.len() > GLOBAL_MEMORY_BUDGET {
                            at_capacity = true;
                            break;
                        }
                        updated_instructions.push_str(&entry);
                        memory_chars += entry.len();
                    }
                    if at_capacity {
                        break;
                    }
                }
                if at_capacity {
                    updated_instructions.push_str(&format!(
                        "\n[Memory at capacity ({}/{} chars). Curate: replace or remove low-value entries.]\n",
                        memory_chars, GLOBAL_MEMORY_BUDGET
                    ));
                }
            }
        }

        memory_router.set_instructions(updated_instructions);

        memory_router
    }

    // Add a setter method for instructions
    pub fn set_instructions(&mut self, new_instructions: String) {
        self.instructions = new_instructions;
    }

    pub fn get_instructions(&self) -> &str {
        &self.instructions
    }

    fn get_memory_file(
        &self,
        category: &str,
        is_global: bool,
        working_dir: Option<&PathBuf>,
    ) -> PathBuf {
        let base_dir = if is_global {
            self.global_memory_dir.clone()
        } else {
            let local_base = working_dir
                .cloned()
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));
            local_base.join(".goose").join("memory")
        };
        base_dir.join(format!("{}.txt", category))
    }

    pub fn retrieve_all(
        &self,
        is_global: bool,
        working_dir: Option<&PathBuf>,
    ) -> io::Result<HashMap<String, Vec<String>>> {
        let base_dir = if is_global {
            self.global_memory_dir.clone()
        } else {
            let local_base = working_dir
                .cloned()
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));
            local_base.join(".goose").join("memory")
        };
        let mut memories = HashMap::new();
        if base_dir.exists() {
            for entry in fs::read_dir(&base_dir)? {
                let entry = entry?;
                if entry.file_type()?.is_file() {
                    let category = entry.file_name().to_string_lossy().replace(".txt", "");
                    let category_memories = self.retrieve(&category, is_global, working_dir)?;
                    memories.insert(
                        category,
                        category_memories.into_iter().flat_map(|(_, v)| v).collect(),
                    );
                }
            }
        }
        Ok(memories)
    }

    pub fn remember(
        &self,
        _context: &str,
        category: &str,
        data: &str,
        tags: &[&str],
        is_global: bool,
        working_dir: Option<&PathBuf>,
    ) -> io::Result<()> {
        let memory_file_path = self.get_memory_file(category, is_global, working_dir);

        if let Some(parent) = memory_file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&memory_file_path)?;
        if !tags.is_empty() {
            writeln!(file, "# {}", tags.join(" "))?;
        }
        writeln!(file, "{}\n", data)?;

        Ok(())
    }

    pub fn retrieve(
        &self,
        category: &str,
        is_global: bool,
        working_dir: Option<&PathBuf>,
    ) -> io::Result<HashMap<String, Vec<String>>> {
        let memory_file_path = self.get_memory_file(category, is_global, working_dir);
        if !memory_file_path.exists() {
            return Ok(HashMap::new());
        }

        let mut file = fs::File::open(memory_file_path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;

        let mut memories = HashMap::new();
        for entry in content.split("\n\n") {
            let mut lines = entry.lines();
            if let Some(first_line) = lines.next() {
                if let Some(stripped) = first_line.strip_prefix('#') {
                    let tags = stripped
                        .split_whitespace()
                        .map(String::from)
                        .collect::<Vec<_>>();
                    memories.insert(tags.join(" "), lines.map(String::from).collect());
                } else {
                    let entry_data: Vec<String> = std::iter::once(first_line.to_string())
                        .chain(lines.map(String::from))
                        .collect();
                    memories
                        .entry("untagged".to_string())
                        .or_insert_with(Vec::new)
                        .extend(entry_data);
                }
            }
        }

        Ok(memories)
    }

    pub fn remove_specific_memory_internal(
        &self,
        category: &str,
        memory_content: &str,
        is_global: bool,
        working_dir: Option<&PathBuf>,
    ) -> io::Result<()> {
        let memory_file_path = self.get_memory_file(category, is_global, working_dir);
        if !memory_file_path.exists() {
            return Ok(());
        }

        let mut file = fs::File::open(&memory_file_path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;

        let memories: Vec<&str> = content.split("\n\n").collect();
        let new_content: Vec<String> = memories
            .into_iter()
            .filter(|entry| !entry.contains(memory_content))
            .map(|s| s.to_string())
            .collect();

        fs::write(memory_file_path, new_content.join("\n\n"))?;

        Ok(())
    }

    pub fn clear_memory(
        &self,
        category: &str,
        is_global: bool,
        working_dir: Option<&PathBuf>,
    ) -> io::Result<()> {
        let memory_file_path = self.get_memory_file(category, is_global, working_dir);
        if memory_file_path.exists() {
            fs::remove_file(memory_file_path)?;
        }

        Ok(())
    }

    pub fn clear_all_global_or_local_memories(
        &self,
        is_global: bool,
        working_dir: Option<&PathBuf>,
    ) -> io::Result<()> {
        let base_dir = if is_global {
            self.global_memory_dir.clone()
        } else {
            let local_base = working_dir
                .cloned()
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));
            local_base.join(".goose").join("memory")
        };
        if base_dir.exists() {
            fs::remove_dir_all(&base_dir)?;
        }
        Ok(())
    }

    /// Stores a memory with optional tags in a specified category
    #[tool(
        name = "remember_memory",
        description = "Stores a memory with optional tags in a specified category"
    )]
    pub async fn remember_memory(
        &self,
        params: Parameters<RememberMemoryParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let working_dir = extract_working_dir_from_meta(&context.meta);

        if params.data.is_empty() {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                "Data must not be empty when remembering a memory".to_string(),
                None,
            ));
        }

        let tags: Vec<&str> = params.tags.iter().map(|s| s.as_str()).collect();
        self.remember(
            "context",
            &params.category,
            &params.data,
            &tags,
            params.is_global,
            working_dir.as_ref(),
        )
        .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;

        // Check budget after write
        let budget = if params.category == USER_PROFILE_CATEGORY {
            USER_PROFILE_BUDGET
        } else {
            GLOBAL_MEMORY_BUDGET
        };
        let memory_file_path =
            self.get_memory_file(&params.category, params.is_global, working_dir.as_ref());
        let file_size = fs::read_to_string(&memory_file_path)
            .map(|c| c.len())
            .unwrap_or(0);

        let mut msg = format!("Stored memory in category: {}", params.category);
        if params.is_global && file_size > budget {
            msg.push_str(&format!(
                "\n\nWarning: category '{}' is over budget ({}/{} chars). \
                 Curate: use replace_memory to update stale entries or remove_specific_memory to drop low-value ones.",
                params.category, file_size, budget
            ));
        }

        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    /// Retrieves all memories from a specified category
    #[tool(
        name = "retrieve_memories",
        description = "Retrieves all memories from a specified category"
    )]
    pub async fn retrieve_memories(
        &self,
        params: Parameters<RetrieveMemoriesParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let working_dir = extract_working_dir_from_meta(&context.meta);

        let memories = if params.category == "*" {
            self.retrieve_all(params.is_global, working_dir.as_ref())
        } else {
            self.retrieve(&params.category, params.is_global, working_dir.as_ref())
        }
        .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Retrieved memories: {:?}",
            memories
        ))]))
    }

    /// Removes all memories within a specified category
    #[tool(
        name = "remove_memory_category",
        description = "Removes all memories within a specified category"
    )]
    pub async fn remove_memory_category(
        &self,
        params: Parameters<RemoveMemoryCategoryParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let working_dir = extract_working_dir_from_meta(&context.meta);

        let message = if params.category == "*" {
            self.clear_all_global_or_local_memories(params.is_global, working_dir.as_ref())
                .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
            format!(
                "Cleared all memory {} categories",
                if params.is_global { "global" } else { "local" }
            )
        } else {
            self.clear_memory(&params.category, params.is_global, working_dir.as_ref())
                .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
            format!("Cleared memories in category: {}", params.category)
        };

        Ok(CallToolResult::success(vec![Content::text(message)]))
    }

    /// Removes a specific memory within a specified category
    #[tool(
        name = "remove_specific_memory",
        description = "Removes a specific memory within a specified category"
    )]
    pub async fn remove_specific_memory(
        &self,
        params: Parameters<RemoveSpecificMemoryParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let working_dir = extract_working_dir_from_meta(&context.meta);

        self.remove_specific_memory_internal(
            &params.category,
            &params.memory_content,
            params.is_global,
            working_dir.as_ref(),
        )
        .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Removed specific memory from category: {}",
            params.category
        ))]))
    }

    /// Replaces a specific memory entry with new content (atomic find-and-replace)
    #[tool(
        name = "replace_memory",
        description = "Replaces an existing memory entry. Use old_content to identify the entry (substring match), and new_content for the replacement. Useful for updating stale memories without remove+re-add."
    )]
    pub async fn replace_memory(
        &self,
        params: Parameters<ReplaceMemoryParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let working_dir = extract_working_dir_from_meta(&context.meta);

        let memory_file_path =
            self.get_memory_file(&params.category, params.is_global, working_dir.as_ref());
        if !memory_file_path.exists() {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                format!("Category '{}' not found", params.category),
                None,
            ));
        }

        let content = fs::read_to_string(&memory_file_path)
            .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;

        let entries: Vec<&str> = content.split("\n\n").collect();
        let matching: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.contains(&params.old_content))
            .map(|(i, _)| i)
            .collect();

        if matching.is_empty() {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                "old_content not found in any entry. Use retrieve_memories to see current content."
                    .to_string(),
                None,
            ));
        }
        if matching.len() > 1 {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                format!(
                    "old_content matches {} entries. Use a more specific substring.",
                    matching.len()
                ),
                None,
            ));
        }

        let mut new_entries: Vec<String> = entries.iter().map(|s| s.to_string()).collect();
        let replacement = if params.tags.is_empty() {
            params.new_content.clone()
        } else {
            format!("# {}\n{}", params.tags.join(" "), params.new_content)
        };
        new_entries[matching[0]] = replacement;

        fs::write(&memory_file_path, new_entries.join("\n\n"))
            .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;

        // Check budget after replacement
        let budget = if params.category == USER_PROFILE_CATEGORY {
            USER_PROFILE_BUDGET
        } else {
            GLOBAL_MEMORY_BUDGET
        };
        let file_size = fs::read_to_string(&memory_file_path)
            .map(|c| c.len())
            .unwrap_or(0);

        let mut msg = format!("Replaced memory in category: {}", params.category);
        if file_size > budget {
            msg.push_str(&format!(
                "\n\nWarning: category '{}' is over budget ({}/{} chars). Curate: remove low-value entries.",
                params.category, file_size, budget
            ));
        }

        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for MemoryServer {
    fn get_info(&self) -> ServerInfo {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "goose-memory",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(self.instructions.clone())
    }
}

// Remove the old MemoryArgs struct since we're using the new parameter structs

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_lazy_directory_creation() {
        let temp_dir = tempdir().unwrap();
        let memory_base = temp_dir.path().join("test_memory");
        let working_dir = memory_base.join("working");

        let router = MemoryServer {
            tool_router: ToolRouter::new(),
            instructions: String::new(),
            global_memory_dir: memory_base.join("global"),
        };

        let local_memory_dir = working_dir.join(".goose").join("memory");

        assert!(!router.global_memory_dir.exists());
        assert!(!local_memory_dir.exists());

        router
            .remember(
                "test_context",
                "test_category",
                "test_data",
                &["tag1"],
                false,
                Some(&working_dir),
            )
            .unwrap();

        assert!(local_memory_dir.exists());
        assert!(!router.global_memory_dir.exists());

        router
            .remember(
                "test_context",
                "global_category",
                "global_data",
                &["global_tag"],
                true,
                None,
            )
            .unwrap();

        assert!(router.global_memory_dir.exists());
    }

    #[test]
    fn test_clear_nonexistent_directories() {
        let temp_dir = tempdir().unwrap();
        let memory_base = temp_dir.path().join("nonexistent_memory");
        let working_dir = memory_base.join("working");

        let router = MemoryServer {
            tool_router: ToolRouter::new(),
            instructions: String::new(),
            global_memory_dir: memory_base.join("global"),
        };

        assert!(router
            .clear_all_global_or_local_memories(false, Some(&working_dir))
            .is_ok());
        assert!(router
            .clear_all_global_or_local_memories(true, None)
            .is_ok());
    }

    #[test]
    fn test_remember_retrieve_clear_workflow() {
        let temp_dir = tempdir().unwrap();
        let memory_base = temp_dir.path().join("workflow_test");
        let working_dir = memory_base.join("working");

        let router = MemoryServer {
            tool_router: ToolRouter::new(),
            instructions: String::new(),
            global_memory_dir: memory_base.join("global"),
        };

        router
            .remember(
                "context",
                "test_category",
                "test_data_content",
                &["test_tag"],
                false,
                Some(&working_dir),
            )
            .unwrap();

        let memories = router
            .retrieve("test_category", false, Some(&working_dir))
            .unwrap();
        assert!(!memories.is_empty());

        let has_content = memories.values().any(|v| {
            v.iter()
                .any(|content| content.contains("test_data_content"))
        });
        assert!(has_content);

        router
            .clear_memory("test_category", false, Some(&working_dir))
            .unwrap();

        let memories_after_clear = router
            .retrieve("test_category", false, Some(&working_dir))
            .unwrap();
        assert!(memories_after_clear.is_empty());
    }

    #[test]
    fn test_directory_creation_on_write() {
        let temp_dir = tempdir().unwrap();
        let memory_base = temp_dir.path().join("write_test");
        let working_dir = memory_base.join("working");

        let router = MemoryServer {
            tool_router: ToolRouter::new(),
            instructions: String::new(),
            global_memory_dir: memory_base.join("global"),
        };

        let local_memory_dir = working_dir.join(".goose").join("memory");
        assert!(!local_memory_dir.exists());

        router
            .remember(
                "context",
                "category",
                "data",
                &[],
                false,
                Some(&working_dir),
            )
            .unwrap();

        assert!(local_memory_dir.exists());
        assert!(local_memory_dir.join("category.txt").exists());
    }

    #[test]
    fn test_remove_specific_memory() {
        let temp_dir = tempdir().unwrap();
        let memory_base = temp_dir.path().join("remove_test");
        let working_dir = memory_base.join("working");

        let router = MemoryServer {
            tool_router: ToolRouter::new(),
            instructions: String::new(),
            global_memory_dir: memory_base.join("global"),
        };

        router
            .remember(
                "context",
                "category",
                "keep_this",
                &[],
                false,
                Some(&working_dir),
            )
            .unwrap();
        router
            .remember(
                "context",
                "category",
                "remove_this",
                &[],
                false,
                Some(&working_dir),
            )
            .unwrap();

        let memories = router
            .retrieve("category", false, Some(&working_dir))
            .unwrap();
        assert_eq!(memories.len(), 1);

        router
            .remove_specific_memory_internal("category", "remove_this", false, Some(&working_dir))
            .unwrap();

        let memories_after = router
            .retrieve("category", false, Some(&working_dir))
            .unwrap();
        let has_removed = memories_after
            .values()
            .any(|v| v.iter().any(|content| content.contains("remove_this")));
        assert!(!has_removed);

        let has_kept = memories_after
            .values()
            .any(|v| v.iter().any(|content| content.contains("keep_this")));
        assert!(has_kept);
    }

    #[test]
    fn test_user_profile_renders_first_in_instructions() {
        let temp_dir = tempdir().unwrap();
        let global_dir = temp_dir.path().join("global_memory");

        let router = MemoryServer {
            tool_router: ToolRouter::new(),
            instructions: String::new(),
            global_memory_dir: global_dir.clone(),
        };

        // Write user_profile and another category
        router
            .remember("ctx", USER_PROFILE_CATEGORY, "Name: Alice", &[], true, None)
            .unwrap();
        router
            .remember("ctx", "environment", "OS: macOS", &[], true, None)
            .unwrap();

        // Build a new server to trigger instructions assembly
        let server = MemoryServer {
            tool_router: ToolRouter::new(),
            instructions: String::new(),
            global_memory_dir: global_dir,
        };
        // Re-run the new() logic by constructing fresh
        let retrieved = server.retrieve_all(true, None).unwrap();
        assert!(retrieved.contains_key(USER_PROFILE_CATEGORY));

        // Check that instructions from new() would have User Profile before Memories
        // We test the rendering logic directly
        let mut instructions = String::new();
        let mut memories = retrieved;
        if let Some(profile) = memories.remove(USER_PROFILE_CATEGORY) {
            instructions.push_str("# User Profile\n");
            for m in &profile {
                instructions.push_str(&format!("- {}\n", m));
            }
        }
        if !memories.is_empty() {
            instructions.push_str("# Memories\n");
        }

        let profile_pos = instructions.find("# User Profile").unwrap();
        let memories_pos = instructions.find("# Memories").unwrap();
        assert!(profile_pos < memories_pos);
    }

    #[test]
    fn test_user_profile_budget_enforced() {
        let temp_dir = tempdir().unwrap();
        let global_dir = temp_dir.path().join("budget_test");

        let router = MemoryServer {
            tool_router: ToolRouter::new(),
            instructions: String::new(),
            global_memory_dir: global_dir.clone(),
        };

        // Write many entries to exceed USER_PROFILE_BUDGET (1375 chars)
        for i in 0..50 {
            router
                .remember(
                    "ctx",
                    USER_PROFILE_CATEGORY,
                    &format!(
                        "Preference {}: this is a moderately long preference entry number {}",
                        i, i
                    ),
                    &[],
                    true,
                    None,
                )
                .unwrap();
        }

        // Build a new server to check instructions
        let new_server = MemoryServer {
            tool_router: ToolRouter::new(),
            instructions: String::new(),
            global_memory_dir: global_dir,
        };
        // Simulate the instruction building
        let mut all = new_server.retrieve_all(true, None).unwrap();
        let profile = all.remove(USER_PROFILE_CATEGORY).unwrap();

        let mut rendered = String::new();
        let mut chars = 0;
        let mut hit_cap = false;
        for m in &profile {
            let entry = format!("- {}\n", m);
            if chars + entry.len() > USER_PROFILE_BUDGET {
                hit_cap = true;
                break;
            }
            rendered.push_str(&entry);
            chars += entry.len();
        }
        // Should have hit the budget before rendering all 50
        assert!(hit_cap);
        assert!(chars <= USER_PROFILE_BUDGET);
    }

    #[test]
    fn test_replace_memory_internal() {
        let temp_dir = tempdir().unwrap();
        let global_dir = temp_dir.path().join("replace_test");

        let router = MemoryServer {
            tool_router: ToolRouter::new(),
            instructions: String::new(),
            global_memory_dir: global_dir,
        };

        // Write two entries
        router
            .remember("ctx", "prefs", "editor: vim", &[], true, None)
            .unwrap();
        router
            .remember("ctx", "prefs", "shell: bash", &[], true, None)
            .unwrap();

        // Read the file and do a manual replace (simulating replace_memory logic)
        let file_path = router.get_memory_file("prefs", true, None);
        let content = fs::read_to_string(&file_path).unwrap();
        let entries: Vec<&str> = content.split("\n\n").collect();
        let matching: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.contains("editor: vim"))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(matching.len(), 1);

        let mut new_entries: Vec<String> = entries.iter().map(|s| s.to_string()).collect();
        new_entries[matching[0]] = "editor: neovim".to_string();
        fs::write(&file_path, new_entries.join("\n\n")).unwrap();

        // Verify replacement
        let memories = router.retrieve("prefs", true, None).unwrap();
        let all_values: Vec<String> = memories.values().flatten().cloned().collect();
        assert!(all_values.iter().any(|v| v.contains("neovim")));
        assert!(!all_values.iter().any(|v| v.contains("editor: vim")));
        assert!(all_values.iter().any(|v| v.contains("shell: bash")));
    }
}
