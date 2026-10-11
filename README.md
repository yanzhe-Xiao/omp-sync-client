# OmpSyncClient (跨平台 Rust 双向同步客户端)

[![Build and Release](https://github.com/yanzhe-Xiao/omp-sync-client/actions/workflows/release.yml/badge.svg)](https://github.com/yanzhe-Xiao/omp-sync-client/actions/workflows/release.yml)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Ubuntu%20%7C%20Linux-blue.svg)](https://github.com/yanzhe-Xiao/omp-sync-client)
[![Rust](https://img.shields.io/badge/language-Rust%202021-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

基于 **Rust** 原生重构的 OMP 高性能跨平台配置与技能双向同步客户端。  
支持 **Windows 桌面端 (GUI + CLI)** 与 **Ubuntu / Linux 服务器端 (原生 .deb / CLI / GUI)**，具有轻量、极致性能、内存安全与零环境依赖等特点。

专用于协调 **本机客户端** 与 **Linux 集群 Master 节点 (`http://10.69.211.84:8900`)** 之间的核心配置、模型角色与技能（Skills）双向同步。

---

## 🌟 核心问题与解决方案：跨平台 `shellPath` 深度隔离

### ⚠️ 问题背景
- **Windows 本机**：OMP 依赖 `shellPath` 指向 Git Bash 可执行文件（如 `C:\Users\xyzxy\AppData\Local\UGit\app-5.48.0\resources\app\git\usr\bin\bash.exe`），否则 Windows 无法正确调用命令行工具。
- **Linux 服务器**：OMP 默认调用 Linux 原生 `/bin/bash`。若将 Windows 路径直接上传，会导致 Linux 集群节点报错；
- **反向覆盖风险**：若直接从 Linux 集群拉取配置，因 Linux 配置中无 `shellPath`，若直接覆盖本机，会导致本地 Windows OMP 丢失 bash 配置而无法启动。

### 🛡️ 智能跨平台隔离与自愈机制 (Smart Platform Isolation)
本程序底层采用 Rust `shell_path` 模块，在传输层全面隔离平台差异：

1. **推送到云端 (Push 时)**：
   - 自动检测并剥离 Windows `shellPath`，本地自动缓存至 `~/.omp/agent/.local_shell_path.txt`；
   - 推送至云端及广播到集群其他节点的配置始终为干净的平台无关配置，绝不污染 Linux 服务器。
2. **从云端拉取 (Pull 时)**：
   - 从服务器拉取最新的核心配置与模型设定；
   - 在 Windows 上，自动从（旧配置 / 本地缓存 / UGit 与 Git 常见路径）中提取 Bash 路径并注入；
   - 自动备份旧配置为 `config.yml.bak`；
   - **在 Ubuntu/Linux 原生运行模式下，自动保持原生 Linux 纯净配置，绝不产生多余的 Windows 路径！**

---

## 📦 软件包下载与安装

### 1. Windows (x64)
- **下载**：`omp-sync-client-windows-x64.zip`（内含独立绿色免安装可执行程序）
- **运行**：
  - 解压后双击 `run.bat` 或 `omp-sync-client.exe` 启动图形界面。
  - 或在命令行终端直接调用 CLI 指令：
    ```cmd
    omp-sync-client.exe pull
    omp-sync-client.exe push
    ```

### 2. Ubuntu / Debian (x86_64)
- **方式 A：Debian 安装包 (.deb)**：
  ```bash
  sudo dpkg -i omp-sync-client_1.0.0_amd64.deb
  # 随后可在终端直接运行
  omp-sync-client
  ```
- **方式 B：独立便携归档 (.tar.gz)**：
  ```bash
  tar -zxvf omp-sync-client-linux-x64.tar.gz
  cd dist/omp-sync-client-linux-x64
  sudo ./install.sh
  ```

---

## 🚀 功能清单与使用指南

### 1. 📝 配置文件双向同步 (config.yml / models.yml)
- **单选切换查看**：
  - 🔘 `config.yml`：模型角色（smol/default/slow/plan）、主题、思考级别，以及自动隔离保护的 `shellPath`。
  - 🔘 `models.yml`：自定义 API Key、Endpoint、Providers 与私有模型定义。
- **两侧比对视图**：左侧展示本机当前文件内容，右侧展示云端 Master 上的最新内容。
- **快捷操作**：
  - `📥 拉取当前文件`：仅拉取当前选择的文件（如仅拉取 `config.yml` 或 `models.yml`）。
  - `📤 推送当前文件`：仅推送当前选择的文件到服务器。
  - `📥 拉取两者 (Both)`：一键将云端的 `config.yml` 与 `models.yml` 全部拉取同步至本机。
  - `📤 推送两者 (Both)`：一键将本机的 `config.yml` 与 `models.yml` 全部推送至云端并触发集群广播。
  - `🔍 比对差异`：智能比对本地与远端内容，自动屏蔽平台特定的 `shellPath` 干扰项。

### 2. 🧩 技能选择性同步 (Selective Skills Management)
- **全量技能状态汇总表格**：
  - 本地扫描 `~/.omp/agent/skills/`，解析 `SKILL.md` 描述与文件统计。
  - 云端获取 `/api/skills`，汇总并集。
  - 清晰显示每一项的 **本地状态**（已安装/未安装，文件数）与 **云端状态**（已就绪/未就绪，文件数）。
- **自由勾选与批量操作**：
  - 支持复选框勾选想要同步的技能项；
  - `[☑️ 全选]` 与 `[⬜ 全不选]`；
  - `[📤 上传所选技能到云端]`：打包勾选的本地技能并上传至服务器，自动触发集群广播；
  - `[📥 从云端下载所选技能]`：从云端下载勾选的技能并解压到本机；
- **单项快捷按钮**：
  - 每一行技能均有专属的 `[📤 上传]` 与 `[📥 下载]` 按钮。

### 3. ⌨️ CLI 命令行模式 (无头服务器 / 自动化脚本最佳实践)
```bash
# 测试集群 Master 连通性
omp-sync-client test [server_url]

# 拉取配置 (自动注入平台适配的 shellPath 并备份旧文件)
omp-sync-client pull [server_url]

# 推送配置 (自动剥离平台专属路径并触发集群同步)
omp-sync-client push [server_url]

# 差异比对
omp-sync-client diff [server_url]

# 技能管理
omp-sync-client skills list [server_url]
omp-sync-client skills pull <skill_name> [server_url]
omp-sync-client skills push <skill_name> [server_url]
```

---

## 🛠️ 本地编译构建

本项目基于 Rust 2021 开发，可在任意装有 Rust 的系统上编译：

```bash
# 1. 编译本机原生 Release 二进制
cargo build --release

# 2. 运行自动化全平台打包脚本 (生成 zip, tar.gz, .deb)
python package.py
```

---

## 📄 License
MIT License
