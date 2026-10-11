use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use chrono::Local;
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};

use crate::core::api::{SkillDto, SyncApiClient};
use crate::core::paths::{get_agent_dir, get_skills_dir};
use crate::core::shell_path::{detect_local_bash_path, is_windows, restore_after_download, sanitize_for_upload};
use crate::core::skills::{merge_skills, scan_local_skills, SkillItem};

const BG_COLOR: Color32 = Color32::from_rgb(15, 17, 23);
const CARD_BG: Color32 = Color32::from_rgb(22, 27, 34);
const CARD_BORDER: Color32 = Color32::from_rgb(48, 54, 61);
const CODE_BG: Color32 = Color32::from_rgb(13, 17, 23);
const TEXT_COLOR: Color32 = Color32::from_rgb(230, 237, 243);
const MUTED_COLOR: Color32 = Color32::from_rgb(139, 148, 158);
const BLUE_ACCENT: Color32 = Color32::from_rgb(88, 166, 255);
const GREEN_ACCENT: Color32 = Color32::from_rgb(63, 185, 80);
const RED_ACCENT: Color32 = Color32::from_rgb(248, 81, 73);
const BTN_BG: Color32 = Color32::from_rgb(33, 38, 45);
const BTN_PRIMARY: Color32 = Color32::from_rgb(31, 111, 235);
const BTN_SUCCESS: Color32 = Color32::from_rgb(35, 134, 54);

#[derive(PartialEq)]
enum ActiveTab {
    ConfigSync,
    SkillsSync,
}

#[derive(PartialEq, Clone)]
enum ActiveConfigFile {
    ConfigYml,
    ModelsYml,
}

enum BgTask {
    TestConnection { server_url: String },
    RefreshRemoteFile { server_url: String, file_key: String },
    PullFile { server_url: String, file_key: String, show_msg: bool },
    PushFile { server_url: String, file_key: String, content: String, show_msg: bool },
    PullBoth { server_url: String },
    PushBoth { server_url: String, local_config: String, local_models: Option<String> },
    RefreshSkills { server_url: String, skills_dir: PathBuf },
    UploadSkill { server_url: String, name: String, local_skill_dir: PathBuf, silent: bool },
    DownloadSkill { server_url: String, name: String, target_skills_dir: PathBuf, silent: bool },
    UploadSelectedSkills { server_url: String, skills_dir: PathBuf, names: Vec<String> },
    DownloadSelectedSkills { server_url: String, skills_dir: PathBuf, names: Vec<String> },
}

enum BgResult {
    Log(String),
    ConnectionTested { success: bool, message: String },
    RemoteFileLoaded { file_key: String, content: String },
    SkillsLoaded { local: HashMap<String, (usize, String)>, remote: Vec<SkillDto> },
    TaskCompleted { message: String, is_error: bool, refresh_after: bool, refresh_skills_after: bool },
}

pub struct OmpSyncApp {
    server_url: String,
    conn_status_text: String,
    conn_status_color: Color32,
    detected_bash: String,
    active_tab: ActiveTab,
    active_file: ActiveConfigFile,
    local_config_text: String,
    remote_config_text: String,
    skills_list: Vec<SkillItem>,
    logs: Vec<String>,
    task_tx: Sender<BgTask>,
    result_rx: Receiver<BgResult>,
    status_dialog: Option<(String, String, bool)>, // (title, message, is_error)
}

