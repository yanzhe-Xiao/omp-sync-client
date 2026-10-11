use std::fs;
use crate::core::api::SyncApiClient;
use crate::core::paths::{get_agent_dir, get_skills_dir};
use crate::core::shell_path::{restore_after_download, sanitize_for_upload};
use crate::core::skills::{merge_skills, scan_local_skills};

const DEFAULT_SERVER_URL: &str = "http://10.69.211.84:8900";

pub fn run_cli(args: &[String]) -> i32 {
    let cmd = args.get(0).map(|s| s.to_lowercase()).unwrap_or_default();
    let server_url = args.get(1).map(|s| s.as_str()).unwrap_or(DEFAULT_SERVER_URL);

    let client = SyncApiClient::new(server_url);
    let agent_dir = get_agent_dir();
    let config_path = agent_dir.join("config.yml");
    let models_path = agent_dir.join("models.yml");

    println!("\n[OMP Sync Client CLI (Rust)] Server: {}", client.base_url);

    match cmd.as_str() {
        "test" | "status" => {
            match &client.test_connection() {
                Ok(msg) => {
                    println!("Connection Test: SUCCESS ({msg})");
                    0
                }
                Err(err) => {
                    eprintln!("Connection Test: FAILED ({err})");
                    1
                }
            }
        }
        "pull" => {
            println!("Pulling config from server...");
            match &client.get_remote_file("config.yml") {
                Ok(remote_config) => {
                    let local_content = if config_path.exists() {
                        fs::read_to_string(&config_path).unwrap_or_default()
                    } else {
                        String::new()
                    };

                    let (merged, injected) = restore_after_download(remote_config, Some(&local_content));

                    if config_path.exists() {
                        let bak_path = agent_dir.join("config.yml.bak");
                        let _ = fs::copy(&config_path, &bak_path);
                    }

                    let _ = fs::create_dir_all(&agent_dir);
                    if let Err(e) = fs::write(&config_path, merged) {
                        eprintln!("[ERROR] Write config.yml failed: {e}");
                        return 1;
                    }
                    println!("[SUCCESS] config.yml pulled! Injected/preserved shellPath: {injected}");

                    if let Ok(remote_models) = client.get_remote_file("models.yml") {
                        if !remote_models.trim().is_empty() {
                            if let Err(e) = fs::write(&models_path, remote_models) {
                                eprintln!("[WARNING] Write models.yml failed: {e}");
                            } else {
                                println!("[SUCCESS] models.yml pulled!");
                            }
                        }
                    }

                    0
                }
                Err(err) => {
                    eprintln!("[ERROR] Pull failed: {err}");
                    1
                }
            }
        }
        "push" => {
            println!("Pushing config to server...");
            if !config_path.exists() {
                eprintln!("[ERROR] Local config.yml not found at {}", config_path.display());
                return 1;
            }

            let local_yaml = match fs::read_to_string(&config_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[ERROR] Failed to read local config.yml: {e}");
                    return 1;
                }
            };

            let (clean_yaml, stripped) = sanitize_for_upload(&local_yaml);
            if let Some(stripped_path) = &stripped {
                println!("[SHIELD] Stripped Windows shellPath '{stripped_path}' to protect Linux server.");
            }

            if let Err(e) = client.save_remote_file("config.yml", &clean_yaml, true) {
                eprintln!("[ERROR] Push config.yml failed: {e}");
                return 1;
            }
            println!("[SUCCESS] config.yml pushed and broadcasted to cluster!");

            if models_path.exists() {
                if let Ok(models_content) = fs::read_to_string(&models_path) {
                    if let Err(e) = client.save_remote_file("models.yml", &models_content, true) {
                        eprintln!("[WARNING] Push models.yml failed: {e}");
                    } else {
                        println!("[SUCCESS] models.yml pushed!");
                    }
                }
            }

            0
        }
        "diff" => {
            println!("Comparing local and remote configurations...");
            let local_cfg = fs::read_to_string(&config_path).unwrap_or_default();
            let (clean_local, stripped) = sanitize_for_upload(&local_cfg);
            println!("[Shield Status] Local Windows shellPath = {}", stripped.unwrap_or_else(|| "(None)".to_string()));

            match &client.get_remote_file("config.yml") {
                Ok(remote_cfg) => {
                    if clean_local.trim() == remote_cfg.trim() {
                        println!("🎉 [config.yml] Local core settings and remote are IDENTICAL! (Ignored platform shellPath)");
                    } else {
                        println!("⚠️  [config.yml] Differences detected between local and remote!");
                    }
                }
                Err(e) => eprintln!("Failed to fetch remote config.yml: {e}"),
            }

            if models_path.exists() {
                let local_models = fs::read_to_string(&models_path).unwrap_or_default();
                match &client.get_remote_file("models.yml") {
                    Ok(remote_models) => {
                        if local_models.trim() == remote_models.trim() {
                            println!("🎉 [models.yml] Local and remote are IDENTICAL!");
                        } else {
                            println!("⚠️  [models.yml] Differences detected between local and remote models.yml!");
                        }
                    }
                    Err(e) => eprintln!("Failed to fetch remote models.yml: {e}"),
                }
            }

            0
        }
        "skills" => {
            let sub_cmd = args.get(1).map(|s| s.as_str()).unwrap_or("list");
            let skills_dir = get_skills_dir();

            match sub_cmd {
                "list" => {
                    let local_map = scan_local_skills(&skills_dir);
                    let remote_list = client.get_remote_skills().unwrap_or_default();
                    let merged = merge_skills(&local_map, &remote_list);

                    println!("\nTotal Skills: {}", merged.len());
                    println!("{:<25} {:<15} {:<15} {}", "SKILL NAME", "LOCAL", "REMOTE", "DESCRIPTION");
                    println!("{}", "-".repeat(80));
                    for item in &merged {
                        println!(
                            "{:<25} {:<15} {:<15} {}",
                            item.name,
                            if item.is_local { format!("{} files", item.local_file_count) } else { "Not Installed".to_string() },
                            if item.is_remote { format!("{} files", item.remote_file_count) } else { "Not Uploaded".to_string() },
                            item.description
                        );
                    }
                    0
                }
                "pull" => {
                    let skill_name = match args.get(2) {
                        Some(name) => name,
                        None => {
                            eprintln!("Usage: omp-sync-client skills pull <skill_name> [server_url]");
                            return 1;
                        }
                    };
                    println!("Downloading skill '{skill_name}'...");
                    if let Err(e) = client.download_skill(skill_name, &skills_dir) {
                        eprintln!("[ERROR] Download skill failed: {e}");
                        return 1;
                    }
                    println!("[SUCCESS] Skill '{skill_name}' downloaded and unpacked to {}", skills_dir.join(skill_name).display());
                    0
                }
                "push" => {
                    let skill_name = match args.get(2) {
                        Some(name) => name,
                        None => {
                            eprintln!("Usage: omp-sync-client skills push <skill_name> [server_url]");
                            return 1;
                        }
                    };
                    let local_skill_path = skills_dir.join(skill_name);
                    println!("Uploading skill '{skill_name}' from {}...", local_skill_path.display());
                    if let Err(e) = client.upload_skill_directory(skill_name, &local_skill_path, true) {
                        eprintln!("[ERROR] Upload skill failed: {e}");
                        return 1;
                    }
                    println!("[SUCCESS] Skill '{skill_name}' uploaded and broadcasted to cluster!");
                    0
                }
                _ => {
                    eprintln!("Unknown skills sub-command: {sub_cmd}. Use: list, pull, push");
                    1
                }
            }
        }
        "help" | "-h" | "--help" => {
            print_usage();
            0
        }
        _ => {
            eprintln!("Unknown command: '{cmd}'");
            print_usage();
            1
        }
    }
}

fn print_usage() {
    println!("\nOMP Sync Client (Rust)");
    println!("Usage:");
    println!("  omp-sync-client                   # Launch GUI desktop interface");
    println!("  omp-sync-client pull [server_url] # Pull config & models from server (auto-inject bash)");
    println!("  omp-sync-client push [server_url] # Push config & models to server (auto-strip bash)");
    println!("  omp-sync-client test [server_url] # Test connection to master server");
    println!("  omp-sync-client diff [server_url] # Compare local and remote configurations");
    println!("  omp-sync-client skills list [server_url]");
    println!("  omp-sync-client skills pull <skill_name> [server_url]");
    println!("  omp-sync-client skills push <skill_name> [server_url]");
}
