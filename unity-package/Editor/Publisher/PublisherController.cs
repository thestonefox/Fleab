using System;
using System.IO;
using System.Threading.Tasks;
using UnityEditor;
using UnityEditor.Build;
using UnityEditor.Build.Reporting;
using UnityEngine;

namespace Fleab.Publisher
{
    internal sealed class SuccessfulBuildHook : IPostprocessBuildWithReport
    {
        public int callbackOrder => int.MaxValue;
        public void OnPostprocessBuild(BuildReport report)
        {
            if (report.summary.result != BuildResult.Succeeded) return;
            try
            {
                var record = Capture(report);
                if (record == null) return;
                PublisherController.Remember(record);
                // Capture Unity data on the build callback's main thread, then
                // start the filesystem-only worker immediately. delayCall can
                // wait for editor focus before starting any publishing work.
                if (PublisherSettings.instance.automaticPublishing) PublisherController.Start(record);
            }
            catch (Exception error) { Debug.LogError("Fleab: Unity built successfully, but publishing could not start. Original build preserved. " + error); }
        }
        internal static BuildRecord Capture(BuildReport report)
        {
            if (report == null || report.summary.result != BuildResult.Succeeded) return null;
#if UNITY_6000_6_OR_NEWER
            if (report.summary.buildType != BuildType.Player) return null;
#endif
            string os;
            switch (report.summary.platform)
            {
                case BuildTarget.StandaloneWindows64: os = "windows"; break;
                case BuildTarget.StandaloneLinux64: os = "linux"; break;
                case BuildTarget.StandaloneOSX: os = "macos"; break;
                default: return null;
            }
            var settings = PublisherSettings.instance;
            string name = string.IsNullOrWhiteSpace(settings.buildNameOverride) ? PlayerSettings.productName : settings.buildNameOverride;
            var files = Array.ConvertAll(report.GetFiles(), f => f.path);
            var record = BuildRecord.Discover(report.summary.outputPath, files, name, PlayerSettings.bundleVersion,
                BuildArchitecture.Detect(report.summary.outputPath, os), report.summary.GetSubtarget<StandaloneBuildSubtarget>() == StandaloneBuildSubtarget.Server, report.summary.buildStartedAt);
#if UNITY_6000_6_OR_NEWER
            record.sessionGuid = report.summary.buildSessionGuid.ToString();
#endif
            return record;
        }
    }

    [InitializeOnLoad]
    internal static class PublisherController
    {
        static readonly string RecordPath = Path.GetFullPath("Library/Fleab/last-build.json");
        static Task<string> task;
        static readonly object Gate = new object();
        static string phase;
        static float fraction;
        public static bool Busy => task != null;
        static PublisherController() { EditorApplication.quitting += OnQuit; }
        static void OnQuit() { if (Busy) Debug.LogWarning("Fleab publishing was interrupted. Use Publish Last Build after reopening the project."); }

        public static void Remember(BuildRecord record)
        {
            Directory.CreateDirectory(Path.GetDirectoryName(RecordPath));
            string tmp = RecordPath + ".tmp";
            File.WriteAllText(tmp, JsonUtility.ToJson(record, true));
            if (File.Exists(RecordPath)) File.Replace(tmp, RecordPath, null); else File.Move(tmp, RecordPath);
        }

        [MenuItem("Tools/Fleab/Publish Last Build")]
        public static void PublishLast()
        {
            try
            {
                Start(LastRecord());
            }
            catch (Exception error) { if (Application.isBatchMode) Debug.LogException(error); else EditorUtility.DisplayDialog("Fleab Publisher", error.Message, "OK"); }
        }
        internal static BuildRecord LastRecord()
        {
            BuildRecord saved = File.Exists(RecordPath) ? JsonUtility.FromJson<BuildRecord>(File.ReadAllText(RecordPath)) : null;
#if UNITY_6000_6_OR_NEWER
            BuildHistory.Refresh();
            foreach (var guid in BuildHistory.GetAllBuilds())
            {
                var report = BuildHistory.LoadBuildReport(guid);
                if (report == null || report.summary.result != BuildResult.Succeeded) continue;
                if (report.summary.buildType != BuildType.Player) continue;
                var target = report.summary.platform;
                if (target != BuildTarget.StandaloneWindows64 && target != BuildTarget.StandaloneLinux64 && target != BuildTarget.StandaloneOSX) continue;
                if (saved != null && saved.sessionGuid == guid.ToString()) return saved;
                var record = SuccessfulBuildHook.Capture(report);
                if (record != null) { Remember(record); return record; }
            }
#endif
            if (saved == null) throw new IOException("No successful supported build is recorded. Build a desktop player once, then publish again.");
            return saved;
        }
        [MenuItem("Tools/Fleab/Retry Publish")]
        static void Retry() => PublishLast();

        public static void Start(BuildRecord record)
        {
            if (Busy) { Debug.LogWarning("Fleab is already publishing a build. Retry when it finishes."); return; }
            string destination = PublisherSettings.instance.publishingDirectory;
            phase = "Packaging"; fraction = 0;
            EditorApplication.LockReloadAssemblies();
            task = Task.Run(() => ArchivePublisher.Publish(record, destination, (p, f) => { lock (Gate) { phase = p; fraction = f; } }));
            EditorApplication.update += Poll;
        }
        static void Poll()
        {
            if (!Application.isBatchMode) { lock (Gate) EditorUtility.DisplayProgressBar("Fleab Publisher", phase, fraction); }
            if (!task.IsCompleted) return;
            try
            {
                string path = task.GetAwaiter().GetResult();
                Debug.Log("Fleab published: " + path);
                if (SceneView.lastActiveSceneView != null) SceneView.lastActiveSceneView.ShowNotification(new GUIContent("Fleab build published"));
            }
            catch (Exception error)
            {
                Debug.LogError("Fleab publishing failed. Unity output preserved. Retry with Tools > Fleab > Retry Publish.\n" + error);
                if (!Application.isBatchMode) EditorUtility.DisplayDialog("Fleab publishing failed", error.Message + "\n\nOriginal build preserved. Use Tools > Fleab > Retry Publish.", "OK");
            }
            finally { task = null; EditorUtility.ClearProgressBar(); EditorApplication.update -= Poll; EditorApplication.UnlockReloadAssemblies(); }
        }
    }
}
