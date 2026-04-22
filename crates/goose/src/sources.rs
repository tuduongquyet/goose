//! Filesystem-backed CRUD for [`SourceEntry`] values exchanged over ACP custom
//! methods. A source is a user-editable entity stored under a per-scope root
//! directory — `~/.agents/skills` for global sources and `<project>/.goose/skills`
//! for project-specific sources. Projects are stored under `Paths::data_dir()/projects/`.

use crate::agents::platform_extensions::parse_frontmatter;
use crate::config::paths::Paths;
use fs_err as fs;
use goose_sdk::custom_requests::{SourceEntry, SourceType};
use sacp::Error;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct SkillFront {
    #[serde(default)]
    description: String,
}

#[derive(Deserialize)]
struct ProjectFront {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default, flatten)]
    properties: HashMap<String, serde_json::Value>,
}

const GLOBAL_SKILLS_SUBPATH: &[&str] = &[".agents", "skills"];
const PROJECT_SKILLS_SUBPATH: &[&str] = &[".goose", "skills"];

fn home_dir() -> Result<PathBuf, Error> {
    dirs::home_dir()
        .ok_or_else(|| Error::internal_error().data("Could not determine home directory"))
}

fn skills_dir_global() -> Result<PathBuf, Error> {
    let mut dir = home_dir()?;
    for part in GLOBAL_SKILLS_SUBPATH {
        dir = dir.join(part);
    }
    Ok(dir)
}

fn skills_dir_project(project_dir: &str) -> Result<PathBuf, Error> {
    if project_dir.trim().is_empty() {
        return Err(
            Error::invalid_params().data("projectDir must not be empty when global is false")
        );
    }
    let mut dir = PathBuf::from(project_dir);
    for part in PROJECT_SKILLS_SUBPATH {
        dir = dir.join(part);
    }
    Ok(dir)
}

fn projects_dir() -> PathBuf {
    Paths::data_dir().join("projects")
}

fn source_base_dir(
    source_type: SourceType,
    global: bool,
    project_dir: Option<&str>,
) -> Result<PathBuf, Error> {
    match source_type {
        SourceType::Skill => {
            if global {
                skills_dir_global()
            } else {
                let pd = project_dir.ok_or_else(|| {
                    Error::invalid_params().data("projectDir is required when global is false")
                })?;
                skills_dir_project(pd)
            }
        }
        SourceType::Project => Ok(projects_dir()),
    }
}

/// Kebab-case validation: `^[a-z0-9]+(-[a-z0-9]+)*$`. Prevents path traversal
/// via names like `../../.ssh/authorized_keys`.
fn validate_source_name(name: &str) -> Result<(), Error> {
    if name.is_empty() {
        return Err(Error::invalid_params().data("Source name must not be empty"));
    }
    let mut expect_alnum = true;
    for ch in name.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            expect_alnum = false;
        } else if ch == '-' && !expect_alnum {
            expect_alnum = true;
        } else {
            return Err(Error::invalid_params().data(format!(
                "Invalid source name \"{}\". Names must be kebab-case (lowercase letters, digits, and hyphens; \
                 must not start or end with a hyphen or contain consecutive hyphens).",
                name
            )));
        }
    }
    if expect_alnum {
        return Err(Error::invalid_params().data(format!(
            "Invalid source name \"{}\". Names must not end with a hyphen.",
            name
        )));
    }
    Ok(())
}

fn build_skill_md(name: &str, description: &str, content: &str) -> String {
    // YAML single-quoted strings escape a literal single quote by doubling it.
    let safe_desc = description.replace('\'', "''");
    let mut md = format!("---\nname: {}\ndescription: '{}'\n---\n", name, safe_desc);
    if !content.is_empty() {
        md.push('\n');
        md.push_str(content);
        md.push('\n');
    }
    md
}

fn parse_skill_frontmatter(raw: &str) -> (String, String) {
    if !raw.trim_start().starts_with("---") {
        return (String::new(), raw.to_string());
    }
    match parse_frontmatter::<SkillFront>(raw) {
        Ok(Some((meta, body))) => (meta.description, body),
        _ => (String::new(), raw.to_string()),
    }
}