impl OmpSyncApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_custom_fonts(&cc.egui_ctx);
        apply_dark_theme(&cc.egui_ctx);

        let (task_tx, task_rx) = channel::<BgTask>();
        let (result_tx, result_rx) = channel::<BgResult>();

        // Spawn background worker thread
        let res_sender = result_tx.clone();
        thread::spawn(move || {
            while let Ok(task) = task_rx.recv() {
                handle_bg_task(task, &res_sender);
            }
        });

        let default_url = "http://10.69.211.84:8900".to_string();
        let agent_dir = get_agent_dir();
        let local_cfg_path = agent_dir.join("config.yml");
        let local_cfg_content = if local_cfg_path.exists() {
            fs::read_to_string(&local_cfg_path).unwrap_or_default()
        } else {
            String::new()
        };

        let bash_path = if is_windows() {
            detect_local_bash_path(&local_cfg_content).unwrap_or_else(|| "未在默认路径找到 Git Bash，将尝试使用环境备用路径".to_string())
        } else {
            "Linux 原生系统环境 (/bin/bash)".to_string()
        };

        let mut app = Self {
            server_url: default_url.clone(),
            conn_status_text: "● 未测试".to_string(),
            conn_status_color: MUTED_COLOR,
            detected_bash: bash_path,
            active_tab: ActiveTab::ConfigSync,
            active_file: ActiveConfigFile::ConfigYml,
            local_config_text: String::new(),
            remote_config_text: String::new(),
            skills_list: Vec::new(),
            logs: Vec::new(),
            task_tx,
            result_rx,
            status_dialog: None,
        };

        app.log(&format!("客户端启动成功。本地目录: {}", agent_dir.display()));
        app.load_local_file();

        // Initial background sync
        let _ = app.task_tx.send(BgTask::TestConnection { server_url: default_url.clone() });
        let _ = app.task_tx.send(BgTask::RefreshRemoteFile {
            server_url: default_url.clone(),
            file_key: "config.yml".to_string(),
        });
        let _ = app.task_tx.send(BgTask::RefreshSkills {
            server_url: default_url,
            skills_dir: get_skills_dir(),
        });

        app
    }

    fn log(&mut self, msg: &str) {
        let time = Local::now().format("%H:%M:%S");
        self.logs.push(format!("[{time}] {msg}"));
    }

    fn active_file_key(&self) -> &'static str {
        match self.active_file {
            ActiveConfigFile::ConfigYml => "config.yml",
            ActiveConfigFile::ModelsYml => "models.yml",
        }
    }

    fn load_local_file(&mut self) {
        let key = self.active_file_key();
        let path = get_agent_dir().join(key);
        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(c) => {
                    self.local_config_text = c;
                    self.log(&format!("已载入本地 {key} 文件。"));
                }
                Err(e) => {
                    self.local_config_text = format!("# 读取失败: {e}");
                    self.log(&format!("读取本地 {key} 失败: {e}"));
                }
            }
        } else {
            self.local_config_text = format!("# 本地未检测到 {key}");
            self.log(&format!("本地未找到 {key} ({})", path.display()));
        }
    }

    fn process_bg_results(&mut self) {
        while let Ok(res) = self.result_rx.try_recv() {
            match &res {
                BgResult::Log(msg) => {
                    self.log(msg);
                }
                BgResult::ConnectionTested { success, message } => {
                    if *success {
                        self.conn_status_text = "● 在线 (Connected)".to_string();
                        self.conn_status_color = GREEN_ACCENT;
                        self.log(&format!("云端服务器连接成功: {}", self.server_url));
                    } else {
                        self.conn_status_text = "● 离线 (Disconnected)".to_string();
                        self.conn_status_color = RED_ACCENT;
                        self.log(&format!("云端服务器连接失败: {message}"));
                    }
                }
                BgResult::RemoteFileLoaded { file_key, content } => {
                    if file_key == self.active_file_key() {
                        self.remote_config_text = content.clone();
                    }
                    self.log(&format!("成功获取云端最新 {file_key}。"));
                }
                BgResult::SkillsLoaded { local, remote } => {
                    self.skills_list = merge_skills(local, remote);
                    self.log(&format!("Skills 列表更新完毕：共 {} 个技能。", self.skills_list.len()));
                }
                BgResult::TaskCompleted { message, is_error, refresh_after, refresh_skills_after } => {
                    if *is_error {
                        self.log(&format!("❌ {message}"));
                        self.status_dialog = Some(("错误".to_string(), message.clone(), true));
                    } else {
                        self.log(&format!("✅ {message}"));
                        self.status_dialog = Some(("完成".to_string(), message.clone(), false));
                    }
                    if *refresh_after {
                        self.load_local_file();
                        let _ = self.task_tx.send(BgTask::RefreshRemoteFile {
                            server_url: self.server_url.clone(),
                            file_key: self.active_file_key().to_string(),
                        });
                    }
                    if *refresh_skills_after {
                        let _ = self.task_tx.send(BgTask::RefreshSkills {
                            server_url: self.server_url.clone(),
                            skills_dir: get_skills_dir(),
                        });
                    }
                }
            }
        }
    }
}

