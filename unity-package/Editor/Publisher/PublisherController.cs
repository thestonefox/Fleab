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
            string os;
            switch (report.summary.platform)
            {
                case BuildTarget.StandaloneWindows64: os = "windows"; break;
                case BuildTarget.StandaloneLinux64: os = "linux"; break;
                case BuildTarget.StandaloneOSX: os = "macos"; break;
                default: return;
            }
            try
            {
                var settings = PublisherSettings.instance;
                string name = string.IsNullOrWhiteSpace(settings.buildNameOverride) ? PlayerSettings.productName : settings.buildNameOverride;
                var files = Array.ConvertAll(report.GetFiles(), f => f.path);
                var record = BuildRecord.Discover(report.summary.outputPath, files, name, PlayerSettings.bundleVersion,
                    BuildArchitecture.Detect(report.summary.outputPath, os), report.summary.GetSubtarget<StandaloneBuildSubtarget>() == StandaloneBuildSubtarget.Server, DateTime.UtcNow);
                PublisherController.Remember(record);
                if (settings.automaticPublishing) EditorApplication.delayCall += () => PublisherController.Start(record);
            }
            catch (Exception error) { Debug.LogError("Fleab: Unity built successfully, but output discovery failed. Original build preserved. " + error); }
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
                if (!File.Exists(RecordPath)) throw new IOException("No successful supported build is recorded. Build a desktop player once with Fleab installed.");
                Start(JsonUtility.FromJson<BuildRecord>(File.ReadAllText(RecordPath)));
            }
            catch (Exception error) { EditorUtility.DisplayDialog("Fleab Publisher", error.Message, "OK"); }
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
