using System;
using System.Collections;
using System.IO;
using NUnit.Framework;
using UnityEditor;
using UnityEditor.Build.Reporting;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.TestTools;

namespace Fleab.Publisher.Tests
{
    public sealed class BuildIntegrationTests
    {
        [UnityTest]
        public IEnumerator NormalBuildCallbackManualPublishAndAutomaticPublish()
        {
            if (Environment.GetEnvironmentVariable("FLEAB_BUILD_TESTS") != "1")
                Assert.Ignore("Set FLEAB_BUILD_TESTS=1 to run actual Windows player builds in an isolated project.");
            if (Application.platform != RuntimePlatform.WindowsEditor) Assert.Ignore("This smoke build targets the Windows Editor.");
            string targetName = Environment.GetEnvironmentVariable("FLEAB_BUILD_TARGET") ?? "Windows64";
            var target = targetName == "Linux64" ? BuildTarget.StandaloneLinux64 : targetName == "macOS" ? BuildTarget.StandaloneOSX : BuildTarget.StandaloneWindows64;
            string root = Path.GetFullPath("FleabAcceptance");
            string entry = targetName == "Linux64" ? "FleabTest.x86_64" : targetName == "macOS" ? "FleabTest.app" : "FleabTest.exe";
            string output = Path.Combine(root, targetName, "output", entry);
            string published = Path.Combine(root, "published");
            Directory.CreateDirectory(Path.GetDirectoryName(output)); Directory.CreateDirectory(published);
            var settings = PublisherSettings.instance;
            bool oldEnabled = settings.automaticPublishing; string oldDirectory = settings.publishingDirectory;
            string oldName = settings.buildNameOverride;
            try
            {
                settings.automaticPublishing = false; settings.publishingDirectory = published; settings.buildNameOverride = "Fleab_Test";
                var scene = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
                new GameObject("Camera", typeof(Camera));
                EditorSceneManager.SaveScene(scene, "Assets/FleabAcceptance.unity");
                var options = new BuildPlayerOptions { scenes = new[] { "Assets/FleabAcceptance.unity" }, locationPathName = output, target = target, subtarget = (int)StandaloneBuildSubtarget.Player, options = BuildOptions.Development };
                var report = BuildPipeline.BuildPlayer(options);
                Assert.AreEqual(BuildResult.Succeeded, report.summary.result);
                Assert.IsFalse(PublisherController.Busy, "Disabled publishing must not start");
                Assert.IsTrue(File.Exists("Library/Fleab/last-build.json"));
#if UNITY_6000_6_OR_NEWER
                File.Delete("Library/Fleab/last-build.json"); // Exercise BuildHistory recovery.
#endif
                int before = Directory.GetFiles(published, "*.tar.zst").Length;
                PublisherController.PublishLast();
                double deadline = EditorApplication.timeSinceStartup + 180;
                while (PublisherController.Busy && EditorApplication.timeSinceStartup < deadline) yield return null;
                Assert.IsFalse(PublisherController.Busy, "Manual publish timed out");
                Assert.AreEqual(before + 1, Directory.GetFiles(published, "*.tar.zst").Length);
                settings.automaticPublishing = true;
                report = BuildPipeline.BuildPlayer(options);
                Assert.AreEqual(BuildResult.Succeeded, report.summary.result);
                // Automatic publishing is queued until the build operation ends.
                yield return null; yield return null;
                deadline = EditorApplication.timeSinceStartup + 180;
                while (PublisherController.Busy && EditorApplication.timeSinceStartup < deadline) yield return null;
                Assert.AreEqual(before + 2, Directory.GetFiles(published, "*.tar.zst").Length);
                Assert.IsTrue(File.Exists(output) || Directory.Exists(output), "Unity output must remain intact");
                foreach (string archive in Directory.GetFiles(published, "*.tar.zst")) TarZstd.Validate(archive);
            }
            finally
            {
                settings.automaticPublishing = oldEnabled; settings.publishingDirectory = oldDirectory; settings.buildNameOverride = oldName;
            }
        }
    }
}
