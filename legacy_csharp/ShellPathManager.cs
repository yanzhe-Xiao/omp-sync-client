using System;
using System.IO;
using System.Text.RegularExpressions;
using YamlDotNet.Serialization;
using YamlDotNet.Serialization.NamingConventions;

namespace OmpSyncClient
{
    /// <summary>
    /// Manages cross-platform shellPath sanitization, caching, and auto-detection.
    /// Solves the conflict between Windows Git Bash paths and Linux native /bin/bash.
    /// </summary>
    public static class ShellPathManager
    {
        private static readonly string CacheFilePath = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.UserProfile),
            ".omp", "agent", ".local_shell_path.txt"
        );

        private static readonly IDeserializer Deserializer = new DeserializerBuilder().Build();
        private static readonly ISerializer Serializer = new SerializerBuilder()
            .WithNamingConvention(NullNamingConvention.Instance)
            .Build();

        /// <summary>
        /// Detects or recovers the local Windows Git Bash path.
        /// Priority:
        /// 1. Current value in local ~/.omp/agent/config.yml
        /// 2. Cached value in ~/.omp/agent/.local_shell_path.txt
        /// 3. Known UGit / Git installation paths on Windows
        /// </summary>
        public static string? DetectLocalBashPath(string localConfigYaml)
        {
            // 1. Try parse from existing local config
            if (!string.IsNullOrWhiteSpace(localConfigYaml))
            {
                try
                {
                    var data = Deserializer.Deserialize<Dictionary<string, object>>(localConfigYaml);
                    if (data != null && data.TryGetValue("shellPath", out var pathObj) && pathObj != null)
                    {
                        var path = pathObj.ToString()?.Trim();
                        if (!string.IsNullOrEmpty(path) && (path.Contains(":\\") || path.EndsWith(".exe", StringComparison.OrdinalIgnoreCase)))
                        {
                            SaveCache(path);
                            return path;
                        }
                    }
                }
                catch { }
            }

            // 2. Try read from persistent cache
            if (File.Exists(CacheFilePath))
            {
                try
                {
                    var cached = File.ReadAllText(CacheFilePath).Trim();
                    if (!string.IsNullOrEmpty(cached) && File.Exists(cached))
                        return cached;
                }
                catch { }
            }

            // 3. Search well-known Windows locations
            var userProfile = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
            var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            var progFiles = Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles);

            // Check UGit installations
            var ugitBase = Path.Combine(localAppData, "UGit");
            if (Directory.Exists(ugitBase))
            {
                var candidate = Directory.GetFiles(ugitBase, "bash.exe", SearchOption.AllDirectories);
                if (candidate.Length > 0)
                {
                    SaveCache(candidate[0]);
                    return candidate[0];
                }
            }

            // Check standard Git
            var standardGitCandidates = new[]
            {
                Path.Combine(progFiles, "Git", "bin", "bash.exe"),
                Path.Combine(progFiles, "Git", "usr", "bin", "bash.exe"),
                @"C:\Program Files\Git\bin\bash.exe",
                @"C:\Program Files\Git\usr\bin\bash.exe",
                Path.Combine(localAppData, "Programs", "Git", "bin", "bash.exe")
            };

            foreach (var cand in standardGitCandidates)
            {
                if (File.Exists(cand))
                {
                    SaveCache(cand);
                    return cand;
                }
            }

            return null;
        }

        public static void SaveCache(string bashPath)
        {
            try
            {
                var dir = Path.GetDirectoryName(CacheFilePath);
                if (!string.IsNullOrEmpty(dir) && !Directory.Exists(dir))
                    Directory.CreateDirectory(dir);
                File.WriteAllText(CacheFilePath, bashPath);
            }
            catch { }
        }

        /// <summary>
        /// Prepares config for UPLOAD to Linux server:
        /// Strips Windows 'shellPath' so Linux uses default /bin/bash, caching the Windows path locally.
        /// </summary>
        public static (string CleanYaml, string? StrippedPath) SanitizeForUpload(string localYaml)
        {
            if (string.IsNullOrWhiteSpace(localYaml))
                return (localYaml, null);

            try
            {
                var dict = Deserializer.Deserialize<Dictionary<string, object>>(localYaml) ?? new Dictionary<string, object>();
                string? stripped = null;

                if (dict.TryGetValue("shellPath", out var val) && val != null)
                {
                    stripped = val.ToString();
                    if (!string.IsNullOrEmpty(stripped))
                        SaveCache(stripped);

                    dict.Remove("shellPath");
                }

                var cleanYaml = Serializer.Serialize(dict);
                return (cleanYaml, stripped);
            }
            catch
            {
                // Fallback regex strip if YAML parsing fails
                var match = Regex.Match(localYaml, @"^shellPath:\s*(.+)$", RegexOptions.Multiline);
                string? stripped = null;
                if (match.Success)
                {
                    stripped = match.Groups[1].Value.Trim('\'', '\"', ' ');
                    SaveCache(stripped);
                    localYaml = Regex.Replace(localYaml, @"^shellPath:\s*.+(\r?\n|$)", "", RegexOptions.Multiline);
                }
                return (localYaml, stripped);
            }
        }

        /// <summary>
        /// Processes config after DOWNLOAD from Linux server:
        /// Injects local Windows 'shellPath' into the pulled config so local OMP continues to work seamlessly.
        /// </summary>
        public static (string MergedYaml, string InjectedPath) RestoreAfterDownload(string remoteYaml, string? currentLocalYaml = null)
        {
            var bashPath = DetectLocalBashPath(currentLocalYaml ?? "");
            if (string.IsNullOrEmpty(bashPath))
            {
                bashPath = @"C:\Users\" + Environment.UserName + @"\AppData\Local\UGit\app-5.48.0\resources\app\git\usr\bin\bash.exe";
            }

            try
            {
                var dict = Deserializer.Deserialize<Dictionary<string, object>>(remoteYaml) ?? new Dictionary<string, object>();
                dict["shellPath"] = bashPath;
                var mergedYaml = Serializer.Serialize(dict);
                return (mergedYaml, bashPath);
            }
            catch
            {
                var mergedYaml = $"shellPath: {bashPath}\n" + remoteYaml;
                return (mergedYaml, bashPath);
            }
        }
    }
}