impl eframe::App for OmpSyncApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_dark_theme(ctx);
        self.process_bg_results();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);

            // 1. TOP SERVER BAR
            egui::Frame::none()
                .fill(CARD_BG)
                .stroke(Stroke::new(1.0, CARD_BORDER))
                .rounding(6.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Master 云端服务器:").strong().color(TEXT_COLOR));
                        egui::Frame::none()
                            .fill(CODE_BG)
                            .stroke(Stroke::new(1.0, CARD_BORDER))
                            .rounding(4.0)
                            .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                            .show(ui, |ui| {
                                let edit = ui.add(egui::TextEdit::singleline(&mut self.server_url)
                                    .frame(false)
                                    .text_color(TEXT_COLOR)
                                    .desired_width(320.0));
                                if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                    let _ = self.task_tx.send(BgTask::TestConnection {
                                        server_url: self.server_url.clone(),
                                    });
                                }
                            });

                        if ui.add(egui::Button::new(RichText::new("测试连接").strong().color(TEXT_COLOR)).fill(BTN_BG).stroke(Stroke::new(1.0, CARD_BORDER))).clicked() {
                            self.log(&format!("正在测试与 {} 的连接...", self.server_url));
                            let _ = self.task_tx.send(BgTask::TestConnection {
                                server_url: self.server_url.clone(),
                            });
                            let _ = self.task_tx.send(BgTask::RefreshSkills {
                                server_url: self.server_url.clone(),
                                skills_dir: get_skills_dir(),
                            });
                        }

                        ui.add_space(8.0);
                        ui.label(RichText::new(&self.conn_status_text).color(self.conn_status_color).strong());
                    });
                });

            // 2. SHELL PATH SHIELD CARD
            egui::Frame::none()
                .fill(CARD_BG)
                .stroke(Stroke::new(1.0, Color32::from_rgb(56, 139, 253)))
                .rounding(6.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("🛡️ 平台路径隔离保护:").strong().color(BLUE_ACCENT));
                        ui.label(RichText::new(&self.detected_bash).color(Color32::from_rgb(126, 231, 135)).monospace().strong());
                        ui.label(RichText::new("(上传自动剔除，拉取自动注入)").size(11.0).color(MUTED_COLOR));
                    });
                });

            // 3. MAIN TABS
            ui.horizontal(|ui| {
                let tab_btn = |ui: &mut egui::Ui, is_active: bool, title: &str| -> egui::Response {
                    let (bg, border, text_color) = if is_active {
                        (Color32::from_rgb(28, 33, 40), Color32::from_rgb(56, 139, 253), BLUE_ACCENT)
                    } else {
                        (CARD_BG, CARD_BORDER, MUTED_COLOR)
                    };
                    egui::Frame::none()
                        .fill(bg)
                        .stroke(Stroke::new(1.0, border))
                        .rounding(egui::Rounding { nw: 6.0, ne: 6.0, sw: 0.0, se: 0.0 })
                        .inner_margin(egui::Margin::symmetric(14.0, 7.0))
                        .show(ui, |ui| {
                            ui.label(RichText::new(title).strong().size(13.5).color(text_color))
                        }).response
                };

                if tab_btn(ui, self.active_tab == ActiveTab::ConfigSync, "📝 配置文件同步 (config.yml / models.yml)").interact(egui::Sense::click()).clicked() {
                    self.active_tab = ActiveTab::ConfigSync;
                }
                if tab_btn(ui, self.active_tab == ActiveTab::SkillsSync, "🧩 技能选择性同步 (Skills 管理)").interact(egui::Sense::click()).clicked() {
                    self.active_tab = ActiveTab::SkillsSync;
                }
            });

            // TAB CONTENT
            let available_height = ui.available_height() - 170.0;
            match self.active_tab {
                ActiveTab::ConfigSync => {
                    self.render_config_tab(ui, available_height);
                }
                ActiveTab::SkillsSync => {
                    self.render_skills_tab(ui, available_height);
                }
            }

            // 4. BOTTOM REAL-TIME LOG CONSOLE
            egui::Frame::none()
                .fill(CARD_BG)
                .stroke(Stroke::new(1.0, CARD_BORDER))
                .rounding(6.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("📜 实时操作日志").strong().color(MUTED_COLOR));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new(RichText::new("清空日志").color(TEXT_COLOR).size(11.0)).fill(BTN_BG).stroke(Stroke::new(1.0, CARD_BORDER))).clicked() {
                                self.logs.clear();
                            }
                        });
                    });

                    ui.add_space(4.0);
                    egui::Frame::none()
                        .fill(CODE_BG)
                        .stroke(Stroke::new(1.0, CARD_BORDER))
                        .rounding(4.0)
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            egui::ScrollArea::vertical()
                                .max_height(110.0)
                                .stick_to_bottom(true)
                                .show(ui, |ui| {
                                    for line in &self.logs {
                                        ui.label(RichText::new(line).monospace().size(12.0).color(Color32::from_rgb(201, 209, 217)));
                                    }
                                });
                        });
                });
        });

        // POPUP DIALOG
        if let Some((title, msg, is_err)) = &self.status_dialog.clone() {
            egui::Window::new(title)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.set_min_width(320.0);
                    let color = if *is_err { RED_ACCENT } else { GREEN_ACCENT };
                    ui.label(RichText::new(msg).color(color));
                    ui.add_space(10.0);
                    if ui.button("确定").clicked() {
                        self.status_dialog = None;
                    }
                });
        }
    }
}

impl OmpSyncApp {
    fn render_config_tab(&mut self, ui: &mut egui::Ui, height: f32) {
        // Toolbar
        egui::Frame::none()
            .fill(CARD_BG)
            .stroke(Stroke::new(1.0, CARD_BORDER))
            .rounding(6.0)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("当前编辑文件:").color(MUTED_COLOR).strong());
                    let old_file = self.active_file.clone();
                    ui.radio_value(&mut self.active_file, ActiveConfigFile::ConfigYml, RichText::new("config.yml (模型角色/主题/思考级别)").color(TEXT_COLOR).strong());
                    ui.radio_value(&mut self.active_file, ActiveConfigFile::ModelsYml, RichText::new("models.yml (自定义 API Key 与模型)").color(TEXT_COLOR).strong());

