using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using ZstdSharp;

namespace Fleab.Publisher
{
    public static class TarZstd
    {
        sealed class Entry { public string Name; public string Full; public string Link; public bool Directory; public int Mode; }
        static readonly byte[] Zero = new byte[512];

        public static void ValidateName(string name)
        {
            if (string.IsNullOrEmpty(name) || Encoding.UTF8.GetByteCount(name) > 4096) throw new IOException("Invalid archive path.");
            foreach (string part in name.Split('/'))
            {
                if (part.Length == 0 || part == "." || part == ".." || part.EndsWith(".", StringComparison.Ordinal) || part.EndsWith(" ", StringComparison.Ordinal) || part.Any(c => char.IsControl(c) || "\\:<>\"|?*".Contains(c)) || part.StartsWith(".fleab-", StringComparison.Ordinal)) throw new IOException("Unsafe archive path: " + name);
                string device = part.Split('.')[0].ToUpperInvariant();
                if (new[] { "CON", "PRN", "AUX", "NUL" }.Contains(device) || device.Length == 4 && (device.StartsWith("COM", StringComparison.Ordinal) || device.StartsWith("LPT", StringComparison.Ordinal)) && device[3] >= '1' && device[3] <= '9') throw new IOException("Reserved filename: " + name);
            }
        }

        static List<Entry> Enumerate(BuildRecord record)
        {
            var entries = new List<Entry>(); var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            Action<string> walk = null;
            walk = relative =>
            {
                ValidateName(relative);
                string parent = record.root;
                var components = relative.Split('/');
                for (int i = 0; i < components.Length - 1; i++)
                {
                    parent = Path.Combine(parent, components[i]);
                    if (UnixMetadata.Link(parent) != null) throw new IOException("Archive entry has a symbolic link parent: " + relative);
                }
                if (!seen.Add(relative)) throw new IOException("Duplicate or case-colliding build entry: " + relative);
                string full = Path.Combine(record.root, relative.Replace('/', Path.DirectorySeparatorChar));
                if (!File.Exists(full) && !Directory.Exists(full)) throw new FileNotFoundException("Last build file is missing; rebuild or restore the output.", full);
                string link = UnixMetadata.Link(full);
                bool directory = link == null && Directory.Exists(full);
                entries.Add(new Entry { Name = relative, Full = full, Link = link, Directory = directory, Mode = UnixMetadata.Mode(full, directory, relative, record.platform) });
                if (directory)
                    foreach (string child in Directory.GetFileSystemEntries(full).OrderBy(p => p, StringComparer.Ordinal)) walk(relative + "/" + Path.GetFileName(child));
            };
            foreach (string entry in record.entries) walk(entry);
            var names = entries.ToDictionary(e => e.Name, StringComparer.Ordinal);
            foreach (Entry entry in entries.Where(e => e.Link != null))
            {
                string resolved = ResolveLinks(NormalizeLink(entry.Name, entry.Link), names, 0);
                if (!names.TryGetValue(resolved, out Entry target) || target.Link != null) throw new IOException("Symbolic link must resolve to an existing entry: " + entry.Name);
            }
            return entries;
        }

