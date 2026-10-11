use std::collections::{BTreeSet, HashMap};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;
use walkdir::WalkDir;

use super::api::SkillDto;

#[derive(Debug, Clone)]
pub struct SkillItem {
    pub name: String,
    pub description: String,
    pub is_local: bool,
    pub is_remote: bool,
    pub local_file_count: usize,
    pub remote_file_count: usize,
    pub is_selected: bool,
}

impl SkillItem {
    pub fn local_status_text(&self) -> String {
        if self.is_local {
            format!("✅ 已安装 ({} 文件)", self.local_file_count)
        } else {
            "❌ 未安装".to_string()
        }
    }

    pub fn remote_status_text(&self) -> String {
        if self.is_remote {
            format!("☁️ 已就绪 ({} 文件)", self.remote_file_count)
        } else {
            "❌ 未上传".to_string()
        }
    }

    pub fn can_upload(&self) -> bool {
        self.is_local
    }

    pub fn can_download(&self) -> bool {
        self.is_remote
    }
}

pub fn scan_local_skills(skills_dir: &Path) -> HashMap<String, (usize, String)> {
    let mut map = HashMap::new();
    if !skills_dir.exists() {
        return map;
    }

    let entries = match fs::read_dir(skills_dir) {
        Ok(e) => e,
        Err(_) => return map,
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = match entry.file_name().into_string() {
            Ok(n) => n,
            Err(_) => continue,
        };

        let mut file_count = 0;
        for file_entry in WalkDir::new(&path).into_iter().filter_map(|e| e.ok()) {
            if file_entry.file_type().is_file() {
                file_count += 1;
            }
        }

        let mut desc = String::new();
        let skill_md = path.join("SKILL.md");
        if skill_md.exists() {
            if let Ok(file) = File::open(&skill_md) {
                let reader = BufReader::new(file);
                for line_res in reader.lines().take(15) {
                    if let Ok(line) = line_res {
                        let trimmed = line.trim();
                        if trimmed.to_lowercase().starts_with("description:") {
                            desc = trimmed["description:".len()..].trim().to_string();
                            break;
                        } else if trimmed.starts_with("# ") && desc.is_empty() {
                            desc = trimmed[2..].trim().to_string();
                        }
                    }
                }
            }
        }

        map.insert(name, (file_count, desc));
    }

    map
}

pub fn merge_skills(
    local_skills: &HashMap<String, (usize, String)>,
    remote_skills: &[SkillDto],
) -> Vec<SkillItem> {
    let mut remote_map = HashMap::new();
    for r in remote_skills {
        remote_map.insert(r.name.clone(), r.clone());
    }

    let mut all_names = BTreeSet::new();
    for k in local_skills.keys() {
        all_names.insert(k.clone());
    }
    for k in remote_map.keys() {
        all_names.insert(k.clone());
    }

    let mut list = Vec::new();
    for name in all_names {
        let local_info = local_skills.get(&name);
        let remote_info = remote_map.get(&name);

        let is_local = local_info.is_some();
        let is_remote = remote_info.is_some();

        let local_count = local_info.map(|(cnt, _)| *cnt).unwrap_or(0);
        let remote_count = remote_info.map(|r| r.file_count).unwrap_or(0);

        let desc = local_info
            .and_then(|(_, d)| if !d.is_empty() { Some(d.clone()) } else { None })
            .or_else(|| remote_info.map(|r| if !r.description.is_empty() { r.description.clone() } else { "无说明".to_string() }))
            .unwrap_or_else(|| "无说明".to_string());

        list.push(SkillItem {
            name,
            description: desc,
            is_local,
            is_remote,
            local_file_count: local_count,
            remote_file_count: remote_count,
            is_selected: false,
        });
    }

    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_skills_union() {
        let mut local = HashMap::new();
        local.insert("local-only".to_string(), (3, "Local description".to_string()));
        local.insert("shared-skill".to_string(), (5, "Local shared desc".to_string()));

        let remote = vec![
            SkillDto {
                name: "shared-skill".to_string(),
                description: "Remote shared desc".to_string(),
                file_count: 5,
                size_kb: 12.0,
            },
            SkillDto {
                name: "remote-only".to_string(),
                description: "Remote description".to_string(),
                file_count: 2,
                size_kb: 4.5,
            },
        ];

        let merged = merge_skills(&local, &remote);
        assert_eq!(merged.len(), 3);

        let local_only = merged.iter().find(|s| s.name == "local-only").unwrap();
        assert!(local_only.is_local);
        assert!(!local_only.is_remote);
        assert_eq!(local_only.local_file_count, 3);

        let remote_only = merged.iter().find(|s| s.name == "remote-only").unwrap();
        assert!(!remote_only.is_local);
        assert!(remote_only.is_remote);
        assert_eq!(remote_only.remote_file_count, 2);

        let shared = merged.iter().find(|s| s.name == "shared-skill").unwrap();
        assert!(shared.is_local);
        assert!(shared.is_remote);
    }
}
