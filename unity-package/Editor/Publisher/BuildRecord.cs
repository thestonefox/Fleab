using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;

namespace Fleab.Publisher
{
    [Serializable]
    public sealed class BuildRecord
    {
        public string product;
        public string version;
        public string timestamp;
        public string platform;
        public string sessionGuid;
        public bool server;
        public string root;
        public string[] entries;

        public string ArchiveName => NameToken(product) + "_" + NameToken(version) + "_" + timestamp + "_" + platform + (server ? "_Server" : "") + ".tar.zst";

        public static string NameToken(string text)
        {
            if (string.IsNullOrWhiteSpace(text)) text = "Unnamed";
            var output = new StringBuilder();
            foreach (byte b in Encoding.UTF8.GetBytes(text))
            {
                bool safe = b >= 'a' && b <= 'z' || b >= 'A' && b <= 'Z' || b >= '0' && b <= '9' || b == '-' || b == '.';
                if (safe && text != "." && text != "..") output.Append((char)b);
                else if (!char.IsControl((char)b) || b >= 128) output.Append('%').Append(b.ToString("X2"));
                else output.Append("%2D");
            }
            return output.ToString();
        }

        // File list comes from BuildReport.GetFiles, supplemented only with
        // known Unity structures required by incremental/script-only builds.
        public static BuildRecord Discover(string output, IEnumerable<string> reportedFiles, string product, string version, string platform, bool server, DateTime utc)
        {
            output = Path.GetFullPath(output);
            if (platform.StartsWith("macOS", StringComparison.Ordinal) && Directory.Exists(output) && !output.EndsWith(".app", StringComparison.OrdinalIgnoreCase))
            {
                string[] bundles = Directory.GetDirectories(output, "*.app");
                if (bundles.Length != 1) throw new IOException("Build output directory has no unique macOS application bundle.");
                output = bundles[0];
            }
            bool bundle = output.EndsWith(".app", StringComparison.OrdinalIgnoreCase) && Directory.Exists(output);
            string root = bundle || File.Exists(output) ? Path.GetDirectoryName(output) : output;
            if (!Directory.Exists(root)) throw new DirectoryNotFoundException("Unity build output is missing: " + root);
            var entries = new HashSet<string>(StringComparer.Ordinal);
            Action<string> add = path =>
            {
                path = Path.GetFullPath(path);
                if (!File.Exists(path) && !Directory.Exists(path)) return;
                string relative = Relative(root, path);
                if (relative.Length > 0) entries.Add(relative);
            };
            if (bundle) add(output);
            else
            {
                if (File.Exists(output)) add(output);
                foreach (string reported in reportedFiles ?? Array.Empty<string>())
                {
                    string path = Path.GetFullPath(reported);
                    if (IsInside(root, path) && (File.Exists(path) || Directory.Exists(path))) add(path);
                }
                string stem = Path.GetFileNameWithoutExtension(output);
                if (Directory.Exists(output))
                {
                    var executables = Directory.GetFiles(output).Where(p =>
                    {
                        if (!Directory.Exists(Path.Combine(root, Path.GetFileNameWithoutExtension(p) + "_Data"))) return false;
                        if (platform.StartsWith("Windows", StringComparison.Ordinal)) return p.EndsWith(".exe", StringComparison.OrdinalIgnoreCase);
                        using (var file = File.OpenRead(p)) return file.ReadByte() == 127 && file.ReadByte() == 'E' && file.ReadByte() == 'L' && file.ReadByte() == 'F';
                    }).ToArray();
                    if (executables.Length != 1) throw new IOException("Build report output directory has no unique Unity executable.");
                    add(executables[0]); stem = Path.GetFileNameWithoutExtension(executables[0]);
                }
                foreach (string name in new[] { stem + "_Data", "MonoBleedingEdge", "UnityPlayer.dll", "UnityPlayer.so", "GameAssembly.dll", "GameAssembly.so", "baselib.dll" }) add(Path.Combine(root, name));
            }
            if (entries.Count == 0) throw new IOException("No surviving Unity build files were found.");
            // Keep outer directories only; avoid duplicating report entries.
            string[] roots = entries.Where(p => !entries.Any(other => other != p && p.StartsWith(other + "/", StringComparison.Ordinal))).OrderBy(p => p, StringComparer.Ordinal).ToArray();
            return new BuildRecord { product = product, version = version, timestamp = utc.ToUniversalTime().ToString("yyyyMMdd-HHmmss"), platform = platform, server = server, root = root, entries = roots };
        }

        public static bool IsInside(string root, string path)
        {
            var comparison = Path.DirectorySeparatorChar == '\\' ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal;
            return Path.GetFullPath(path).StartsWith(Path.GetFullPath(root).TrimEnd(Path.DirectorySeparatorChar) + Path.DirectorySeparatorChar, comparison);
        }
        public static string Relative(string root, string path)
        {
            if (!IsInside(root, path)) throw new IOException("Build file is outside output directory: " + path);
            return path.Substring(Path.GetFullPath(root).TrimEnd(Path.DirectorySeparatorChar).Length + 1).Replace('\\', '/');
        }
    }
}