        static string NormalizeLink(string name, string link)
        {
            if (string.IsNullOrEmpty(link) || link.StartsWith("/", StringComparison.Ordinal) || link.Contains('\\') || link.Contains(':')) throw new IOException("Unsafe symbolic link: " + name);
            foreach (string part in link.Split('/'))
            {
                if (part != "" && part != "." && part != "..") ValidateName(part);
            }
            string parent = string.Join("/", name.Split('/').Take(name.Split('/').Length - 1));
            return parent.Length == 0 ? link : parent + "/" + link;
        }
        static string ResolveLinks(string path, Dictionary<string, Entry> names, int depth)
        {
            if (depth >= 40) throw new IOException("Symbolic link cycle or excessive chain.");
            string[] parts = path.Split('/');
            var resolved = new List<string>();
            for (int i = 0; i < parts.Length; i++)
            {
                if (parts[i] == "" || parts[i] == ".") continue;
                if (parts[i] == "..")
                {
                    if (resolved.Count == 0) throw new IOException("Symbolic link escapes archive.");
                    resolved.RemoveAt(resolved.Count-1); continue;
                }
                resolved.Add(parts[i]);
                string prefix = string.Join("/", resolved);
                if (names.TryGetValue(prefix, out Entry entry) && entry.Link != null)
                {
                    string substituted = NormalizeLink(entry.Name, entry.Link);
                    if (i+1 < parts.Length) substituted += "/" + string.Join("/", parts.Skip(i+1));
                    return ResolveLinks(substituted, names, depth + 1);
                }
                if (i+1 < parts.Length && (!names.TryGetValue(prefix, out Entry directory) || !directory.Directory) && !names.Keys.Any(n => n.StartsWith(prefix + "/", StringComparison.Ordinal)))
                    throw new IOException("Symbolic link traverses a missing or non-directory entry.");
            }
            if (resolved.Count == 0) throw new IOException("Symbolic link targets archive root.");
            return string.Join("/", resolved);
        }

        public static void Create(BuildRecord record, string archive, Action<string, float> progress = null)
        {
            progress?.Invoke("Packaging", 0);
            List<Entry> entries = Enumerate(record);
            using (var output = new FileStream(archive, FileMode.CreateNew, FileAccess.Write, FileShare.None))
            {
                using (var zstd = new CompressionStream(output, 3, leaveOpen: true))
                {
                    zstd.SetParameter(ZstdSharp.Unsafe.ZSTD_cParameter.ZSTD_c_checksumFlag, 1);
                    int index = 0;
                    foreach (Entry e in entries)
                    {
                        progress?.Invoke("Compression", (float)index++ / entries.Count);
                        long size = e.Directory || e.Link != null ? 0 : new FileInfo(e.Full).Length;
                        var pax = new StringBuilder();
                        if (size > 8589934591L) Pax(pax, "size", size.ToString(CultureInfo.InvariantCulture));
                        if (Encoding.UTF8.GetByteCount(e.Name) > 100 || e.Name.Any(c => c > 127)) Pax(pax, "path", e.Name);
                        if (e.Link != null && (Encoding.UTF8.GetByteCount(e.Link) > 100 || e.Link.Any(c => c > 127))) Pax(pax, "linkpath", e.Link);
                        if (pax.Length > 0)
                        {
                            byte[] data = Encoding.UTF8.GetBytes(pax.ToString());
                            Header(zstd, "PaxHeader", null, 420, data.Length, 'x'); zstd.Write(data, 0, data.Length); Pad(zstd, data.Length);
                        }
                        Header(zstd, Encoding.UTF8.GetByteCount(e.Name) <= 100 ? e.Name : "PaxEntry", e.Link != null && Encoding.UTF8.GetByteCount(e.Link) <= 100 ? e.Link : null, e.Mode, size > 8589934591L ? 0 : size, e.Link != null ? '2' : e.Directory ? '5' : '0');
                        if (!e.Directory && e.Link == null)
                        {
                            using (var input = new FileStream(e.Full, FileMode.Open, FileAccess.Read, FileShare.Read))
                            {
                                long written = 0; var buffer = new byte[131072]; int n;
                                while ((n = input.Read(buffer, 0, buffer.Length)) > 0) { zstd.Write(buffer, 0, n); written += n; }
                                if (written != size) throw new IOException("Build file changed during compression: " + e.Full);
                            }
                            Pad(zstd, size);
                        }
                    }
                    zstd.Write(Zero, 0, 512); zstd.Write(Zero, 0, 512);
                }
                output.Flush(true);
            }
        }
        static void Pax(StringBuilder b, string key, string value)
        {
            string body = " " + key + "=" + value + "\n"; int n = Encoding.UTF8.GetByteCount(body) + 1;
            while (n != Encoding.UTF8.GetByteCount(body) + n.ToString(CultureInfo.InvariantCulture).Length) n = Encoding.UTF8.GetByteCount(body) + n.ToString(CultureInfo.InvariantCulture).Length;
            b.Append(n.ToString(CultureInfo.InvariantCulture)).Append(body);
        }
        static void Header(Stream stream, string name, string link, int mode, long size, char type)
        {
            var header = new byte[512];
            Text(header, 0, 100, name); Octal(header, 100, 8, mode); Octal(header, 108, 8, 0); Octal(header, 116, 8, 0); Octal(header, 124, 12, size); Octal(header, 136, 12, 0);
            for (int i = 148; i < 156; i++) header[i] = 32;
            header[156] = (byte)type; if (link != null) Text(header, 157, 100, link);
            Text(header, 257, 6, "ustar"); Text(header, 263, 2, "00");
            Octal(header, 148, 8, header.Sum(v => (int)v)); stream.Write(header, 0, header.Length);
        }
        static void Text(byte[] header, int offset, int length, string value)
        { byte[] bytes = Encoding.UTF8.GetBytes(value); if (bytes.Length > length) throw new IOException("TAR field too long."); Array.Copy(bytes, 0, header, offset, bytes.Length); }
        static void Octal(byte[] header, int offset, int length, long value)
        { Text(header, offset, length, Convert.ToString(value, 8).PadLeft(length - 1, '0') + "\0"); }
        static void Pad(Stream stream, long length) { int n = (int)((512 - length % 512) % 512); stream.Write(Zero, 0, n); }

