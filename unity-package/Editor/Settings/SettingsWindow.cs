using System.IO;
using UnityEditor;
using UnityEngine;

namespace Fleab.Publisher
{
    internal sealed class SettingsWindow : EditorWindow
    {
        Vector2 scroll;
        GUIStyle contentStyle;

        [MenuItem("Tools/Fleab/Settings")]
        static void Open()
        {
            string version = UnityEditor.PackageManager.PackageInfo.FindForAssembly(typeof(SettingsWindow).Assembly)?.version;
            var window = GetWindow<SettingsWindow>(string.IsNullOrEmpty(version) ? "Fleab Publisher" : "Fleab Publisher " + version);
            window.minSize = new Vector2(360, 320);
        }

        void OnGUI()
        {
            // Use Unity's own controls and skin so this follows the editor theme.
            contentStyle ??= new GUIStyle { padding = new RectOffset(16, 16, 16, 16) };
            scroll = EditorGUILayout.BeginScrollView(scroll);
            GUILayout.BeginVertical(contentStyle);
            EditorGUILayout.LabelField("Publishing", EditorStyles.boldLabel);
            GUILayout.Space(8);
            var settings = PublisherSettings.instance;
            EditorGUI.BeginChangeCheck();
            settings.automaticPublishing = EditorGUILayout.ToggleLeft("Publish successful builds automatically", settings.automaticPublishing);
            GUILayout.Space(12);
            EditorGUILayout.LabelField("Publish folder");
            EditorGUILayout.BeginHorizontal();
            settings.publishingDirectory = EditorGUILayout.TextField(settings.publishingDirectory);
            var browse = new GUIContent(EditorGUIUtility.IconContent("Folder Icon")) { tooltip = "Choose publishing folder" };
            if (GUILayout.Button(browse, GUILayout.Width(30)))
            {
                string path = EditorUtility.OpenFolderPanel("Publishing directory", settings.publishingDirectory, "");
                if (!string.IsNullOrEmpty(path)) { settings.publishingDirectory = path; settings.Persist(); }
            }
            EditorGUILayout.EndHorizontal();
            GUILayout.Space(12);
            EditorGUILayout.LabelField("Build name override");
            settings.buildNameOverride = EditorGUILayout.TextField(settings.buildNameOverride);
            EditorGUILayout.LabelField("Leave blank to use the project's product name.", EditorStyles.wordWrappedMiniLabel);
            if (EditorGUI.EndChangeCheck()) settings.Persist();
            if (string.IsNullOrWhiteSpace(settings.publishingDirectory) || !Path.IsPathRooted(settings.publishingDirectory))
                EditorGUILayout.HelpBox("Choose an absolute local folder or a mounted network share. Unity's build output is detected automatically.", MessageType.Info);
            using (new EditorGUI.DisabledScope(PublisherController.Busy))
            {
                GUILayout.Space(16);
                EditorGUILayout.LabelField("Last build", EditorStyles.boldLabel);
                GUILayout.Space(8);
                EditorGUILayout.BeginHorizontal();
                if (GUILayout.Button("Publish last build", GUILayout.Height(28))) PublisherController.PublishLast();
                if (GUILayout.Button("Retry publish", GUILayout.Height(28))) PublisherController.PublishLast();
                EditorGUILayout.EndHorizontal();
            }
            GUILayout.EndVertical();
            EditorGUILayout.EndScrollView();
        }
    }
}