fn build_project_md(
    name: &str,
    description: &str,
    content: &str,
    properties: &HashMap<String, serde_json::Value>,
) -> String {
    let mut fm = serde_yaml::Mapping::new();
    fm.insert(
        serde_yaml::Value::String("name".into()),
        serde_yaml::Value::String(name.into()),
    );
    fm.insert(
        serde_yaml::Value::String("description".into()),
        serde_yaml::Value::String(description.into()),
    );
    for (k, v) in properties {
        if k == "name" || k == "description" {
            continue;
        }
        if let Ok(yv) = serde_yaml::to_value(v) {
            fm.insert(serde_yaml::Value::String(k.clone()), yv);
        }
    }
    let yaml = serde_yaml::to_string(&fm).unwrap_or_default();
    let mut md = format!("---\n{yaml}---\n");
    if !content.is_empty() {
        md.push('\n');
        md.push_str(content);
        md.push('\n');
    }
    md
}

fn parse_project_frontmatter(
    raw: &str,
) -> (String, String, String, HashMap<String, serde_json::Value>) {
    if !raw.trim_start().starts_with("---") {
        return (
            String::new(),
            String::new(),
            raw.to_string(),
            HashMap::new(),
        );
    }
    match parse_frontmatter::<ProjectFront>(raw) {
        Ok(Some((meta, body))) => (meta.name, meta.description, body, meta.properties),
        _ => (
            String::new(),
            String::new(),
            raw.to_string(),
            HashMap::new(),
        ),
    }
}

/// Resolve the on-disk path for a source entry.
fn source_path(source_type: SourceType, base: &Path, name: &str) -> PathBuf {
    match source_type {
        SourceType::Skill => base.join(name).join("SKILL.md"),
        SourceType::Project => base.join(format!("{name}.md")),
    }
}

/// The directory we report back in SourceEntry.directory.
fn source_dir(source_type: SourceType, base: &Path, name: &str) -> PathBuf {
    match source_type {
        SourceType::Skill => base.join(name),
        SourceType::Project => base.to_path_buf(),
    }
}

fn source_entry_with_props(
    source_type: SourceType,
    name: &str,
    description: &str,
    content: &str,
    dir: &Path,
    global: bool,
    properties: HashMap<String, serde_json::Value>,
) -> SourceEntry {
    SourceEntry {
        source_type,
        name: name.to_string(),
        description: description.to_string(),
        content: content.to_string(),
        directory: dir.to_string_lossy().to_string(),
        global,
        properties,
    }
}

pub fn create_source(
    source_type: SourceType,
    name: &str,
    description: &str,
    content: &str,
    global: bool,
    project_dir: Option<&str>,
    properties: HashMap<String, serde_json::Value>,
) -> Result<SourceEntry, Error> {
    validate_source_name(name)?;
    let base = source_base_dir(source_type, global, project_dir)?;
    let file = source_path(source_type, &base, name);
    let dir = source_dir(source_type, &base, name);

    match source_type {
        SourceType::Skill => {
            if dir.exists() {
                return Err(Error::invalid_params()
                    .data(format!("A source named \"{}\" already exists", name)));
            }
            fs::create_dir_all(&dir).map_err(|e| {
                Error::internal_error().data(format!("Failed to create source directory: {e}"))
            })?;
        }
        SourceType::Project => {
            if file.exists() {
                return Err(Error::invalid_params()
                    .data(format!("A source named \"{}\" already exists", name)));
            }
            fs::create_dir_all(&base).map_err(|e| {
                Error::internal_error().data(format!("Failed to create projects directory: {e}"))
            })?;
        }
    }

    let md = match source_type {
        SourceType::Skill => build_skill_md(name, description, content),
        SourceType::Project => build_project_md(name, description, content, &properties),
    };
    fs::write(&file, md)
        .map_err(|e| Error::internal_error().data(format!("Failed to write source file: {e}")))?;

    Ok(source_entry_with_props(
        source_type,
        name,
        description,
        content,
        &dir,
        global,
        properties,
    ))
}