                    if old_file != self.active_file {
                        self.load_local_file();
                        let _ = self.task_tx.send(BgTask::RefreshRemoteFile {
                            server_url: self.server_url.clone(),
                            file_key: self.active_file_key().to_string(),
                        });
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(RichText::new("🔍 比对差异").color(TEXT_COLOR).strong()).fill(BTN_BG).stroke(Stroke::new(1.0, CARD_BORDER))).clicked() {
                            self.run_diff();
                        }
                        if ui.add(egui::Button::new(RichText::new("📤 推送两者 (Both)").color(TEXT_COLOR).strong()).fill(BTN_BG).stroke(Stroke::new(1.0, CARD_BORDER))).clicked() {
                            self.log("==================================================");
                            self.log("开始一键推送全部配置至云端 (config.yml + models.yml)...");
                            let agent_dir = get_agent_dir();
                            let models_content = fs::read_to_string(agent_dir.join("models.yml")).ok();
                            let config_content = match fs::read_to_string(agent_dir.join("config.yml")) {
                                Ok(c) => c,
                                Err(e) => {
                                    self.log(&format!("读取本地 config.yml 失败: {e}"));
                                    return;
                                }
                            };
                            let _ = self.task_tx.send(BgTask::PushBoth {
                                server_url: self.server_url.clone(),
                                local_config: config_content,
                                local_models: models_content,
                            });
                        }
                        if ui.add(egui::Button::new(RichText::new("📥 拉取两者 (Both)").color(TEXT_COLOR).strong()).fill(BTN_BG).stroke(Stroke::new(1.0, CARD_BORDER))).clicked() {
                            self.log("==================================================");
                            self.log("开始一键从云端拉取全部配置 (config.yml + models.yml)...");
                            let _ = self.task_tx.send(BgTask::PullBoth {
                                server_url: self.server_url.clone(),
                            });
                        }
                        if ui.add(egui::Button::new(RichText::new("📤 推送当前文件").color(Color32::WHITE).strong()).fill(BTN_SUCCESS)).clicked() {
                            let key = self.active_file_key().to_string();
                            let content = self.local_config_text.clone();
                            let _ = self.task_tx.send(BgTask::PushFile {
                                server_url: self.server_url.clone(),
                                file_key: key,
                                content,
                                show_msg: true,
                            });
                        }
                        if ui.add(egui::Button::new(RichText::new("📥 拉取当前文件").color(Color32::WHITE).strong()).fill(BTN_PRIMARY)).clicked() {
                            let key = self.active_file_key().to_string();
                            let _ = self.task_tx.send(BgTask::PullFile {
                                server_url: self.server_url.clone(),
                                file_key: key,
                                show_msg: true,
                            });
                        }
                    });
                });
            });

        // Split View
        ui.horizontal(|ui| {
            let col_width = (ui.available_width() - 10.0) / 2.0;

            // Local Column
            ui.allocate_ui_with_layout(Vec2::new(col_width, height), egui::Layout::top_down(egui::Align::Min), |ui| {
                egui::Frame::none()
                    .fill(CARD_BG)
                    .stroke(Stroke::new(1.0, CARD_BORDER))
                    .rounding(6.0)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("💻 本机配置 ({})", self.active_file_key())).strong().color(BLUE_ACCENT));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.add(egui::Button::new(RichText::new("刷新本地").color(TEXT_COLOR).size(11.0)).fill(BTN_BG).stroke(Stroke::new(1.0, CARD_BORDER))).clicked() {
                                    self.load_local_file();
                                }
                            });
                        });
                        ui.add_space(4.0);
                        egui::Frame::none()
                            .fill(CODE_BG)
                            .stroke(Stroke::new(1.0, CARD_BORDER))
                            .rounding(4.0)
                            .inner_margin(8.0)
                            .show(ui, |ui| {
                                egui::ScrollArea::vertical().max_height(height - 50.0).show(ui, |ui| {
                                    ui.add(egui::TextEdit::multiline(&mut self.local_config_text)
                                        .frame(false)
                                        .text_color(TEXT_COLOR)
                                        .font(egui::TextStyle::Monospace)
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(16));
                                });
                            });
                    });
            });

            // Remote Column
            ui.allocate_ui_with_layout(Vec2::new(col_width, height), egui::Layout::top_down(egui::Align::Min), |ui| {
                egui::Frame::none()
                    .fill(CARD_BG)
                    .stroke(Stroke::new(1.0, CARD_BORDER))
                    .rounding(6.0)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("☁️ 云端 Master 配置 ({})", self.active_file_key())).strong().color(GREEN_ACCENT));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.add(egui::Button::new(RichText::new("刷新云端").color(TEXT_COLOR).size(11.0)).fill(BTN_BG).stroke(Stroke::new(1.0, CARD_BORDER))).clicked() {
                                    let key = self.active_file_key().to_string();
                                    let _ = self.task_tx.send(BgTask::RefreshRemoteFile {
                                        server_url: self.server_url.clone(),
                                        file_key: key,
                                    });
                                }
                            });
                        });
                        ui.add_space(4.0);
                        egui::Frame::none()
                            .fill(CODE_BG)
                            .stroke(Stroke::new(1.0, CARD_BORDER))
                            .rounding(4.0)
                            .inner_margin(8.0)
                            .show(ui, |ui| {
                                egui::ScrollArea::vertical().max_height(height - 50.0).show(ui, |ui| {
                                    ui.add(egui::TextEdit::multiline(&mut self.remote_config_text)
                                        .frame(false)
                                        .text_color(TEXT_COLOR)
                                        .font(egui::TextStyle::Monospace)
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(16));
                                });
                            });
                    });
            });
        });
    }

    fn render_skills_tab(&mut self, ui: &mut egui::Ui, height: f32) {
        // Top Toolbar
        egui::Frame::none()
            .fill(CARD_BG)
            .stroke(Stroke::new(1.0, CARD_BORDER))
            .rounding(6.0)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new("🔄 刷新列表").color(TEXT_COLOR)).fill(BTN_BG).stroke(Stroke::new(1.0, CARD_BORDER))).clicked() {
                        let _ = self.task_tx.send(BgTask::RefreshSkills {
                            server_url: self.server_url.clone(),
                            skills_dir: get_skills_dir(),
                        });
                    }
                    if ui.add(egui::Button::new(RichText::new("☑️ 全选").color(TEXT_COLOR)).fill(BTN_BG).stroke(Stroke::new(1.0, CARD_BORDER))).clicked() {
                        for item in &mut self.skills_list {
                            item.is_selected = true;
                        }
                    }
                    if ui.add(egui::Button::new(RichText::new("⬜ 全不选").color(TEXT_COLOR)).fill(BTN_BG).stroke(Stroke::new(1.0, CARD_BORDER))).clicked() {
                        for item in &mut self.skills_list {
                            item.is_selected = false;
                        }
                    }

                    let local_count = self.skills_list.iter().filter(|s| s.is_local).count();
                    let remote_count = self.skills_list.iter().filter(|s| s.is_remote).count();
                    ui.label(RichText::new(format!("共检测到 {} 个技能 (本地 {} 个, 云端 {} 个)", self.skills_list.len(), local_count, remote_count)).color(MUTED_COLOR));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(RichText::new("📥 从云端下载所选技能").color(Color32::WHITE).strong()).fill(BTN_PRIMARY)).clicked() {
                            let selected: Vec<String> = self.skills_list.iter().filter(|s| s.is_selected && s.is_remote).map(|s| s.name.clone()).collect();
                            if selected.is_empty() {
                                self.status_dialog = Some(("提示".to_string(), "请勾选至少一个云端存在的 Skill 进行下载。".to_string(), false));
                            } else {
                                let _ = self.task_tx.send(BgTask::DownloadSelectedSkills {
                                    server_url: self.server_url.clone(),
                                    skills_dir: get_skills_dir(),
                                    names: selected,
                                });
                            }
                        }
                        if ui.add(egui::Button::new(RichText::new("📤 上传所选技能到云端").color(Color32::WHITE).strong()).fill(BTN_SUCCESS)).clicked() {
                            let selected: Vec<String> = self.skills_list.iter().filter(|s| s.is_selected && s.is_local).map(|s| s.name.clone()).collect();
                            if selected.is_empty() {
                                self.status_dialog = Some(("提示".to_string(), "请勾选至少一个本地存在的 Skill 进行上传。".to_string(), false));
                            } else {
                                let _ = self.task_tx.send(BgTask::UploadSelectedSkills {
                                    server_url: self.server_url.clone(),
                                    skills_dir: get_skills_dir(),
                                    names: selected,
                                });
                            }
                        }
                    });
                });
            });

        // Skills Table
        egui::Frame::none()
            .fill(CARD_BG)
            .stroke(Stroke::new(1.0, CARD_BORDER))
            .rounding(6.0)
            .inner_margin(6.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(height).show(ui, |ui| {
                    // Header
                    ui.horizontal(|ui| {
                        ui.allocate_ui(Vec2::new(30.0, 20.0), |ui| { ui.label(""); });
                        ui.allocate_ui(Vec2::new(180.0, 20.0), |ui| { ui.label(RichText::new("Skill 名称").strong()); });
                        ui.allocate_ui(Vec2::new(260.0, 20.0), |ui| { ui.label(RichText::new("功能描述").strong()); });
                        ui.allocate_ui(Vec2::new(140.0, 20.0), |ui| { ui.label(RichText::new("本地状态").strong()); });
                        ui.allocate_ui(Vec2::new(140.0, 20.0), |ui| { ui.label(RichText::new("云端状态").strong()); });
                        ui.allocate_ui(Vec2::new(140.0, 20.0), |ui| { ui.label(RichText::new("单个操作").strong()); });
                    });
                    ui.separator();

                    let mut upload_target = None;
                    let mut download_target = None;

                    for item in &mut self.skills_list {
                        ui.horizontal(|ui| {
                            ui.allocate_ui(Vec2::new(30.0, 24.0), |ui| {
                                ui.checkbox(&mut item.is_selected, "");
                            });
                            ui.allocate_ui(Vec2::new(180.0, 24.0), |ui| {
                                ui.label(RichText::new(&item.name).strong());
                            });
                            ui.allocate_ui(Vec2::new(260.0, 24.0), |ui| {
                                ui.label(RichText::new(&item.description).color(MUTED_COLOR));
                            });
                            ui.allocate_ui(Vec2::new(140.0, 24.0), |ui| {
                                let color = if item.is_local { GREEN_ACCENT } else { MUTED_COLOR };
                                ui.label(RichText::new(item.local_status_text()).color(color).strong());
                            });
                            ui.allocate_ui(Vec2::new(140.0, 24.0), |ui| {
                                let color = if item.is_remote { BLUE_ACCENT } else { MUTED_COLOR };
                                ui.label(RichText::new(item.remote_status_text()).color(color).strong());
                            });
                            ui.allocate_ui(Vec2::new(140.0, 24.0), |ui| {
                                ui.horizontal(|ui| {
                                    if ui.add_enabled(item.can_upload(), egui::Button::new(RichText::new("📤 上传").color(TEXT_COLOR).size(11.0)).fill(BTN_BG).stroke(Stroke::new(1.0, CARD_BORDER))).clicked() {
                                        upload_target = Some(item.name.clone());
                                    }
                                    if ui.add_enabled(item.can_download(), egui::Button::new(RichText::new("📥 下载").color(TEXT_COLOR).size(11.0)).fill(BTN_BG).stroke(Stroke::new(1.0, CARD_BORDER))).clicked() {
                                        download_target = Some(item.name.clone());
                                    }
                                });
                            });
                        });
                        ui.separator();
                    }

                    if let Some(target) = upload_target {
                        let _ = self.task_tx.send(BgTask::UploadSkill {
                            server_url: self.server_url.clone(),
                            name: target.clone(),
                            local_skill_dir: get_skills_dir().join(&target),
                            silent: false,
                        });
                    }
                    if let Some(target) = download_target {
                        let _ = self.task_tx.send(BgTask::DownloadSkill {
                            server_url: self.server_url.clone(),
                            name: target,
                            target_skills_dir: get_skills_dir(),
                            silent: false,
                        });
                    }
                });
            });
    }

    fn run_diff(&mut self) {
        let key = self.active_file_key();
        self.log("==================================================");
        self.log(&format!("正在比对本地与云端 {key} 配置..."));

        let local = self.local_config_text.trim().to_string();
        let remote = self.remote_config_text.trim().to_string();

        if remote.is_empty() || remote.starts_with("# 获取云端") {
            self.log("请先刷新云端配置以进行比对。");
            self.status_dialog = Some(("提示".to_string(), "请先刷新云端配置以进行比对。".to_string(), false));
            return;
        }

        if key == "config.yml" {
            let (clean_local, stripped) = sanitize_for_upload(&local);
            self.log(&format!("[本地保护项] Windows shellPath = {}", stripped.unwrap_or_else(|| "(无)".to_string())));

            if clean_local.trim() == remote.trim() {
                self.log("🎉 本地核心配置 (模型角色/主题/思考级别等) 与云端完全一致！");
                self.status_dialog = Some(("比对结果".to_string(), "核心配置与云端完全一致！\n(已忽略本地平台独有的 shellPath 差异)".to_string(), false));
            } else {
                self.log("⚠️ 检测到配置存在差异，请查看两侧具体差异项。");
                self.status_dialog = Some(("比对结果".to_string(), "检测到配置存在差异！\n请通过两侧编辑框查看具体差异项。".to_string(), false));
            }
        } else {
            if local == remote {
                self.log(&format!("🎉 本地 {key} 与云端完全一致！"));
                self.status_dialog = Some(("比对结果".to_string(), format!("{key} 与云端完全一致！"), false));
            } else {
                self.log(&format!("⚠️ 本地与云端 {key} 存在差异。"));
                self.status_dialog = Some(("比对结果".to_string(), "检测到配置存在差异！\n请通过两侧编辑框查看具体差异项。".to_string(), false));
            }
        }
    }
}

