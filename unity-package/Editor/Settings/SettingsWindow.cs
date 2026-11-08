using System.IO;
using UnityEditor;
using UnityEngine;

namespace Fleab.Publisher
{
    internal sealed class SettingsWindow : EditorWindow
    {
        [MenuItem("Tools/Fleab/Settings")]
        static void Open() => GetWindow<SettingsWindow>("Fleab Publisher");

        void OnGUI()
        {
            EditorGUILayout.LabelField("Fleab Publisher", EditorStyles.boldLabel);
            EditorGUILayout.LabelField("Build it. Hop it. Play it.");
            var settings = PublisherSettings.instance;
            EditorGUI.BeginChangeCheck();
            settings.automaticPublishing = EditorGUILayout.Toggle("Enable automatic publishing", settings.automaticPublishing);
            EditorGUILayout.BeginHorizontal();
            settings.publishingDirectory = EditorGUILayout.TextField("Publishing directory", settings.publishingDirectory);
            if (GUILayout.Button("Browse", GUILayout.Width(65)))
            {
                string path = EditorUtility.OpenFolderPanel("Publishing directory", settings.publishingDirectory, "");
                if (!string.IsNullOrEmpty(path)) { settings.publishingDirectory = path; settings.Persist(); }
            }
            EditorGUILayout.EndHorizontal();
            settings.buildNameOverride = EditorGUILayout.TextField("Build name override", settings.buildNameOverride);
            if (EditorGUI.EndChangeCheck()) settings.Persist();
            if (string.IsNullOrWhiteSpace(settings.publishingDirectory) || !Path.IsPathRooted(settings.publishingDirectory))
                EditorGUILayout.HelpBox("Choose an absolute local folder or a mounted network share. Unity's build output is detected automatically.", MessageType.Info);
            using (new EditorGUI.DisabledScope(PublisherController.Busy))
            {
                if (GUILayout.Button("Publish Last Build")) PublisherController.PublishLast();
                if (GUILayout.Button("Retry Publish")) PublisherController.PublishLast();
            }
        }
    }
}
