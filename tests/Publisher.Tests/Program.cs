using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Fleab.Publisher;

string root = Path.Combine(Path.GetTempPath(), "fleab-tests-" + Guid.NewGuid().ToString("N"));
Directory.CreateDirectory(root);
int passed = 0;
void Check(bool condition, string message) { if (!condition) throw new Exception(message); passed++; Console.WriteLine("PASS " + message); }
void Fails(Action action, string message) { try { action(); } catch { Check(true, message); return; } throw new Exception(message); }
try
{
    Check(BuildRecord.NameToken("My_Game") == "My%5FGame", "Unambiguous underscores");
    Check(BuildRecord.NameToken("a/b: ü") == "a%2Fb%3A%20%C3%BC", "UTF-8 and filename character escaping");
    string output = Path.Combine(root, "output"); Directory.CreateDirectory(output);
    string exe = Path.Combine(output, "Game.exe"); File.WriteAllBytes(exe, new byte[] { 77, 90, 0, 0 });
    Directory.CreateDirectory(Path.Combine(output, "Game_Data")); File.WriteAllText(Path.Combine(output, "Game_Data", "data"), "Unity test data");
    string longName = new string('x', 110) + "ü.txt"; File.WriteAllText(Path.Combine(output, "Game_Data", longName), "long UTF-8 path");
    File.WriteAllText(Path.Combine(output, "unrelated.txt"), "must not publish");
    var record = BuildRecord.Discover(exe, new[] { exe }, "Interop_Game", "1.2_β", "Windows64", false, new DateTime(2026,11,8,9,30,0, DateTimeKind.Utc));
    Check(record.entries.Length == 2 && !record.entries.Contains("unrelated.txt"), "Only reported files and known Unity structures included");
    Check(record.ArchiveName == "Interop%5FGame_1.2%5F%CE%B2_20261108-093000_Windows64.tar.zst", "Shared archive grammar");
    string publish = Path.Combine(root, "published");
    string first = ArchivePublisher.Publish(record, publish);
    string second = ArchivePublisher.Publish(record, publish);
    Check(first != second && File.Exists(first) && File.Exists(second), "Same-version collision never overwrites");
    Check(Path.GetFileName(second).Contains('~'), "Collision suffix is explicit");
    TarZstd.Validate(first); Check(true, "Managed zstd and TAR validation round trip");
    string fixtures = args.Length > 0 ? args[0] : null;
    if (fixtures != null) { Directory.CreateDirectory(fixtures); File.Copy(first, Path.Combine(fixtures, record.ArchiveName), true); }
    string bad = Path.Combine(root, "corrupt.zst"); File.WriteAllBytes(bad, File.ReadAllBytes(first).Take(20).ToArray());
    Fails(() => TarZstd.Validate(bad), "Truncated compressed archive rejected");
    Fails(() => ArchivePublisher.Publish(record, output), "Publishing inside build output rejected");
    Check(File.Exists(exe) && File.Exists(Path.Combine(output, "unrelated.txt")), "Publishing failure preserves original output");
    Fails(() => TarZstd.ValidateName("../escape"), "Traversal rejected");
    Fails(() => TarZstd.ValidateName("CON.txt"), "Windows device names rejected");
    Fails(() => TarZstd.ValidateName("foo\\bar"), "Backslash archive path rejected");
    File.Delete(exe);
    Fails(() => ArchivePublisher.Publish(record, publish), "Missing last build gives useful failure");
    File.WriteAllBytes(exe, new byte[] { 77, 90, 0, 0 });
    Check(File.Exists(ArchivePublisher.Publish(record, publish)), "Retry publishes without rebuilding");
    record.server = true; Check(record.ArchiveName.Contains("_Server"), "Explicit server archive suffix");
    Check(!Directory.GetFiles(publish).Any(p => p.EndsWith(".partial")), "Temporary publishing files cleaned");
    foreach (var item in new[] { ("linux", "Game.x86_64", "Linux64"), ("macos", "Game.app", "macOSARM64") })
    {
        string dir = Path.Combine(root, item.Item1); Directory.CreateDirectory(dir);
        string main = Path.Combine(dir, item.Item2);
        if (item.Item1 == "macos") { Directory.CreateDirectory(Path.Combine(main, "Contents", "MacOS")); File.WriteAllBytes(Path.Combine(main, "Contents", "MacOS", "Game"), new byte[] { 0xCF,0xFA,0xED,0xFE,12,0,0,1 }); }
        else { var bytes = new byte[20]; bytes[0]=127; bytes[1]=69; bytes[2]=76; bytes[3]=70; bytes[18]=62; File.WriteAllBytes(main, bytes); Directory.CreateDirectory(Path.Combine(dir, "Game_Data")); }
        Check(BuildArchitecture.Detect(main, item.Item1) == item.Item3, "Architecture read from " + item.Item1 + " output");
        var r = BuildRecord.Discover(main, new[] { main }, "Game", "1", item.Item3, false, DateTime.UtcNow);
        var fromDirectory = BuildRecord.Discover(dir, new[] { main }, "Game", "1", item.Item3, false, DateTime.UtcNow);
        Check(fromDirectory.entries.SequenceEqual(r.entries), item.Item1 + " directory output discovery");
        string archive = ArchivePublisher.Publish(r, publish); TarZstd.Validate(archive);
        Check(true, item.Item1 + " structure packaged");
    }
    if (!OperatingSystem.IsWindows())
    {
        string bundle = Path.Combine(root, "links", "Game.app");
        string framework = Path.Combine(bundle, "Contents", "Frameworks", "Test.framework");
        string versions = Path.Combine(framework, "Versions");
        Directory.CreateDirectory(Path.Combine(versions, "A"));
        File.WriteAllText(Path.Combine(versions, "A", "Test"), "framework payload");
        Directory.CreateSymbolicLink(Path.Combine(versions, "Current"), "A");
        File.CreateSymbolicLink(Path.Combine(framework, "Test"), "Versions/Current/Test");
        var r = BuildRecord.Discover(bundle, new[] { bundle }, "Links", "1", "macOS64", false, DateTime.UtcNow);
        string a = ArchivePublisher.Publish(r, publish); TarZstd.Validate(a);
        Check(true, "Safe macOS framework symlink chain preserved");
        File.CreateSymbolicLink(Path.Combine(framework, "escape"), "../../../../../../outside");
        Fails(() => ArchivePublisher.Publish(r, publish), "Escaping symlink rejected by publisher");
        File.Delete(Path.Combine(framework, "escape"));
        File.CreateSymbolicLink(Path.Combine(framework, "cycle1"), "cycle2");
        File.CreateSymbolicLink(Path.Combine(framework, "cycle2"), "cycle1");
        Fails(() => ArchivePublisher.Publish(r, publish), "Symlink cycle rejected by publisher");
    }
    Console.WriteLine($"{passed} publisher assertions passed.");
}
finally { Directory.Delete(root, true); }
