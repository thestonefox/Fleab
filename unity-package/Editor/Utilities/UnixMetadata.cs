using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

namespace Fleab.Publisher
{
    internal static class UnixMetadata
    {
        [DllImport("libc", SetLastError = true, EntryPoint = "readlink")]
        static extern IntPtr ReadLink(string path, byte[] buffer, UIntPtr size);
        [DllImport("libc", SetLastError = true, EntryPoint = "lstat")]
        static extern int LStat(string path, IntPtr buffer);
        [DllImport("libc", SetLastError = true, EntryPoint = "lstat$INODE64")]
        static extern int LStatDarwinInode64(string path, IntPtr buffer);
        [DllImport("libc", SetLastError = true, EntryPoint = "__lxstat")]
        static extern int LStatLegacyLinux(int version, string path, IntPtr buffer);
        static bool legacyLinuxStat;

        static int Stat(string path, IntPtr buffer, bool mac)
        {
            if (mac && RuntimeInformation.ProcessArchitecture == Architecture.X64)
                return LStatDarwinInode64(path, buffer);
            if (legacyLinuxStat) return LStatLegacyLinux(1, path, buffer);
            try { return LStat(path, buffer); }
            catch (EntryPointNotFoundException) when (RuntimeInformation.IsOSPlatform(OSPlatform.Linux) && RuntimeInformation.ProcessArchitecture == Architecture.X64)
            {
                // glibc before 2.33 exposes this through its versioned wrapper.
                // Version 1 is the Linux x64 stat ABI, with mode at offset 24.
                legacyLinuxStat = true;
                return LStatLegacyLinux(1, path, buffer);
            }
        }

        public static string Link(string path)
        {
            if (Path.DirectorySeparatorChar == '\\')
            {
                if ((File.GetAttributes(path) & FileAttributes.ReparsePoint) != 0)
                    throw new IOException("Publishing Windows reparse points is unsupported; build into a regular directory.");
                return null;
            }
            var buffer = new byte[4096];
            long n = ReadLink(path, buffer, (UIntPtr)buffer.Length).ToInt64();
            if (n < 0) return null;
            if (n >= buffer.Length) throw new IOException("Symbolic link target too long.");
            return Encoding.UTF8.GetString(buffer, 0, (int)n);
        }

        public static int Mode(string path, bool directory, string relative, string platform)
        {
            if (Path.DirectorySeparatorChar != '\\')
            {
                IntPtr buffer = Marshal.AllocHGlobal(512);
                try
                {
                    bool mac = RuntimeInformation.IsOSPlatform(OSPlatform.OSX);
                    // Intel Darwin exports the modern stat layout through a
                    // suffixed symbol. Plain lstat uses its legacy layout.
                    // ARM64 Darwin exposes only the modern, unsuffixed ABI.
                    int result = Stat(path, buffer, mac);
                    if (result != 0) throw new IOException("Cannot inspect Unix mode: " + path);
                    // Darwin stat uses a 16-bit mode at offset 4. Linux x64
                    // uses offset 24; Linux ARM64 uses offset 16.
                    int offset = mac ? 4 : RuntimeInformation.ProcessArchitecture == Architecture.Arm64 ? 16 : 24;
                    int mode = (mac ? Marshal.ReadInt16(buffer, offset) : Marshal.ReadInt32(buffer, offset)) & 511;
                    return directory ? mode | 448 : mode;
                }
                finally { Marshal.FreeHGlobal(buffer); }
            }
            if (directory) return 493; // 0755
            bool executable = relative.Contains(".app/Contents/MacOS/") || relative.EndsWith(".so", StringComparison.OrdinalIgnoreCase);
            if (platform.StartsWith("Linux", StringComparison.Ordinal) && !relative.Contains("/"))
            {
                using (var file = File.OpenRead(path))
                { executable |= file.ReadByte() == 127 && file.ReadByte() == 'E' && file.ReadByte() == 'L' && file.ReadByte() == 'F'; }
            }
            return executable ? 493 : 420; // 0755 or 0644
        }
    }
}
