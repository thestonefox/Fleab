using System.IO;
using UnityEditor;
using UnityEngine;

namespace Fleab.Publisher
{
    internal sealed class SettingsWindow : EditorWindow
    {
        Vector2 scroll;
        GUIStyle contentStyle;

        internal static void PublishFolderRects(Rect row, out Rect field, out Rect button, out Rect icon)
        {
            row.height = EditorGUIUtility.singleLineHeight;
            float buttonWidth = Mathf.Min(28, row.width);
            button = new Rect(row.xMax - buttonWidth, row.y, buttonWidth, row.height);
            field = new Rect(row.x, row.y, Mathf.Max(0, row.width - buttonWidth - 4), row.height);
            float iconSize = Mathf.Max(0, Mathf.Min(16, button.width - 4, button.height - 4));
            icon = new Rect(button.center.x - iconSize / 2, button.center.y - iconSize / 2, iconSize, iconSize);
        }

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
            // Reserve one editor line; the source icon texture must not size the row.
            var row = EditorGUILayout.GetControlRect(false, EditorGUIUtility.singleLineHeight);
            PublishFolderRects(row, out var fieldRect, out var buttonRect, out var iconRect);
            settings.publishingDirectory = EditorGUI.TextField(fieldRect, settings.publishingDirectory);
            if (GUI.Button(buttonRect, new GUIContent("", "Choose publishing folder"), EditorStyles.miniButton))
            {
                string path = EditorUtility.OpenFolderPanel("Publishing directory", settings.publishingDirectory, "");
                if (!string.IsNullOrEmpty(path)) { settings.publishingDirectory = path; settings.Persist(); }
            }
            if (Event.current.type == EventType.Repaint)
                GUI.DrawTexture(iconRect, EditorGUIUtility.IconContent("Folder Icon").image, ScaleMode.ScaleToFit);
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