        public static void Validate(string archive)
        {
            // Full decompression checks truncation and zstd errors. Also inspect
            // each TAR header/checksum and ensure its payload is present.
            using (var input = File.OpenRead(archive))
            using (var stream = new DecompressionStream(input))
            {
                var header = new byte[512]; var buffer = new byte[131072];
                long? paxSize = null;
                while (true)
                {
                    ReadExact(stream, header, 512);
                    if (header.All(b => b == 0)) { ReadExact(stream, header, 512); if (header.Any(b => b != 0)) throw new IOException("Invalid TAR ending."); break; }
                    int expected = Convert.ToInt32(Encoding.ASCII.GetString(header, 148, 8).Trim('\0', ' '), 8);
                    int sum = 0; for (int i = 0; i < 512; i++) sum += i >= 148 && i < 156 ? 32 : header[i];
                    if (sum != expected) throw new IOException("Invalid TAR checksum.");
                    long size = Convert.ToInt64(Encoding.ASCII.GetString(header, 124, 12).Trim('\0', ' '), 8);
                    if (header[156] == (byte)'x')
                    {
                        if (size > 65536) throw new IOException("PAX metadata too large.");
                        var data = new byte[(int)size]; ReadExact(stream, data, data.Length);
                        int offset = 0;
                        while (offset < data.Length)
                        {
                            int space = Array.IndexOf(data, (byte)' ', offset);
                            if (space < 0) throw new IOException("Invalid PAX record.");
                            int length = int.Parse(Encoding.ASCII.GetString(data, offset, space - offset), CultureInfo.InvariantCulture);
                            if (length <= space-offset+1 || offset+length > data.Length) throw new IOException("Invalid PAX record length.");
                            string field = Encoding.UTF8.GetString(data, space+1, offset+length-space-2);
                            if (field.StartsWith("size=", StringComparison.Ordinal)) paxSize = long.Parse(field.Substring(5), CultureInfo.InvariantCulture);
                            offset += length;
                        }
                        ReadExact(stream, buffer, (int)((512-size%512)%512));
                        continue;
                    }
                    if (paxSize.HasValue) { size = paxSize.Value; paxSize = null; }
                    long remaining = size + (512 - size % 512) % 512;
                    while (remaining > 0) { int n = (int)Math.Min(buffer.Length, remaining); ReadExact(stream, buffer, n); remaining -= n; }
                }
                int read; while ((read = stream.Read(buffer, 0, buffer.Length)) > 0) if (buffer.Take(read).Any(b => b != 0)) throw new IOException("Unexpected archive trailer.");
            }
        }
        static void ReadExact(Stream stream, byte[] bytes, int length)
        { int done = 0; while (done < length) { int n = stream.Read(bytes, done, length - done); if (n == 0) throw new EndOfStreamException("Truncated archive."); done += n; } }
    }
}
