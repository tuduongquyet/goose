use std::path::PathBuf;

pub fn goose_home_dir() -> PathBuf {
    if let Ok(root) = std::env::var("GOOSE_PATH_ROOT") {
        return PathBuf::from(root);
    }

    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".goose")
}

pub fn goose_artifacts_dir() -> PathBuf {
    goose_home_dir().join("artifacts")
}

pub fn goose_projects_dir() -> PathBuf {
    goose_home_dir().join("projects")
}

pub fn goose_personas_path() -> PathBuf {
    goose_home_dir().join("personas.json")
}

pub fn goose_agents_dir() -> PathBuf {
    goose_home_dir().join("agents")
}

pub fn goose_avatars_dir() -> PathBuf {
    goose_home_dir().join("avatars")
}