pub fn update_source(
    source_type: SourceType,
    name: &str,
    description: &str,
    content: &str,
    global: bool,
    project_dir: Option<&str>,
    properties: HashMap<String, serde_json::Value>,
) -> Result<SourceEntry, Error> {
    validate_source_name(name)?;
    let base = source_base_dir(source_type, global, project_dir)?;
    let file = source_path(source_type, &base, name);
    let dir = source_dir(source_type, &base, name);

    if !file.exists() {
        return Err(Error::invalid_params().data(format!("Source \"{}\" not found", name)));
    }

    let md = match source_type {
        SourceType::Skill => build_skill_md(name, description, content),
        SourceType::Project => build_project_md(name, description, content, &properties),
    };
    fs::write(&file, md)
        .map_err(|e| Error::internal_error().data(format!("Failed to write source file: {e}")))?;

    Ok(source_entry_with_props(
        source_type,
        name,
        description,
        content,
        &dir,
        global,
        properties,
    ))
}

pub fn delete_source(
    source_type: SourceType,
    name: &str,
    global: bool,
    project_dir: Option<&str>,
) -> Result<(), Error> {
    validate_source_name(name)?;
    let base = source_base_dir(source_type, global, project_dir)?;

    match source_type {
        SourceType::Skill => {
            let dir = base.join(name);
            if !dir.exists() {
                return Err(Error::invalid_params().data(format!("Source \"{}\" not found", name)));
            }
            fs::remove_dir_all(&dir).map_err(|e| {
                Error::internal_error().data(format!("Failed to delete source: {e}"))
            })?;
        }
        SourceType::Project => {
            let file = base.join(format!("{name}.md"));
            if !file.exists() {
                return Err(Error::invalid_params().data(format!("Source \"{}\" not found", name)));
            }
            fs::remove_file(&file).map_err(|e| {
                Error::internal_error().data(format!("Failed to delete source: {e}"))
            })?;
        }
    }
    Ok(())
}

pub fn list_sources(
    source_type: Option<SourceType>,
    project_dir: Option<&str>,
    include_project_sources: bool,
) -> Result<Vec<SourceEntry>, Error> {
    let kinds: Vec<SourceType> = match source_type {
        Some(k) => vec![k],
        None => vec![SourceType::Skill, SourceType::Project],
    };

    let mut sources = Vec::new();
    for kind in kinds {
        match kind {
            SourceType::Skill => {
                if let Some(pd) = project_dir {
                    if !pd.trim().is_empty() {
                        let dir = skills_dir_project(pd)?;
                        sources.extend(read_skill_dir(&dir, false, Some(pd), None)?);
                    }
                }

                if include_project_sources {
                    let projects = read_project_dir()?;
                    for proj in &projects {
                        let dirs = proj
                            .properties
                            .get("workingDirs")
                            .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
                            .unwrap_or_default();
                        let project_name = proj
                            .properties
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&proj.name);
                        for wd in &dirs {
                            if project_dir == Some(wd.as_str()) {
                                continue; // already scanned above
                            }
                            let dir = skills_dir_project(wd)?;
                            sources.extend(read_skill_dir(
                                &dir,
                                false,
                                Some(wd),
                                Some(project_name),
                            )?);
                        }
                    }
                }

                let dir = skills_dir_global()?;
                sources.extend(read_skill_dir(&dir, true, None, None)?);
            }
            SourceType::Project => {
                sources.extend(read_project_dir()?);
            }
        }
    }
    sources.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(sources)
}

/// Read all skills from a skills directory.
///
/// * `project_root` – when present, stored as the `projectDir` property so the
///   frontend can pass it back for update/delete operations.
/// * `project_name` – human-readable project name shown as a badge in the UI.
fn read_skill_dir(
    dir: &Path,
    global: bool,
    project_root: Option<&str>,
    project_name: Option<&str>,
) -> Result<Vec<SourceEntry>, Error> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(dir)
        .map_err(|e| Error::internal_error().data(format!("Failed to read skills dir: {e}")))?;

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_md = path.join("SKILL.md");
        if !skill_md.exists() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let raw = fs::read_to_string(&skill_md).unwrap_or_default();
        let (description, content) = parse_skill_frontmatter(&raw);

        let mut props = HashMap::new();
        if let Some(pn) = project_name {
            props.insert(
                "projectName".into(),
                serde_json::Value::String(pn.to_string()),
            );
        }
        if let Some(pr) = project_root {
            props.insert(
                "projectDir".into(),
                serde_json::Value::String(pr.to_string()),
            );
        }

        out.push(source_entry_with_props(
            SourceType::Skill,
            &name,
            &description,
            &content,
            &path,
            global,
            props,
        ));
    }
    Ok(out)
}

