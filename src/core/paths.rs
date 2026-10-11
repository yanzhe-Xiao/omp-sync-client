use std::path::PathBuf;

pub fn get_agent_dir() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        home.join(".omp").join("agent")
    } else {
        PathBuf::from(".omp/agent")
    }
}

pub fn get_skills_dir() -> PathBuf {
    get_agent_dir().join("skills")
}

pub fn get_cache_file_path() -> PathBuf {
    get_agent_dir().join(".local_shell_path.txt")
}
