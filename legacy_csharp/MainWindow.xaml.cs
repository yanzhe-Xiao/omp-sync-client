using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.ComponentModel;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Media;

namespace OmpSyncClient
{
    public class SkillItemViewModel : INotifyPropertyChanged
    {
        private bool _isSelected;
        public string Name { get; set; } = "";
        public string Description { get; set; } = "";
        public bool IsLocal { get; set; }
        public bool IsRemote { get; set; }
        public int LocalFileCount { get; set; }
        public int RemoteFileCount { get; set; }

        public bool IsSelected
        {
            get => _isSelected;
            set
            {
                if (_isSelected != value)
                {
                    _isSelected = value;
                    OnPropertyChanged(nameof(IsSelected));
                }
            }
        }

        public string LocalStatusText => IsLocal ? $"✅ 已安装 ({LocalFileCount} 文件)" : "❌ 未安装";
        public Brush LocalStatusColor => IsLocal 
            ? new SolidColorBrush((Color)ColorConverter.ConvertFromString("#3FB950"))
            : new SolidColorBrush((Color)ColorConverter.ConvertFromString("#8B949E"));

        public string RemoteStatusText => IsRemote ? $"☁️ 已就绪 ({RemoteFileCount} 文件)" : "❌ 未上传";
        public Brush RemoteStatusColor => IsRemote
            ? new SolidColorBrush((Color)ColorConverter.ConvertFromString("#58A6FF"))
            : new SolidColorBrush((Color)ColorConverter.ConvertFromString("#8B949E"));

        public bool CanUpload => IsLocal;
        public bool CanDownload => IsRemote;

