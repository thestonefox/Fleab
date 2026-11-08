using System;
using System.IO;

namespace Fleab.Publisher
{
    public static class ArchivePublisher
    {
        public static string Publish(BuildRecord record, string destination, Action<string, float> progress = null)
        {
            if (record == null || record.entries == null || record.entries.Length == 0) throw new IOException("No successful supported build has been recorded.");
            if (string.IsNullOrWhiteSpace(destination) || !Path.IsPathRooted(destination)) throw new IOException("Choose an absolute publishing directory in Tools > Fleab > Settings.");
            destination = Path.GetFullPath(destination);
            if (BuildRecord.IsInside(record.root, destination) || string.Equals(record.root, destination, StringComparison.OrdinalIgnoreCase)) throw new IOException("Publishing directory must be outside the Unity build output.");
            Directory.CreateDirectory(destination);
            string local = Path.Combine(Path.GetTempPath(), "fleab-" + Guid.NewGuid().ToString("N") + ".tar.zst");
            string partial = Path.Combine(destination, ".fleab-" + Guid.NewGuid().ToString("N") + ".partial");
            try
            {
                TarZstd.Create(record, local, progress);
                progress?.Invoke("Validating", 0); TarZstd.Validate(local);
                using (var input = File.OpenRead(local))
                using (var output = new FileStream(partial, FileMode.CreateNew, FileAccess.Write, FileShare.None))
                {
                    var buffer = new byte[131072]; int n; long done = 0;
                    while ((n = input.Read(buffer, 0, buffer.Length)) > 0) { output.Write(buffer, 0, n); done += n; progress?.Invoke("Publishing", (float)done / input.Length); }
                    output.Flush(true);
                }
                TarZstd.Validate(partial);
                string final = Path.Combine(destination, record.ArchiveName);
                while (true)
                {
                    try { File.Move(partial, final); return final; }
                    catch (IOException) when (File.Exists(final)) { final = Path.Combine(destination, record.ArchiveName.Substring(0, record.ArchiveName.Length - 8) + "~" + Guid.NewGuid().ToString("N") + ".tar.zst"); }
                }
            }
            finally { if (File.Exists(local)) File.Delete(local); if (File.Exists(partial)) File.Delete(partial); }
        }
    }
}