fn handle_bg_task(task: BgTask, sender: &Sender<BgResult>) {
    match task {
        BgTask::TestConnection { server_url } => {
            let client = SyncApiClient::new(&server_url);
            match &client.test_connection() {
                Ok(msg) => {
                    let _ = sender.send(BgResult::ConnectionTested { success: true, message: msg.clone() });
                }
                Err(err) => {
                    let _ = sender.send(BgResult::ConnectionTested { success: false, message: err.clone() });
                }
            }
        }
        BgTask::RefreshRemoteFile { server_url, file_key } => {
            let client = SyncApiClient::new(&server_url);
            match &client.get_remote_file(&file_key) {
                Ok(content) => {
                    let _ = sender.send(BgResult::RemoteFileLoaded { file_key, content: content.clone() });
                }
                Err(e) => {
                    let _ = sender.send(BgResult::Log(format!("获取云端 {file_key} 失败: {e}")));
                    let _ = sender.send(BgResult::RemoteFileLoaded { file_key, content: format!("# 获取失败: {e}") });
                }
            }
        }
        BgTask::PullFile { server_url, file_key, show_msg } => {
            let client = SyncApiClient::new(&server_url);
            let agent_dir = get_agent_dir();
            let local_path = agent_dir.join(&file_key);

            match &client.get_remote_file(&file_key) {
                Ok(remote_content) => {
                    if remote_content.trim().is_empty() {
                        let _ = sender.send(BgResult::Log(format!("[警告] 远端 {file_key} 内容为空，跳过覆盖。")));
                        return;
                    }

                    let (final_content, injected) = if file_key == "config.yml" {
                        let cur = fs::read_to_string(&local_path).unwrap_or_default();
                        restore_after_download(remote_content, Some(&cur))
                    } else {
                        (remote_content.clone(), String::new())
                    };

                    if local_path.exists() {
                        let bak_path = agent_dir.join(format!("{file_key}.bak"));
                        let _ = fs::copy(&local_path, &bak_path);
                        let _ = sender.send(BgResult::Log(format!("[备份] 已将本地 {file_key} 备份为 {file_key}.bak")));
                    }

                    let _ = fs::create_dir_all(&agent_dir);
                    if let Err(e) = fs::write(&local_path, &final_content) {
                        let _ = sender.send(BgResult::TaskCompleted {
                            message: format!("写入 {file_key} 失败: {e}"),
                            is_error: true,
                            refresh_after: false,
                            refresh_skills_after: false,
                        });
                        return;
                    }

                    let msg = if file_key == "config.yml" {
                        format!("拉取成功！已自动保留/注入本地 Windows Bash: {injected}")
                    } else {
                        format!("{file_key} 拉取同步成功！")
                    };

                    let _ = sender.send(BgResult::TaskCompleted {
                        message: if show_msg { msg } else { format!("{file_key} 拉取成功") },
                        is_error: false,
                        refresh_after: true,
                        refresh_skills_after: false,
                    });
                }
                Err(e) => {
                    let _ = sender.send(BgResult::TaskCompleted {
                        message: format!("拉取 {file_key} 失败: {e}"),
                        is_error: true,
                        refresh_after: false,
                        refresh_skills_after: false,
                    });
                }
            }
        }
        BgTask::PushFile { server_url, file_key, content, show_msg } => {
            let client = SyncApiClient::new(&server_url);
            let (clean_content, _stripped) = if file_key == "config.yml" {
                let (c, s) = sanitize_for_upload(&content);
                if let Some(sp) = &s {
                    let _ = sender.send(BgResult::Log(format!("🛡️ 已自动剔除 Windows 路径 '{sp}'，保护服务器 Linux 环境。")));
                }
                (c, s)
            } else {
                (content, None)
            };

            match &client.save_remote_file(&file_key, &clean_content, true) {
                Ok(_) => {
                    let _ = sender.send(BgResult::TaskCompleted {
                        message: if show_msg {
                            format!("推送成功！\n{file_key} 已上传至云端并分发至集群服务器。\n已自动剔除 Windows 路径保护服务器环境。")
                        } else {
                            format!("{file_key} 推送成功！")
                        },
                        is_error: false,
                        refresh_after: true,
                        refresh_skills_after: false,
                    });
                }
                Err(e) => {
                    let _ = sender.send(BgResult::TaskCompleted {
                        message: format!("推送 {file_key} 失败: {e}"),
                        is_error: true,
                        refresh_after: false,
                        refresh_skills_after: false,
                    });
                }
            }
        }
        BgTask::PullBoth { server_url } => {
            let client = SyncApiClient::new(&server_url);
            let agent_dir = get_agent_dir();

            let mut success = true;
            let mut err_msg = String::new();

            // Pull config.yml
            match &client.get_remote_file("config.yml") {
                Ok(remote_config) => {
                    let local_path = agent_dir.join("config.yml");
                    let cur = fs::read_to_string(&local_path).unwrap_or_default();
                    let (merged, injected) = restore_after_download(remote_config, Some(&cur));
                    if local_path.exists() {
                        let _ = fs::copy(&local_path, agent_dir.join("config.yml.bak"));
                    }
                    let _ = fs::create_dir_all(&agent_dir);
                    let _ = fs::write(&local_path, merged);
                    let _ = sender.send(BgResult::Log(format!("✅ config.yml 拉取成功！注入: {injected}")));
                }
                Err(e) => {
                    success = false;
                    err_msg = format!("拉取 config.yml 失败: {e}");
                }
            }

            // Pull models.yml
            if success {
                match &client.get_remote_file("models.yml") {
                    Ok(remote_models) => {
                        let _ = fs::write(agent_dir.join("models.yml"), remote_models);
                        let _ = sender.send(BgResult::Log("✅ models.yml 拉取成功！".to_string()));
                    }
                    Err(e) => {
                        let _ = sender.send(BgResult::Log(format!("⚠️ 拉取 models.yml 失败: {e}")));
                    }
                }
            }

            let _ = sender.send(BgResult::TaskCompleted {
                message: if success {
                    "全量配置拉取完成！\n已自动为您保留并注入 Windows Bash 路径。".to_string()
                } else {
                    err_msg
                },
                is_error: !success,
                refresh_after: true,
                refresh_skills_after: false,
            });
        }
        BgTask::PushBoth { server_url, local_config, local_models } => {
            let client = SyncApiClient::new(&server_url);
            let (clean_cfg, stripped) = sanitize_for_upload(&local_config);
            if let Some(sp) = &stripped {
                let _ = sender.send(BgResult::Log(format!("🛡️ 已自动剔除 Windows 路径 '{sp}'，保护服务器 Linux 环境。")));
            }

            let mut success = true;
            let mut err_msg = String::new();

            match &client.save_remote_file("config.yml", &clean_cfg, true) {
                Ok(_) => {
                    let _ = sender.send(BgResult::Log("✅ config.yml 已成功推送并广播！".to_string()));
                }
                Err(e) => {
                    success = false;
                    err_msg = format!("推送 config.yml 失败: {e}");
                }
            }

            if success {
                if let Some(models) = &local_models {
                    match &client.save_remote_file("models.yml", models, true) {
                        Ok(_) => {
                            let _ = sender.send(BgResult::Log("✅ models.yml 已成功推送并广播！".to_string()));
                        }
                        Err(e) => {
                            let _ = sender.send(BgResult::Log(format!("⚠️ 推送 models.yml 失败: {e}")));
                        }
                    }
                }
            }

            let _ = sender.send(BgResult::TaskCompleted {
                message: if success {
                    "全量配置推送成功！\n已自动剔除 Windows 路径并触发集群广播更新。".to_string()
                } else {
                    err_msg
                },
                is_error: !success,
                refresh_after: true,
                refresh_skills_after: false,
            });
        }
        BgTask::RefreshSkills { server_url, skills_dir } => {
            let client = SyncApiClient::new(&server_url);
            let local_skills = scan_local_skills(&skills_dir);
            let remote_skills = client.get_remote_skills().unwrap_or_else(|e| {
                let _ = sender.send(BgResult::Log(format!("获取云端 Skills 列表失败: {e}")));
                Vec::new()
            });
            let _ = sender.send(BgResult::SkillsLoaded {
                local: local_skills,
                remote: remote_skills,
            });
        }
        BgTask::UploadSkill { server_url, name, local_skill_dir, silent } => {
            let client = SyncApiClient::new(&server_url);
            let _ = sender.send(BgResult::Log(format!("正在打包上传 Skill: '{name}'...")));
            match &client.upload_skill_directory(&name, &local_skill_dir, true) {
                Ok(_) => {
                    let _ = sender.send(BgResult::Log(format!("✅ Skill '{name}' 上传成功，已触发服务器向集群所有节点同步！")));
                    if !silent {
                        let _ = sender.send(BgResult::TaskCompleted {
                            message: format!("Skill '{name}' 上传成功并已分发至集群服务器！"),
                            is_error: false,
                            refresh_after: false,
                            refresh_skills_after: true,
                        });
                    }
                }
                Err(e) => {
                    let _ = sender.send(BgResult::TaskCompleted {
                        message: format!("上传 Skill '{name}' 失败: {e}"),
                        is_error: true,
                        refresh_after: false,
                        refresh_skills_after: false,
                    });
                }
            }
        }
        BgTask::DownloadSkill { server_url, name, target_skills_dir, silent } => {
            let client = SyncApiClient::new(&server_url);
            let _ = sender.send(BgResult::Log(format!("正在从云端下载 Skill: '{name}'...")));
            match &client.download_skill(&name, &target_skills_dir) {
                Ok(_) => {
                    let _ = sender.send(BgResult::Log(format!("✅ Skill '{name}' 下载解压成功！")));
                    if !silent {
                        let _ = sender.send(BgResult::TaskCompleted {
                            message: format!("Skill '{name}' 已成功下载解压到本机 skills/ 目录！"),
                            is_error: false,
                            refresh_after: false,
                            refresh_skills_after: true,
                        });
                    }
                }
                Err(e) => {
                    let _ = sender.send(BgResult::TaskCompleted {
                        message: format!("下载 Skill '{name}' 失败: {e}"),
                        is_error: true,
                        refresh_after: false,
                        refresh_skills_after: false,
                    });
                }
            }
        }
        BgTask::UploadSelectedSkills { server_url, skills_dir, names } => {
            let client = SyncApiClient::new(&server_url);
            let total = names.len();
            let _ = sender.send(BgResult::Log("==================================================".to_string()));
            let _ = sender.send(BgResult::Log(format!("开始批量上传所选 {total} 个 Skill 到云端...")));

            let mut success = 0;
            for name in &names {
                let local_dir = skills_dir.join(name);
                match &client.upload_skill_directory(name, &local_dir, true) {
                    Ok(_) => {
                        success += 1;
                        let _ = sender.send(BgResult::Log(format!("✅ Skill '{name}' 上传成功！")));
                    }
                    Err(e) => {
                        let _ = sender.send(BgResult::Log(format!("❌ 上传 Skill '{name}' 失败: {e}")));
                    }
                }
            }

            let _ = sender.send(BgResult::TaskCompleted {
                message: format!("批量上传完成！成功上传 {success}/{total} 个 Skill 并已分发至集群。"),
                is_error: success == 0 && total > 0,
                refresh_after: false,
                refresh_skills_after: true,
            });
        }
        BgTask::DownloadSelectedSkills { server_url, skills_dir, names } => {
            let client = SyncApiClient::new(&server_url);
            let total = names.len();
            let _ = sender.send(BgResult::Log("==================================================".to_string()));
            let _ = sender.send(BgResult::Log(format!("开始批量下载所选 {total} 个 Skill 到本机...")));

            let mut success = 0;
            for name in &names {
                match &client.download_skill(name, &skills_dir) {
                    Ok(_) => {
                        success += 1;
                        let _ = sender.send(BgResult::Log(format!("✅ Skill '{name}' 下载解压成功！")));
                    }
                    Err(e) => {
                        let _ = sender.send(BgResult::Log(format!("❌ 下载 Skill '{name}' 失败: {e}")));
                    }
                }
            }

            let _ = sender.send(BgResult::TaskCompleted {
                message: format!("批量下载完成！成功下载 {success}/{total} 个 Skill 到本机目录。"),
                is_error: success == 0 && total > 0,
                refresh_after: false,
                refresh_skills_after: true,
            });
        }
    }
}