        public event PropertyChangedEventHandler? PropertyChanged;
        protected void OnPropertyChanged(string propName) => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(propName));
    }

    public partial class MainWindow : Window
    {
        private readonly SyncApiClient _client;
        private readonly string _localAgentDir;
        private readonly string _localSkillsDir;
        private string _activeFileKey = "config.yml";

        public ObservableCollection<SkillItemViewModel> SkillsList { get; set; } = new();

        public MainWindow()
        {
            InitializeComponent();

            _localAgentDir = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.UserProfile),
                ".omp", "agent"
            );
            _localSkillsDir = Path.Combine(_localAgentDir, "skills");

            _client = new SyncApiClient(TxtServerUrl.Text.Trim());
            DgSkills.ItemsSource = SkillsList;

            Loaded += MainWindow_Loaded;
        }

        private async void MainWindow_Loaded(object sender, RoutedEventArgs e)
        {
            DetectAndDisplayBash();
            LoadLocalFile(_activeFileKey);
            await AutoTestConnection();
            await RefreshSkillsList();
        }

        private void Log(string msg)
        {
            var time = DateTime.Now.ToString("HH:mm:ss");
            TxtLogs.AppendText($"[{time}] {msg}\n");
            TxtLogs.ScrollToEnd();
        }

        private void DetectAndDisplayBash()
        {
            var localConfigPath = Path.Combine(_localAgentDir, "config.yml");
            var localContent = File.Exists(localConfigPath) ? File.ReadAllText(localConfigPath) : "";
            var bashPath = ShellPathManager.DetectLocalBashPath(localContent);

            if (!string.IsNullOrEmpty(bashPath))
            {
                TxtDetectedBash.Text = bashPath;
                TxtDetectedBash.Foreground = new SolidColorBrush((Color)ColorConverter.ConvertFromString("#7EE787"));
                Log($"检测到本地 Windows Git Bash: {bashPath}");
            }
            else
            {
                TxtDetectedBash.Text = "未在默认路径找到 Git Bash，将尝试使用环境备用路径";
                TxtDetectedBash.Foreground = new SolidColorBrush((Color)ColorConverter.ConvertFromString("#D29922"));
                Log("未检测到本地 bash.exe，请确认 Git 或 UGit 是否已安装。");
            }
        }

        private async Task AutoTestConnection()
        {
            _client.BaseUrl = TxtServerUrl.Text.Trim();
            var (ok, msg) = await _client.TestConnectionAsync();
            if (ok)
            {
                TxtConnStatus.Text = "● 在线 (Connected)";
                TxtConnStatus.Foreground = new SolidColorBrush((Color)ColorConverter.ConvertFromString("#3FB950"));
                Log($"云端服务器连接成功: {_client.BaseUrl}");
                await RefreshRemoteFile(_activeFileKey);
            }
            else
            {
                TxtConnStatus.Text = "● 离线 (Disconnected)";
                TxtConnStatus.Foreground = new SolidColorBrush((Color)ColorConverter.ConvertFromString("#F85149"));
                Log($"云端服务器连接失败: {msg}");
            }
        }

        private async void BtnTestConn_Click(object sender, RoutedEventArgs e)
        {
            Log($"正在测试与 {_client.BaseUrl} 的连接...");
            await AutoTestConnection();
            await RefreshSkillsList();
        }

        // =====================================================================
        // FILE CONFIGURATION TAB (config.yml / models.yml)
        // =====================================================================

        private void FileRadio_Checked(object sender, RoutedEventArgs e)
        {
            if (RbConfigFile == null || RbModelsFile == null) return;
            _activeFileKey = RbConfigFile.IsChecked == true ? "config.yml" : "models.yml";

            TxtLocalTitle.Text = $"💻 本机配置 ({_activeFileKey})";
            TxtRemoteTitle.Text = $"☁️ 云端 Master 配置 ({_activeFileKey})";

            LoadLocalFile(_activeFileKey);
            _ = RefreshRemoteFile(_activeFileKey);
        }

        private void LoadLocalFile(string fileKey)
        {
            var filePath = Path.Combine(_localAgentDir, fileKey);
            if (File.Exists(filePath))
            {
                TxtLocalConfig.Text = File.ReadAllText(filePath);
                Log($"已载入本地 {fileKey} 文件。");
            }
            else
            {
                TxtLocalConfig.Text = $"# 本地未检测到 {fileKey}";
                Log($"本地未找到 {fileKey} ({filePath})");
            }
        }

        private async Task RefreshRemoteFile(string fileKey)
        {
            _client.BaseUrl = TxtServerUrl.Text.Trim();
            try
            {
                var content = await _client.GetRemoteFileAsync(fileKey);
                TxtRemoteConfig.Text = content;
                Log($"成功获取云端最新 {fileKey}。");
            }
            catch (Exception ex)
            {
                TxtRemoteConfig.Text = $"# 获取云端文件失败: {ex.Message}";
                Log($"获取云端 {fileKey} 失败: {ex.Message}");
            }
        }

        private void BtnRefreshLocal_Click(object sender, RoutedEventArgs e)
        {
            LoadLocalFile(_activeFileKey);
            if (_activeFileKey == "config.yml") DetectAndDisplayBash();
        }

        private async void BtnRefreshRemote_Click(object sender, RoutedEventArgs e)
        {
            await RefreshRemoteFile(_activeFileKey);
        }

        private async void BtnPullCurrent_Click(object sender, RoutedEventArgs e)
        {
            await PullFileAsync(_activeFileKey);
        }

        private async void BtnPushCurrent_Click(object sender, RoutedEventArgs e)
        {
            await PushFileAsync(_activeFileKey);
        }

        private async void BtnPullBoth_Click(object sender, RoutedEventArgs e)
        {
            Log("==================================================");
            Log("开始一键从云端拉取全部配置 (config.yml + models.yml)...");
            await PullFileAsync("config.yml", showMsg: false);
            await PullFileAsync("models.yml", showMsg: false);
            MessageBox.Show("全量配置拉取完成！\n已自动为您保留并注入 Windows Bash 路径。", "拉取完成", MessageBoxButton.OK, MessageBoxImage.Information);
        }

        private async void BtnPushBoth_Click(object sender, RoutedEventArgs e)
        {
            Log("==================================================");
            Log("开始一键推送全部配置至云端 (config.yml + models.yml)...");
            await PushFileAsync("config.yml", showMsg: false);
            await PushFileAsync("models.yml", showMsg: false);
            MessageBox.Show("全量配置推送成功！\n已自动剔除 Windows 路径并触发集群广播更新。", "推送完成", MessageBoxButton.OK, MessageBoxImage.Information);
        }

        private async Task PullFileAsync(string fileKey, bool showMsg = true)
        {
            _client.BaseUrl = TxtServerUrl.Text.Trim();
            var localPath = Path.Combine(_localAgentDir, fileKey);

            try
            {
                var remoteContent = await _client.GetRemoteFileAsync(fileKey);
                if (string.IsNullOrWhiteSpace(remoteContent))
                {
                    Log($"[警告] 远端 {fileKey} 内容为空，跳过覆盖。");
                    return;
                }

                string finalContent = remoteContent;
                string? injected = null;

                if (fileKey == "config.yml")
                {
                    var localContent = File.Exists(localPath) ? File.ReadAllText(localPath) : "";
                    (finalContent, injected) = ShellPathManager.RestoreAfterDownload(remoteContent, localContent);
                }

                // Backup local
                if (File.Exists(localPath))
                {
                    File.Copy(localPath, localPath + ".bak", true);
                    Log($"[备份] 已将本地 {fileKey} 备份为 {fileKey}.bak");
                }

                Directory.CreateDirectory(_localAgentDir);
                File.WriteAllText(localPath, finalContent);

                if (_activeFileKey == fileKey)
                {
                    TxtLocalConfig.Text = finalContent;
                }

                if (fileKey == "config.yml")
                {
                    Log($"✅ {fileKey} 拉取成功！已自动保留/注入本地 Windows Bash: {injected}");
                    if (showMsg)
                    {
                        MessageBox.Show($"拉取成功！\n已自动为您保留并注入 Windows 原生 Bash 路径:\n{injected}", "同步完成", MessageBoxButton.OK, MessageBoxImage.Information);
                    }
                }
                else
                {
                    Log($"✅ {fileKey} 拉取同步成功！");
                    if (showMsg)
                    {
                        MessageBox.Show($"拉取成功！本地 {fileKey} 已更新。", "同步完成", MessageBoxButton.OK, MessageBoxImage.Information);
                    }
                }
            }
            catch (Exception ex)
            {
                Log($"❌ 拉取 {fileKey} 失败: {ex.Message}");
                if (showMsg) MessageBox.Show($"拉取失败: {ex.Message}", "错误", MessageBoxButton.OK, MessageBoxImage.Error);
            }
        }

        private async Task PushFileAsync(string fileKey, bool showMsg = true)
        {
            _client.BaseUrl = TxtServerUrl.Text.Trim();
            var localPath = Path.Combine(_localAgentDir, fileKey);

            if (!File.Exists(localPath))
            {
                Log($"[错误] 本地未找到 {fileKey} 文件。");
                if (showMsg) MessageBox.Show($"本地未找到 {fileKey} 文件。", "提示", MessageBoxButton.OK, MessageBoxImage.Warning);
                return;
            }

            try
            {
                var localYaml = File.ReadAllText(localPath);
                string cleanYaml = localYaml;

                if (fileKey == "config.yml")
                {
                    string? stripped;
                    (cleanYaml, stripped) = ShellPathManager.SanitizeForUpload(localYaml);
                    if (!string.IsNullOrEmpty(stripped))
                    {
                        Log($"🛡️ 已自动剔除 Windows 路径 '{stripped}'，避免破坏服务器 Linux 环境。");
                    }
                }

                await _client.SaveRemoteFileAsync(fileKey, cleanYaml, syncNow: true);
                Log($"✅ {fileKey} 已成功推送至云端并触发集群广播同步！");

                if (_activeFileKey == fileKey)
                {
                    await RefreshRemoteFile(fileKey);
                }

                if (showMsg)
                {
                    MessageBox.Show($"推送成功！\n{fileKey} 已上传至云端并分发至集群服务器。\n已自动剔除 Windows 路径保护服务器环境。", "推送完成", MessageBoxButton.OK, MessageBoxImage.Information);
                }
            }
            catch (Exception ex)
            {
                Log($"❌ 推送 {fileKey} 失败: {ex.Message}");
                if (showMsg) MessageBox.Show($"推送失败: {ex.Message}", "错误", MessageBoxButton.OK, MessageBoxImage.Error);
            }
        }

        private void BtnDiff_Click(object sender, RoutedEventArgs e)
        {
            Log("==================================================");
            Log($"正在比对本地与云端 {_activeFileKey} 配置...");

            var local = TxtLocalConfig.Text.Trim();
            var remote = TxtRemoteConfig.Text.Trim();

            if (string.IsNullOrEmpty(remote) || remote.StartsWith("# 获取云端"))
            {
                Log("请先拉取云端配置以进行比对。");
                return;
            }

            if (_activeFileKey == "config.yml")
            {
                var (cleanLocal, stripped) = ShellPathManager.SanitizeForUpload(local);
                Log($"[本地保护项] Windows shellPath = {stripped ?? "(无)"}");

                if (cleanLocal.Trim() == remote.Trim())
                {
                    Log("🎉 本地核心配置 (模型角色/主题/思考级别等) 与云端完全一致！");
                    MessageBox.Show("核心配置与云端完全一致！\n(已忽略本地平台独有的 shellPath 差异)", "比对结果", MessageBoxButton.OK, MessageBoxImage.Information);
                }
                else
                {
                    Log("⚠️ 检测到配置存在差异，请查看两侧具体差异项。");
                    MessageBox.Show("检测到配置存在差异！\n请通过两侧编辑框查看具体差异项。", "比对结果", MessageBoxButton.OK, MessageBoxImage.Warning);
                }
            }
            else
            {
                if (local.Trim() == remote.Trim())
                {
                    Log($"🎉 本地 { _activeFileKey} 与云端完全一致！");
                    MessageBox.Show($"{_activeFileKey} 与云端完全一致！", "比对结果", MessageBoxButton.OK, MessageBoxImage.Information);
                }
                else
                {
                    Log($"⚠️ 本地与云端 {_activeFileKey} 存在差异。");
                    MessageBox.Show("检测到配置存在差异！\n请通过两侧编辑框查看具体差异项。", "比对结果", MessageBoxButton.OK, MessageBoxImage.Warning);
                }
            }
        }

        // =====================================================================
        // SELECTIVE SKILLS TAB
        // =====================================================================

        private async void BtnRefreshSkills_Click(object sender, RoutedEventArgs e)
        {
            await RefreshSkillsList();
        }

        private async Task RefreshSkillsList()
        {
            Log("正在扫描本地与云端 Skills 列表...");
            _client.BaseUrl = TxtServerUrl.Text.Trim();

            // 1. Gather local skills
            var localSkillsDict = new Dictionary<string, (int FileCount, string Desc)>();
            if (Directory.Exists(_localSkillsDir))
            {
                foreach (var dir in Directory.GetDirectories(_localSkillsDir))
                {
                    var name = Path.GetFileName(dir);
                    var files = Directory.GetFiles(dir, "*", SearchOption.AllDirectories);
                    var desc = "";
                    var skillMd = Path.Combine(dir, "SKILL.md");
                    if (File.Exists(skillMd))
                    {
                        try
                        {
                            var lines = File.ReadLines(skillMd).Take(15);
                            foreach (var line in lines)
                            {
                                if (line.StartsWith("description:", StringComparison.OrdinalIgnoreCase))
                                {
                                    desc = line.Substring("description:".Length).Trim();
                                    break;
                                }
                                else if (line.StartsWith("# ") && string.IsNullOrEmpty(desc))
                                {
                                    desc = line.Substring(2).Trim();
                                }
                            }
                        }
                        catch { }
                    }
                    localSkillsDict[name] = (files.Length, desc);
                }
            }

            // 2. Gather remote skills
            var remoteSkillsDict = new Dictionary<string, SkillDto>();
            try
            {
                var remoteList = await _client.GetRemoteSkillsAsync();
                foreach (var r in remoteList)
                {
                    remoteSkillsDict[r.Name] = r;
                }
            }
            catch (Exception ex)
            {
                Log($"获取云端 Skills 列表失败: {ex.Message}");
            }

            // 3. Union all skill names
            var allNames = localSkillsDict.Keys.Union(remoteSkillsDict.Keys).OrderBy(n => n).ToList();

            SkillsList.Clear();
            foreach (var name in allNames)
            {
                var isLocal = localSkillsDict.TryGetValue(name, out var localInfo);
                var isRemote = remoteSkillsDict.TryGetValue(name, out var remoteInfo);

                var item = new SkillItemViewModel
                {
                    Name = name,
                    Description = !string.IsNullOrEmpty(localInfo.Desc) ? localInfo.Desc : (remoteInfo?.Description ?? "无说明"),
                    IsLocal = isLocal,
                    IsRemote = isRemote,
                    LocalFileCount = isLocal ? localInfo.FileCount : 0,
                    RemoteFileCount = isRemote ? (remoteInfo?.FileCount ?? 0) : 0,
                    IsSelected = false
                };
                SkillsList.Add(item);
            }

            TxtSkillsSummary.Text = $"共检测到 {SkillsList.Count} 个技能 (本地 {localSkillsDict.Count} 个, 云端 {remoteSkillsDict.Count} 个)";
            Log($"Skills 列表更新完毕：共 {SkillsList.Count} 个技能。");
        }

        private void BtnSelectAllSkills_Click(object sender, RoutedEventArgs e)
        {
            foreach (var s in SkillsList) s.IsSelected = true;
        }

        private void BtnDeselectAllSkills_Click(object sender, RoutedEventArgs e)
        {
            foreach (var s in SkillsList) s.IsSelected = false;
        }

        private async void BtnUploadSingleSkill_Click(object sender, RoutedEventArgs e)
        {
            if (sender is FrameworkElement { Tag: SkillItemViewModel item })
            {
                await UploadSkillItemAsync(item);
            }
        }

        private async void BtnDownloadSingleSkill_Click(object sender, RoutedEventArgs e)
        {
            if (sender is FrameworkElement { Tag: SkillItemViewModel item })
            {
                await DownloadSkillItemAsync(item);
            }
        }

        private async void BtnUploadSelectedSkills_Click(object sender, RoutedEventArgs e)
        {
            var selected = SkillsList.Where(s => s.IsSelected && s.IsLocal).ToList();
            if (selected.Count == 0)
            {
                MessageBox.Show("请勾选至少一个本地存在的 Skill 进行上传。", "提示", MessageBoxButton.OK, MessageBoxImage.Information);
                return;
            }

            Log("==================================================");
            Log($"开始批量上传所选 {selected.Count} 个 Skill 到云端...");

            int success = 0;
            foreach (var s in selected)
            {
                try
                {
                    await UploadSkillItemAsync(s, silent: true);
                    success++;
                }
                catch (Exception ex)
                {
                    Log($"❌ 上传 Skill '{s.Name}' 失败: {ex.Message}");
                }
            }

            Log($"✅ 批量上传完成: {success}/{selected.Count} 成功！");
            await RefreshSkillsList();
            MessageBox.Show($"批量上传完成！成功上传 {success} 个 Skill 并已分发至集群。", "完成", MessageBoxButton.OK, MessageBoxImage.Information);
        }

        private async void BtnDownloadSelectedSkills_Click(object sender, RoutedEventArgs e)
        {
            var selected = SkillsList.Where(s => s.IsSelected && s.IsRemote).ToList();
            if (selected.Count == 0)
            {
                MessageBox.Show("请勾选至少一个云端存在的 Skill 进行下载。", "提示", MessageBoxButton.OK, MessageBoxImage.Information);
                return;
            }

            Log("==================================================");
            Log($"开始批量下载所选 {selected.Count} 个 Skill 到本机...");

            int success = 0;
            foreach (var s in selected)
            {
                try
                {
                    await DownloadSkillItemAsync(s, silent: true);
                    success++;
                }
                catch (Exception ex)
                {
                    Log($"❌ 下载 Skill '{s.Name}' 失败: {ex.Message}");
                }
            }

            Log($"✅ 批量下载完成: {success}/{selected.Count} 成功！");
            await RefreshSkillsList();
            MessageBox.Show($"批量下载完成！成功下载 {success} 个 Skill 到本机目录。", "完成", MessageBoxButton.OK, MessageBoxImage.Information);
        }

        private async Task UploadSkillItemAsync(SkillItemViewModel item, bool silent = false)
        {
            var localSkillDir = Path.Combine(_localSkillsDir, item.Name);
            if (!Directory.Exists(localSkillDir))
            {
                Log($"[错误] 本地未找到 Skill 目录: {localSkillDir}");
                return;
            }

            Log($"正在打包上传 Skill: '{item.Name}'...");
            await _client.UploadSkillDirectoryAsync(item.Name, localSkillDir, syncNow: true);
            Log($"✅ Skill '{item.Name}' 上传成功，已触发服务器向集群所有节点同步！");

            if (!silent)
            {
                await RefreshSkillsList();
                MessageBox.Show($"Skill '{item.Name}' 上传成功并已分发至集群服务器！", "完成", MessageBoxButton.OK, MessageBoxImage.Information);
            }
        }

        private async Task DownloadSkillItemAsync(SkillItemViewModel item, bool silent = false)
        {
            Log($"正在从云端下载 Skill: '{item.Name}'...");
            Directory.CreateDirectory(_localSkillsDir);
            await _client.DownloadSkillAsync(item.Name, _localSkillsDir);
            Log($"✅ Skill '{item.Name}' 下载解压成功！");

            if (!silent)
            {
                await RefreshSkillsList();
                MessageBox.Show($"Skill '{item.Name}' 已成功下载解压到本机 skills/ 目录！", "完成", MessageBoxButton.OK, MessageBoxImage.Information);
            }
        }

        private void BtnClearLog_Click(object sender, RoutedEventArgs e)
        {
            TxtLogs.Clear();
        }
    }
}
