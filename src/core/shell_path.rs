use std::fs;
use std::path::{Path, PathBuf};
use regex::Regex;
use serde_yaml::Value;
use walkdir::WalkDir;

use super::paths::get_cache_file_path;

pub fn is_windows() -> bool {
    cfg!(target_os = "windows")
}

/// Detects or recovers the local Git Bash path.
/// Priority on Windows:
/// 1. Current value in local ~/.omp/agent/config.yml
/// 2. Cached value in ~/.omp/agent/.local_shell_path.txt
/// 3. Known UGit / Git installation paths on Windows
pub fn detect_local_bash_path(local_config_yaml: &str) -> Option<String> {
    if !is_windows() {
        return Some("/bin/bash".to_string());
    }

    // 1. Try parse from existing local config
    if !local_config_yaml.trim().is_empty() {
        if let Ok(Value::Mapping(map)) = serde_yaml::from_str::<Value>(local_config_yaml) {
            if let Some(val) = map.get(&Value::String("shellPath".to_string())) {
                if let Some(path_str) = val.as_str() {
                    let trimmed = path_str.trim();
                    if !trimmed.is_empty() && (trimmed.contains(":\\") || trimmed.to_lowercase().ends_with(".exe")) {
                        save_cache(trimmed);
                        return Some(trimmed.to_string());
                    }
                }
            }
        }
    }

    // 2. Try read from persistent cache
    let cache_file = get_cache_file_path();
    if cache_file.exists() {
        if let Ok(content) = fs::read_to_string(&cache_file) {
            let cached = content.trim();
            if !cached.is_empty() && Path::new(cached).exists() {
                return Some(cached.to_string());
            }
        }
    }

    // 3. Search well-known Windows locations
    #[cfg(target_os = "windows")]
    {
        let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();
        let program_files = std::env::var("ProgramFiles").unwrap_or_default();

        // Check UGit installations
        if !local_app_data.is_empty() {
            let ugit_dir = PathBuf::from(&local_app_data).join("UGit");
            if ugit_dir.exists() {
                for entry in WalkDir::new(ugit_dir).into_iter().filter_map(|e| e.ok()) {
                    if entry.file_type().is_file() {
                        if let Some(name) = entry.file_name().to_str() {
                            if name.eq_ignore_ascii_case("bash.exe") {
                                let path_str = entry.path().to_string_lossy().to_string();
                                save_cache(&path_str);
                                return Some(path_str);
                            }
                        }
                    }
                }
            }
        }

        // Check standard Git candidates
        let candidates = [
            PathBuf::from(&program_files).join("Git").join("bin").join("bash.exe"),
            PathBuf::from(&program_files).join("Git").join("usr").join("bin").join("bash.exe"),
            PathBuf::from(r"C:\Program Files\Git\bin\bash.exe"),
            PathBuf::from(r"C:\Program Files\Git\usr\bin\bash.exe"),
            PathBuf::from(&local_app_data).join("Programs").join("Git").join("bin").join("bash.exe"),
        ];

        for cand in &candidates {
            if cand.exists() {
                let path_str = cand.to_string_lossy().to_string();
                save_cache(&path_str);
                return Some(path_str);
            }
        }
    }

    None
}

pub fn save_cache(bash_path: &str) {
    let cache_file = get_cache_file_path();
    if let Some(parent) = cache_file.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(cache_file, bash_path.trim());
}

/// Prepares config for UPLOAD to Linux server:
/// Strips Windows 'shellPath' so Linux uses default /bin/bash, caching the Windows path locally.
pub fn sanitize_for_upload(local_yaml: &str) -> (String, Option<String>) {
    if local_yaml.trim().is_empty() {
        return (local_yaml.to_string(), None);
    }

    if let Ok(mut val) = serde_yaml::from_str::<Value>(local_yaml) {
        if let Value::Mapping(map) = &mut val {
            let key = Value::String("shellPath".to_string());
            if let Some(old_val) = map.remove(&key) {
                let stripped = match old_val {
                    Value::String(s) => s,
                    _ => old_val.as_str().unwrap_or("").to_string(),
                };

                if !stripped.is_empty() {
                    save_cache(&stripped);
                }

                if let Ok(clean) = serde_yaml::to_string(&val) {
                    return (clean, Some(stripped));
                }
            }
        }
    }

    // Fallback regex strip if YAML parsing fails or is non-standard
    let re_find = Regex::new(r"(?m)^shellPath:\s*(.+)$").unwrap();
    let mut stripped_res = None;
    if let Some(cap) = re_find.captures(local_yaml) {
        if let Some(m) = cap.get(1) {
            let path_val = m.as_str().trim().trim_matches('\'').trim_matches('"').to_string();
            save_cache(&path_val);
            stripped_res = Some(path_val);
        }
    }

    let re_replace = Regex::new(r"(?m)^shellPath:\s*.*(?:\r?\n|$)").unwrap();
    let cleaned = re_replace.replace_all(local_yaml, "").to_string();
    (cleaned, stripped_res)
}

/// Processes config after DOWNLOAD from Linux server:
/// On Windows: Injects local Windows 'shellPath' into the pulled config.
/// On Linux: Leaves config untouched or native.
pub fn restore_after_download(remote_yaml: &str, current_local_yaml: Option<&str>) -> (String, String) {
    if !is_windows() {
        return (remote_yaml.to_string(), "Linux 原生环境无需注入 Windows Bash 路径".to_string());
    }

    let bash_path = detect_local_bash_path(current_local_yaml.unwrap_or(""))
        .unwrap_or_else(|| {
            let username = std::env::var("USERNAME").unwrap_or_else(|_| "user".to_string());
            format!(r"C:\Users\{username}\AppData\Local\UGit\app-5.48.0\resources\app\git\usr\bin\bash.exe")
        });

    if let Ok(mut val) = serde_yaml::from_str::<Value>(remote_yaml) {
        if let Value::Mapping(map) = &mut val {
            map.insert(
                Value::String("shellPath".to_string()),
                Value::String(bash_path.clone()),
            );
            if let Ok(merged) = serde_yaml::to_string(&val) {
                return (merged, bash_path);
            }
        }
    }

    // Fallback prepend
    let merged = format!("shellPath: {}\n{}", bash_path, remote_yaml);
    (merged, bash_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_for_upload_removes_shell_path() {
        let sample = "model: default\nshellPath: C:\\Git\\bin\\bash.exe\ntheme: dark\n";
        let (clean, stripped) = sanitize_for_upload(sample);
        assert!(!clean.contains("shellPath"));
        assert_eq!(stripped.as_deref(), Some(r"C:\Git\bin\bash.exe"));
        assert!(clean.contains("model: default"));
        assert!(clean.contains("theme: dark"));
    }

    #[test]
    fn test_restore_after_download_injects_shell_path() {
        let remote = "model: gpt-4\ntheme: dark\n";
        let local = "shellPath: D:\\tools\\bash.exe\n";
        let (merged, injected) = restore_after_download(remote, Some(local));
        if is_windows() {
            assert!(merged.contains("shellPath"));
            assert_eq!(injected, r"D:\tools\bash.exe");
        }
    }
}