pub fn apply_dark_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.dark_mode = true;
    visuals.panel_fill = BG_COLOR;
    visuals.window_fill = CARD_BG;
    visuals.extreme_bg_color = CODE_BG;
    visuals.code_bg_color = CODE_BG;
    visuals.override_text_color = Some(TEXT_COLOR);

    visuals.widgets.noninteractive.bg_fill = CARD_BG;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_COLOR);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, CARD_BORDER);

    visuals.widgets.inactive.bg_fill = BTN_BG;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_COLOR);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, CARD_BORDER);

    visuals.widgets.hovered.bg_fill = CARD_BORDER;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BLUE_ACCENT);

    visuals.widgets.active.bg_fill = BTN_PRIMARY;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);

    visuals.selection.bg_fill = Color32::from_rgb(26, 79, 160);
    visuals.selection.stroke = Stroke::new(1.0, BLUE_ACCENT);

    ctx.set_visuals(visuals);
}

fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let windir = std::env::var("WINDIR").or_else(|_| std::env::var("SystemRoot")).unwrap_or_else(|_| "C:\\Windows".to_string());
    let win_fonts = PathBuf::from(windir).join("Fonts");

    let mut cjk_candidates = vec![
        win_fonts.join("msyh.ttc"),
        win_fonts.join("msyh.ttf"),
        win_fonts.join("simhei.ttf"),
        win_fonts.join("simsun.ttc"),
        win_fonts.join("deng.ttf"),
        PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
        PathBuf::from("/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc"),
        PathBuf::from("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc"),
        PathBuf::from("/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc"),
        PathBuf::from("/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf"),
    ];

    if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
        let user_fonts = PathBuf::from(local_app).join("Microsoft").join("Windows").join("Fonts");
        cjk_candidates.push(user_fonts.join("msyh.ttc"));
        cjk_candidates.push(user_fonts.join("simhei.ttf"));
    }

    for path in &cjk_candidates {
        if let Ok(bytes) = fs::read(path) {
            fonts.font_data.insert(
                "cjk_fallback".to_owned(),
                egui::FontData::from_owned(bytes),
            );
            if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                vec.push("cjk_fallback".to_owned());
            }
            if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                vec.push("cjk_fallback".to_owned());
            }
            break;
        }
    }
    let emoji_candidates = [
        r"C:\Windows\Fonts\seguiemj.ttf",
        r"C:\Windows\Fonts\seguisym.ttf",
        "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
        "/usr/share/fonts/noto/NotoColorEmoji.ttf",
    ];

    for path in &emoji_candidates {
        if let Ok(bytes) = fs::read(path) {
            fonts.font_data.insert(
                "emoji_fallback".to_owned(),
                egui::FontData::from_owned(bytes),
            );
            if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                vec.push("emoji_fallback".to_owned());
            }
            if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                vec.push("emoji_fallback".to_owned());
            }
            break;
        }
    }

    ctx.set_fonts(fonts);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_setup() {
        let ctx = egui::Context::default();
        setup_custom_fonts(&ctx);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            let galley = ctx.fonts(|f| f.layout_no_wrap("🛡️ 配置文件同步 📥 拉取 📤 推送".to_string(), egui::FontId::proportional(14.0), egui::Color32::WHITE));
            assert!(galley.size().x > 0.0);
            println!("Galley rendered width (with emojis): {}", galley.size().x);
        });
}
}