fn read_project_dir() -> Result<Vec<SourceEntry>, Error> {
    let dir = projects_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(&dir)
        .map_err(|e| Error::internal_error().data(format!("Failed to read projects dir: {e}")))?;

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        let raw = fs::read_to_string(&path).unwrap_or_default();
        let (title, description, content, properties) = parse_project_frontmatter(&raw);
        let display_name = if title.is_empty() {
            name.clone()
        } else {
            title
        };
        out.push(source_entry_with_props(
            SourceType::Project,
            &name,
            &description,
            &content,
            &dir,
            true,
            {
                let mut p = properties;
                if display_name != name {
                    p.insert("title".into(), serde_json::Value::String(display_name));
                }
                p
            },
        ));
    }
    Ok(out)
}

/// Read a single project source by name.
/// Get the working directories configured for a project.
pub fn project_working_dirs(project_id: &str) -> Vec<String> {
    let entry = match read_project(project_id) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    entry
        .properties
        .get("workingDirs")
        .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
        .unwrap_or_default()
}

pub fn read_project(name: &str) -> Result<SourceEntry, Error> {
    validate_source_name(name)?;
    let dir = projects_dir();
    let file = dir.join(format!("{name}.md"));
    if !file.exists() {
        return Err(Error::invalid_params().data(format!("Project \"{}\" not found", name)));
    }
    let raw = fs::read_to_string(&file)
        .map_err(|e| Error::internal_error().data(format!("Failed to read project: {e}")))?;
    let (title, description, content, properties) = parse_project_frontmatter(&raw);
    let display_name = if title.is_empty() {
        name.to_string()
    } else {
        title
    };
    Ok(source_entry_with_props(
        SourceType::Project,
        name,
        &description,
        &content,
        &dir,
        true,
        {
            let mut p = properties;
            if display_name != *name {
                p.insert("title".into(), serde_json::Value::String(display_name));
            }
            p
        },
    ))
}

pub fn export_source(
    source_type: SourceType,
    name: &str,
    global: bool,
    project_dir: Option<&str>,
) -> Result<(String, String), Error> {
    validate_source_name(name)?;
    let base = source_base_dir(source_type, global, project_dir)?;
    let file = source_path(source_type, &base, name);

    if !file.exists() {
        return Err(Error::invalid_params().data(format!("Source \"{}\" not found", name)));
    }

    let raw = fs::read_to_string(&file)
        .map_err(|e| Error::internal_error().data(format!("Failed to read source: {e}")))?;

    let type_slug = match source_type {
        SourceType::Skill => "skill",
        SourceType::Project => "project",
    };

    let mut export = match source_type {
        SourceType::Skill => {
            let (description, content) = parse_skill_frontmatter(&raw);
            serde_json::json!({
                "version": 1,
                "type": type_slug,
                "name": name,
                "description": description,
                "content": content,
            })
        }
        SourceType::Project => {
            let (title, description, content, properties) = parse_project_frontmatter(&raw);
            let mut obj = serde_json::json!({
                "version": 1,
                "type": type_slug,
                "name": name,
                "title": title,
                "description": description,
                "content": content,
            });
            if !properties.is_empty() {
                obj["properties"] = serde_json::to_value(&properties).unwrap_or_default();
            }
            obj
        }
    };
    if export.get("version").is_none() {
        export["version"] = serde_json::json!(1);
    }

    let json = serde_json::to_string_pretty(&export)
        .map_err(|e| Error::internal_error().data(format!("Failed to serialize source: {e}")))?;
    let filename = format!("{}.{}.json", name, type_slug);
    Ok((json, filename))
}

