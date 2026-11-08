using System;
using System.IO;
using NUnit.Framework;

namespace Fleab.Publisher.Tests
{
    public sealed class PublisherTests
    {
        [TestCase("My_Game", "My%5FGame")]
        [TestCase("v1 β", "v1%20%CE%B2")]
        public void NamesAreUnambiguous(string input, string expected)
            => Assert.AreEqual(expected, BuildRecord.NameToken(input));

        [TestCase("../outside")]
        [TestCase("C:/outside")]
        [TestCase("CON")]
        public void UnsafeNamesAreRejected(string path)
            => Assert.Throws<IOException>(() => TarZstd.ValidateName(path));

        [Test]
        public void PublishingTwicePreservesOutputAndArchives()
        {
            string root = Path.Combine(Path.GetTempPath(), "fleab-" + Guid.NewGuid().ToString("N"));
            try
            {
                string build = Path.Combine(root, "build"); Directory.CreateDirectory(build);
                string exe = Path.Combine(build, "Game.exe"); File.WriteAllBytes(exe, new byte[] { 77,90,0,0 });
                Directory.CreateDirectory(Path.Combine(build, "Game_Data"));
                var record = BuildRecord.Discover(exe, new[] { exe }, "Game", "1", "Windows64", true, DateTime.UtcNow);
                string a = ArchivePublisher.Publish(record, Path.Combine(root, "published"));
                string b = ArchivePublisher.Publish(record, Path.Combine(root, "published"));
                Assert.AreNotEqual(a, b); Assert.IsTrue(File.Exists(a)); Assert.IsTrue(File.Exists(exe));
                Assert.IsTrue(Path.GetFileName(a).Contains("_Server")); TarZstd.Validate(a);
            }
            finally { if (Directory.Exists(root)) Directory.Delete(root, true); }
        }
    }
}
