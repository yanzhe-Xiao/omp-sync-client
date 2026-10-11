using System;
using System.Collections.Generic;
using System.IO;
using System.IO.Compression;
using System.Net.Http;
using System.Text;
using System.Text.Json;
using System.Threading.Tasks;

namespace OmpSyncClient
{
    public class SkillDto
    {
        public string Name { get; set; } = "";
        public string Description { get; set; } = "";
        public int FileCount { get; set; }
        public double SizeKb { get; set; }
    }

    public class SyncApiClient
    {
        private readonly HttpClient _http;
        public string BaseUrl { get; set; }

        public SyncApiClient(string baseUrl = "http://10.69.211.84:8900")
        {
            BaseUrl = baseUrl.TrimEnd('/');
            _http = new HttpClient { Timeout = TimeSpan.FromSeconds(30) };
        }

        public async Task<(bool Success, string Message)> TestConnectionAsync()
        {
            try
            {
                var resp = await _http.GetAsync($"{BaseUrl}/api/status");
                if (resp.IsSuccessStatusCode)
                {
                    return (true, "连接正常 (HTTP 200)");
                }
                return (false, $"HTTP 错误: {(int)resp.StatusCode} {resp.ReasonPhrase}");
            }
            catch (Exception ex)
            {
                return (false, $"无法连接至 {BaseUrl}: {ex.Message}");
            }
        }

        public async Task<string> GetRemoteFileAsync(string fileKey)
        {
            var resp = await _http.GetAsync($"{BaseUrl}/api/files/{Uri.EscapeDataString(fileKey)}");
            resp.EnsureSuccessStatusCode();
            var json = await resp.Content.ReadAsStringAsync();
            using var doc = JsonDocument.Parse(json);
            return doc.RootElement.GetProperty("content").GetString() ?? "";
        }

        public async Task<bool> SaveRemoteFileAsync(string fileKey, string content, bool syncNow = true)
        {
            var payload = new { content, sync_now = syncNow };
            var jsonStr = JsonSerializer.Serialize(payload);
            var contentHttp = new StringContent(jsonStr, Encoding.UTF8, "application/json");

            var resp = await _http.PostAsync($"{BaseUrl}/api/files/{Uri.EscapeDataString(fileKey)}", contentHttp);
            if (!resp.IsSuccessStatusCode)
            {
                var err = await resp.Content.ReadAsStringAsync();
                throw new HttpRequestException($"保存失败 ({resp.StatusCode}): {err}");
            }
            return true;
        }

        public async Task<List<SkillDto>> GetRemoteSkillsAsync()
        {
            var resp = await _http.GetAsync($"{BaseUrl}/api/skills");
            resp.EnsureSuccessStatusCode();
            var json = await resp.Content.ReadAsStringAsync();
            using var doc = JsonDocument.Parse(json);
            var list = new List<SkillDto>();

            if (doc.RootElement.TryGetProperty("skills", out var skillsElem))
            {
                foreach (var s in skillsElem.EnumerateArray())
                {
                    list.Add(new SkillDto
                    {
                        Name = s.GetProperty("name").GetString() ?? "",
                        Description = s.TryGetProperty("description", out var d) ? d.GetString() ?? "" : "",
                        FileCount = s.TryGetProperty("file_count", out var fc) ? fc.GetInt32() : 0,
                        SizeKb = s.TryGetProperty("size_kb", out var sz) ? sz.GetDouble() : 0,
                    });
                }
            }
            return list;
        }

        public async Task DownloadSkillAsync(string skillName, string targetSkillsDir)
        {
            var url = $"{BaseUrl}/api/skills/{Uri.EscapeDataString(skillName)}/download";
            var resp = await _http.GetAsync(url, HttpCompletionOption.ResponseHeadersRead);
            resp.EnsureSuccessStatusCode();

            var tempZip = Path.Combine(Path.GetTempPath(), $"dl_{skillName}_{Guid.NewGuid():N}.zip");
            try
            {
                using (var fs = new FileStream(tempZip, FileMode.Create, FileAccess.Write, FileShare.None))
                {
                    await resp.Content.CopyToAsync(fs);
                }

                var targetDir = Path.Combine(targetSkillsDir, skillName);
                if (Directory.Exists(targetDir))
                {
                    Directory.Delete(targetDir, true);
                }
                Directory.CreateDirectory(targetDir);

                ZipFile.ExtractToDirectory(tempZip, targetDir, overwriteFiles: true);
            }
            finally
            {
                if (File.Exists(tempZip))
                {
                    try { File.Delete(tempZip); } catch { }
                }
            }
        }

        public async Task<string> UploadSkillDirectoryAsync(string skillName, string localSkillDir, bool syncNow = true)
        {
            var tempZip = Path.Combine(Path.GetTempPath(), $"up_{skillName}_{Guid.NewGuid():N}.zip");
            try
            {
                if (File.Exists(tempZip)) File.Delete(tempZip);
                ZipFile.CreateFromDirectory(localSkillDir, tempZip, CompressionLevel.Optimal, false);

                using var form = new MultipartFormDataContent();
                var fileStream = File.OpenRead(tempZip);
                form.Add(new StreamContent(fileStream), "file", $"{skillName}.zip");
                form.Add(new StringContent(syncNow ? "true" : "false"), "sync_now");

                var resp = await _http.PostAsync($"{BaseUrl}/api/skills/upload", form);
                if (!resp.IsSuccessStatusCode)
                {
                    var err = await resp.Content.ReadAsStringAsync();
                    throw new HttpRequestException($"上传失败: {err}");
                }
                return skillName;
            }
            finally
            {
                if (File.Exists(tempZip))
                {
                    try { File.Delete(tempZip); } catch { }
                }
            }
        }
    }
}
