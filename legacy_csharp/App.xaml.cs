using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using System.Windows;

namespace OmpSyncClient;

public partial class App : Application
{
    [DllImport("kernel32.dll", SetLastError = true, ExactSpelling = true)]
    private static extern bool AttachConsole(int dwProcessId);

    private const int ATTACH_PARENT_PROCESS = -1;

    private async void Application_Startup(object sender, StartupEventArgs e)
    {
        if (e.Args.Length > 0)
        {
            if (AttachConsole(ATTACH_PARENT_PROCESS))
            {
                var standardOutput = new StreamWriter(Console.OpenStandardOutput()) { AutoFlush = true };
                Console.SetOut(standardOutput);
                var standardError = new StreamWriter(Console.OpenStandardError()) { AutoFlush = true };
                Console.SetError(standardError);
            }
            var cmd = e.Args[0].ToLowerInvariant();
            var serverUrl = e.Args.Length > 1 ? e.Args[1] : "http://10.69.211.84:8900";
            var client = new SyncApiClient(serverUrl);
            var localAgentDir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".omp", "agent");
            var localConfigPath = Path.Combine(localAgentDir, "config.yml");
            var localModelsPath = Path.Combine(localAgentDir, "models.yml");

            Console.WriteLine($"\n[OMP Sync Client CLI] Server: {serverUrl}");

            if (cmd == "test" || cmd == "status")
            {
                var (ok, msg) = await client.TestConnectionAsync();
                Console.WriteLine($"Connection Test: {(ok ? "SUCCESS" : "FAILED")} ({msg})");
                Shutdown(ok ? 0 : 1);
                return;
            }
            else if (cmd == "pull")
            {
                try
                {
                    Console.WriteLine("Pulling config from server...");
                    var remote = await client.GetRemoteFileAsync("config.yml");
                    var localContent = File.Exists(localConfigPath) ? File.ReadAllText(localConfigPath) : "";
                    var (merged, injected) = ShellPathManager.RestoreAfterDownload(remote, localContent);

                    if (File.Exists(localConfigPath))
                    {
                        File.Copy(localConfigPath, localConfigPath + ".bak", true);
                    }

                    Directory.CreateDirectory(localAgentDir);
                    File.WriteAllText(localConfigPath, merged);
                    Console.WriteLine($"[SUCCESS] config.yml pulled! Injected local Windows shellPath: {injected}");

                    try
                    {
                        var remoteModels = await client.GetRemoteFileAsync("models.yml");
                        if (!string.IsNullOrWhiteSpace(remoteModels))
                        {
                            File.WriteAllText(localModelsPath, remoteModels);
                            Console.WriteLine("[SUCCESS] models.yml pulled!");
                        }
                    }
                    catch { }

                    Shutdown(0);
                    return;
                }
                catch (Exception ex)
                {
                    Console.WriteLine($"[ERROR] Pull failed: {ex.Message}");
                    Shutdown(1);
                    return;
                }
            }
            else if (cmd == "push")
            {
                try
                {
                    Console.WriteLine("Pushing config to server...");
                    if (!File.Exists(localConfigPath))
                    {
                        Console.WriteLine("[ERROR] Local config.yml not found.");
                        Shutdown(1);
                        return;
                    }

                    var localYaml = File.ReadAllText(localConfigPath);
                    var (cleanYaml, stripped) = ShellPathManager.SanitizeForUpload(localYaml);
                    if (!string.IsNullOrEmpty(stripped))
                    {
                        Console.WriteLine($"[SHIELD] Stripped Windows shellPath '{stripped}' to protect Linux server.");
                    }

                    await client.SaveRemoteFileAsync("config.yml", cleanYaml, syncNow: true);
                    Console.WriteLine("[SUCCESS] config.yml pushed and broadcasted to cluster!");

                    if (File.Exists(localModelsPath))
                    {
                        var modelsContent = File.ReadAllText(localModelsPath);
                        await client.SaveRemoteFileAsync("models.yml", modelsContent, syncNow: true);
                        Console.WriteLine("[SUCCESS] models.yml pushed!");
                    }

                    Shutdown(0);
                    return;
                }
                catch (Exception ex)
                {
                    Console.WriteLine($"[ERROR] Push failed: {ex.Message}");
                    Shutdown(1);
                    return;
                }
            }
            else
            {
                Console.WriteLine("Usage: OmpSyncClient.exe [pull|push|test|status] [server_url]");
                Shutdown(0);
                return;
            }
        }

        // GUI Mode
        var win = new MainWindow();
        win.Show();
    }
}
