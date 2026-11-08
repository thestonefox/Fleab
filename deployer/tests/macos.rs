use fleab::{archives::Target, launching};
use std::{fs, path::Path};
#[test]
fn bundle_entrypoint_is_validated_from_binary_plist() {
    let root = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let bundle = root.path().join("Game.app");
    fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
    let mut info = plist::Dictionary::new();
    info.insert(
        "CFBundleExecutable".into(),
        plist::Value::String("Game".into()),
    );
    plist::Value::Dictionary(info.clone())
        .to_file_binary(bundle.join("Contents/Info.plist"))
        .unwrap();
    assert!(launching::detect(root.path(), Target::MacClient, None).is_err());
    fs::write(bundle.join("Contents/MacOS/Game"), b"\xcf\xfa\xed\xfe").unwrap();
    assert_eq!(
        launching::detect(root.path(), Target::MacClient, None).unwrap(),
        Path::new("Game.app")
    );
    info.insert(
        "CFBundleExecutable".into(),
        plist::Value::String("../../escape".into()),
    );
    plist::Value::Dictionary(info)
        .to_file_binary(bundle.join("Contents/Info.plist"))
        .unwrap();
    assert!(launching::detect(root.path(), Target::MacClient, Some("Game.app")).is_err());
}
