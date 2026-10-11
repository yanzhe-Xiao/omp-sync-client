use std::fs::{self, File};
use std::io::{self, Cursor};
use std::path::Path;
use std::time::Duration;
use reqwest::blocking::multipart::{Form, Part};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDto {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub file_count: usize,
    #[serde(default)]
    pub size_kb: f64,
}

#[derive(Debug, Deserialize)]
struct FileResponse {
    #[serde(default)]
    pub content: String,
}

#[derive(Debug, Deserialize)]
struct SkillsListResponse {
    #[serde(default)]
    pub skills: Vec<SkillDto>,
}

#[derive(Debug, Clone)]
pub struct SyncApiClient {
    pub base_url: String,
    client: Client,
}

impl SyncApiClient {
    pub fn new(base_url: &str) -> Self {
        let trimmed = base_url.trim_end_matches('/').to_string();
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            base_url: trimmed,
            client,
        }
    }

    pub fn test_connection(&self) -> Result<String, String> {
        let url = format!("{}/api/status", self.base_url);
        match self.client.get(&url).send() {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    Ok(format!("连接正常 (HTTP {})", status.as_u16()))
                } else {
                    Err(format!("HTTP 错误: {}", status))
                }
            }
            Err(e) => Err(format!("无法连接至 {}: {}", self.base_url, e)),
        }
    }

    pub fn get_remote_file(&self, file_key: &str) -> Result<String, String> {
        let url = format!("{}/api/files/{}", self.base_url, file_key);
        let resp = self.client.get(&url).send().map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("获取文件失败 (HTTP {}): {}", resp.status(), resp.text().unwrap_or_default()));
        }
        let data: FileResponse = resp.json().map_err(|e| format!("解析响应失败: {e}"))?;
        Ok(data.content)
    }

    pub fn save_remote_file(&self, file_key: &str, content: &str, sync_now: bool) -> Result<(), String> {
        let url = format!("{}/api/files/{}", self.base_url, file_key);
        let payload = serde_json::json!({
            "content": content,
            "sync_now": sync_now
        });

        let resp = self.client.post(&url)
            .json(&payload)
            .send()
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            let err = resp.text().unwrap_or_default();
            return Err(format!("保存失败: {err}"));
        }

        Ok(())
    }

    pub fn get_remote_skills(&self) -> Result<Vec<SkillDto>, String> {
        let url = format!("{}/api/skills", self.base_url);
        let resp = self.client.get(&url).send().map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("获取技能列表失败 (HTTP {}): {}", resp.status(), resp.text().unwrap_or_default()));
        }
        let data: SkillsListResponse = resp.json().map_err(|e| format!("解析技能列表失败: {e}"))?;
        Ok(data.skills)
    }

    pub fn download_skill(&self, skill_name: &str, target_skills_dir: &Path) -> Result<(), String> {
        let url = format!("{}/api/skills/{}/download", self.base_url, skill_name);
        let resp = self.client.get(&url).send().map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("下载技能失败 (HTTP {}): {}", resp.status(), resp.text().unwrap_or_default()));
        }

        let bytes = resp.bytes().map_err(|e| format!("读取技能包流失败: {e}"))?;
        let cursor = Cursor::new(bytes);
        let mut archive = ZipArchive::new(cursor).map_err(|e| format!("解析技能 ZIP 失败: {e}"))?;

        let target_dir = target_skills_dir.join(skill_name);
        if target_dir.exists() {
            let _ = fs::remove_dir_all(&target_dir);
        }
        fs::create_dir_all(&target_dir).map_err(|e| format!("创建技能目录失败: {e}"))?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| format!("解压文件项失败: {e}"))?;
            let enclosed = match file.enclosed_name() {
                Some(p) => p.to_owned(),
                None => continue,
            };

            let out_path = target_dir.join(enclosed);
            if file.is_dir() {
                fs::create_dir_all(&out_path).map_err(|e| format!("创建子目录失败: {e}"))?;
            } else {
                if let Some(p) = out_path.parent() {
                    if !p.exists() {
                        fs::create_dir_all(p).map_err(|e| format!("创建父目录失败: {e}"))?;
                    }
                }
                let mut out_file = File::create(&out_path).map_err(|e| format!("写入文件失败: {e}"))?;
                io::copy(&mut file, &mut out_file).map_err(|e| format!("复制文件内容失败: {e}"))?;
            }
        }

        Ok(())
    }

    pub fn upload_skill_directory(&self, skill_name: &str, local_skill_dir: &Path, sync_now: bool) -> Result<String, String> {
        if !local_skill_dir.exists() {
            return Err(format!("本地技能目录不存在: {}", local_skill_dir.display()));
        }

        // Create zip archive in memory
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buf));
            let options = SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o755);

            for entry in WalkDir::new(local_skill_dir).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                let rel_path = match path.strip_prefix(local_skill_dir) {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let rel_path_str = rel_path.to_string_lossy().replace('\\', "/");
                if rel_path_str.is_empty() {
                    continue;
                }

                if entry.file_type().is_dir() {
                    let dir_name = if rel_path_str.ends_with('/') {
                        rel_path_str
                    } else {
                        format!("{rel_path_str}/")
                    };
                    zip.add_directory(dir_name, options).map_err(|e| e.to_string())?;
                } else if entry.file_type().is_file() {
                    zip.start_file(rel_path_str, options).map_err(|e| e.to_string())?;
                    let mut f = File::open(path).map_err(|e| e.to_string())?;
                    io::copy(&mut f, &mut zip).map_err(|e| e.to_string())?;
                }
            }
            zip.finish().map_err(|e| format!("构建 ZIP 压缩包失败: {e}"))?;
        }

        let part = Part::bytes(buf)
            .file_name(format!("{skill_name}.zip"))
            .mime_str("application/zip")
            .map_err(|e| e.to_string())?;

        let form = Form::new()
            .part("file", part)
            .text("sync_now", if sync_now { "true" } else { "false" });

        let url = format!("{}/api/skills/upload", self.base_url);
        let resp = self.client.post(&url)
            .multipart(form)
            .send()
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err = resp.text().unwrap_or_default();
            return Err(format!("上传失败 (HTTP {status}): {err}"));
        }

        Ok(skill_name.to_string())
    }
}
