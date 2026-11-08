using UnityEditor;

namespace Fleab.Publisher
{
    [FilePath("ProjectSettings/FleabSettings.asset", FilePathAttribute.Location.ProjectFolder)]
    internal sealed class PublisherSettings : ScriptableSingleton<PublisherSettings>
    {
        public bool automaticPublishing;
        public string publishingDirectory = "";
        public string buildNameOverride = "";
        public void Persist() => Save(true);
    }
}
