using System;
using System.IO;
using System.Linq;

namespace Fleab.Publisher
{
    public static class BuildArchitecture
    {
        public static string Detect(string output, string os)
        {
            if (os == "macos")
            {
                string bundle = output.EndsWith(".app", StringComparison.OrdinalIgnoreCase) ? output : Directory.GetDirectories(output, "*.app").Single();
                foreach (string file in Directory.GetFiles(Path.Combine(bundle, "Contents", "MacOS")))
                {
                    byte[] header = Read(file, 8);
                    uint magic = BitConverter.ToUInt32(header, 0);
                    if (magic == 0xBEBAFECA || magic == 0xBFBAFECA || magic == 0xCAFEBABE || magic == 0xCAFEBABF) return "macOS";
                    if (magic == 0xFEEDFACF || magic == 0xFEEDFACE)
                    {
                        uint cpu = BitConverter.ToUInt32(header, 4);
                        if (cpu == 0x0100000C) return "macOSARM64";
                        if (cpu == 0x01000007) return "macOS64";
                    }
                }
                throw new IOException("Cannot determine architecture from built Mach-O executable.");
            }
            if (Directory.Exists(output))
                output = Directory.GetFiles(output).Single(p => Directory.Exists(Path.Combine(output, Path.GetFileNameWithoutExtension(p) + "_Data")));
            using (var stream = File.OpenRead(output))
            using (var reader = new BinaryReader(stream))
            {
                if (os == "windows")
                {
                    if (reader.ReadUInt16() != 0x5A4D) throw new IOException("Invalid Windows player.");
                    stream.Position = 0x3C; uint pe = reader.ReadUInt32(); stream.Position = pe;
                    if (reader.ReadUInt32() != 0x4550) throw new IOException("Invalid PE header.");
                    ushort machine = reader.ReadUInt16();
                    if (machine == 0x8664) return "Windows64";
                    if (machine == 0xAA64) return "WindowsARM64";
                }
                if (os == "linux")
                {
                    if (reader.ReadUInt32() != 0x464C457F) throw new IOException("Invalid Linux player.");
                    stream.Position = 18; ushort machine = reader.ReadUInt16();
                    if (machine == 62) return "Linux64";
                    if (machine == 183) return "LinuxARM64";
                }
            }
            throw new IOException("Unsupported player CPU architecture.");
        }
        static byte[] Read(string file, int n)
        { var bytes = new byte[n]; using (var stream = File.OpenRead(file)) if (stream.Read(bytes, 0, n) != n) return new byte[n]; return bytes; }
    }
}