pub fn import_sources(
    data: &str,
    global: bool,
    project_dir: Option<&str>,
) -> Result<Vec<SourceEntry>, Error> {
    let value: serde_json::Value = serde_json::from_str(data)
        .map_err(|e| Error::invalid_params().data(format!("Invalid JSON: {e}")))?;

    let version = value
        .get("version")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| Error::invalid_params().data("Missing or invalid \"version\" field"))?;
    if version != 1 {
        return Err(
            Error::invalid_params().data(format!("Unsupported source export version: {}", version))
        );
    }

    // Default to `skill` to preserve compatibility with pre-sources skill exports.
    let source_type = match value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("skill")
    {
        "skill" => SourceType::Skill,
        "project" => SourceType::Project,
        other => {
            return Err(Error::invalid_params().data(format!("Unsupported source type: {}", other)));
        }
    };

    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::invalid_params().data("Missing or invalid \"name\" field"))?
        .to_string();
    if name.is_empty() {
        return Err(Error::invalid_params().data("Source name must not be empty"));
    }

    let description = value
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Accept both the new `content` key and the legacy skills `instructions` key.
    let content = value
        .get("content")
        .or_else(|| value.get("instructions"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let properties: HashMap<String, serde_json::Value> = value
        .get("properties")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    validate_source_name(&name)?;

    let base = source_base_dir(source_type, global, project_dir)?;

    let mut final_name = name.clone();
    let exists = |n: &str| source_path(source_type, &base, n).exists();
    if exists(&final_name) {
        final_name = format!("{}-imported", name);
        let mut counter = 2u32;
        while exists(&final_name) {
            final_name = format!("{}-imported-{}", name, counter);
            counter += 1;
        }
    }

    create_source(
        source_type,
        &final_name,
        &description,
        &content,
        global,
        project_dir,
        properties,
    )
    .map(|entry| vec![entry])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    // Tests that set GOOSE_PATH_ROOT must run serially to avoid racing on the
    // global env var.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_root(f: impl FnOnce(&std::path::Path)) {
        let _guard = ENV_LOCK.lock().unwrap();
        let tmp = TempDir::new().unwrap();
        unsafe { std::env::set_var("GOOSE_PATH_ROOT", tmp.path()) };
        f(tmp.path());
        unsafe { std::env::remove_var("GOOSE_PATH_ROOT") };
    }

    #[test]
    fn kebab_case_validation() {
        assert!(validate_source_name("my-skill").is_ok());
        assert!(validate_source_name("abc123").is_ok());
        assert!(validate_source_name("").is_err());
        assert!(validate_source_name("-leading").is_err());
        assert!(validate_source_name("trailing-").is_err());
        assert!(validate_source_name("double--hyphen").is_err());
        assert!(validate_source_name("CAPS").is_err());
        assert!(validate_source_name("../escape").is_err());
    }

    #[test]
    fn create_list_update_delete_project_skill() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().to_str().unwrap();

        let created = create_source(
            SourceType::Skill,
            "my-skill",
            "does the thing",
            "step one\nstep two",
            false,
            Some(project),
            HashMap::new(),
        )
        .unwrap();
        assert_eq!(created.name, "my-skill");
        assert!(!created.global);
        assert!(PathBuf::from(&created.directory).join("SKILL.md").exists());

        let listed = list_sources(Some(SourceType::Skill), Some(project), false).unwrap();
        assert!(listed.iter().any(|s| s.name == "my-skill" && !s.global));

        let updated = update_source(
            SourceType::Skill,
            "my-skill",
            "now does a different thing",
            "step three",
            false,
            Some(project),
            HashMap::new(),
        )
        .unwrap();
        assert_eq!(updated.description, "now does a different thing");

        delete_source(SourceType::Skill, "my-skill", false, Some(project)).unwrap();
        assert!(!PathBuf::from(&created.directory).exists());
    }

    #[test]
    fn create_rejects_duplicate_name() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().to_str().unwrap();

        create_source(
            SourceType::Skill,
            "dup",
            "d",
            "c",
            false,
            Some(project),
            HashMap::new(),
        )
        .unwrap();
        let err = create_source(
            SourceType::Skill,
            "dup",
            "d",
            "c",
            false,
            Some(project),
            HashMap::new(),
        )
        .unwrap_err();
        assert!(format!("{:?}", err).contains("already exists"));
    }

    #[test]
    fn project_scope_requires_project_dir() {
        let err = create_source(
            SourceType::Skill,
            "x",
            "d",
            "c",
            false,
            None,
            HashMap::new(),
        )
        .unwrap_err();
        assert!(format!("{:?}", err).contains("projectDir"));
    }

    #[test]
    fn export_then_import_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let project_a = tmp.path().join("a");
        let project_b = tmp.path().join("b");
        std::fs::create_dir_all(&project_a).unwrap();
        std::fs::create_dir_all(&project_b).unwrap();

        create_source(
            SourceType::Skill,
            "portable",
            "describes itself",
            "body goes here",
            false,
            Some(project_a.to_str().unwrap()),
            HashMap::new(),
        )
        .unwrap();

        let (json, filename) = export_source(
            SourceType::Skill,
            "portable",
            false,
            Some(project_a.to_str().unwrap()),
        )
        .unwrap();
        assert_eq!(filename, "portable.skill.json");

        let imported = import_sources(&json, false, Some(project_b.to_str().unwrap())).unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].name, "portable");
        assert_eq!(imported[0].description, "describes itself");
        assert_eq!(imported[0].content, "body goes here");
    }

    #[test]
    fn import_collision_appends_suffix() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().to_str().unwrap();

        create_source(
            SourceType::Skill,
            "busy",
            "d",
            "c",
            false,
            Some(project),
            HashMap::new(),
        )
        .unwrap();

        let payload = serde_json::json!({
            "version": 1,
            "type": "skill",
            "name": "busy",
            "description": "d",
            "content": "c",
        })
        .to_string();
        let imported = import_sources(&payload, false, Some(project)).unwrap();
        assert_eq!(imported[0].name, "busy-imported");
    }

    #[test]
    fn project_crud_with_properties() {
        with_temp_root(|_root| {
            let mut props = HashMap::new();
            props.insert("icon".into(), serde_json::json!("🚀"));
            props.insert(
                "workingDirs".into(),
                serde_json::json!(["/Users/me/code/myapp"]),
            );

            let created = create_source(
                SourceType::Project,
                "my-app",
                "A web application",
                "Build with React.\nUse TypeScript.",
                true,
                None,
                props.clone(),
            )
            .unwrap();
            assert_eq!(created.name, "my-app");
            assert_eq!(created.description, "A web application");
            assert_eq!(created.content, "Build with React.\nUse TypeScript.");
            assert_eq!(created.properties.get("icon").unwrap(), "🚀");

            let listed = list_sources(Some(SourceType::Project), None, false).unwrap();
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].name, "my-app");
            assert_eq!(listed[0].properties.get("icon").unwrap(), "🚀");

            let read_back = read_project("my-app").unwrap();
            assert_eq!(read_back.description, "A web application");

            let updated = update_source(
                SourceType::Project,
                "my-app",
                "Updated description",
                "New instructions",
                true,
                None,
                {
                    let mut p = props.clone();
                    p.insert("color".into(), serde_json::json!("#ff0000"));
                    p
                },
            )
            .unwrap();
            assert_eq!(updated.description, "Updated description");
            assert!(updated.properties.contains_key("color"));

            delete_source(SourceType::Project, "my-app", true, None).unwrap();
            assert!(read_project("my-app").is_err());
        });
    }

    #[test]
    fn list_skills_includes_project_scoped() {
        with_temp_root(|root| {
            let work_dir = root.join("code").join("myapp");
            std::fs::create_dir_all(&work_dir).unwrap();

            let mut props = HashMap::new();
            props.insert(
                "workingDirs".into(),
                serde_json::json!([work_dir.to_str().unwrap()]),
            );
            props.insert("title".into(), serde_json::json!("My App"));
            create_source(
                SourceType::Project,
                "my-app",
                "test project",
                "",
                true,
                None,
                props,
            )
            .unwrap();

            create_source(
                SourceType::Skill,
                "local-helper",
                "helps locally",
                "do the thing",
                false,
                Some(work_dir.to_str().unwrap()),
                HashMap::new(),
            )
            .unwrap();

            let without = list_sources(Some(SourceType::Skill), None, false).unwrap();
            assert!(
                !without.iter().any(|s| s.name == "local-helper"),
                "should not appear without includeProjectSources"
            );

            let with = list_sources(Some(SourceType::Skill), None, true).unwrap();
            let found = with.iter().find(|s| s.name == "local-helper");
            assert!(found.is_some(), "should appear with includeProjectSources");
            let skill = found.unwrap();
            assert!(!skill.global);
            assert_eq!(
                skill.properties.get("projectName").and_then(|v| v.as_str()),
                Some("My App")
            );
            assert!(skill
                .properties
                .get("projectDir")
                .and_then(|v| v.as_str())
                .is_some());
        });
    }
}
