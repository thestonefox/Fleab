//! Actual native menus and window lifecycle. Never runs on the host desktop.
#![cfg(windows)]
use fleab::{
    archives::Target,
    deployment, platform,
    profiles::{self, Config, Profile},
    updates,
};
use std::{
    fs,
    process::{Child, Command},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM},
    UI::WindowsAndMessaging::*,
};
use windows_sys::core::BOOL;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
fn wait(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(35);
    while !condition() {
        assert!(Instant::now() < deadline, "Tray acceptance timed out");
        std::thread::sleep(Duration::from_millis(100));
    }
}
struct Search {
    pid: u32,
    class: String,
    window: HWND,
}
unsafe extern "system" fn find_window(window: HWND, data: LPARAM) -> BOOL {
    let search = unsafe { &mut *(data as *mut Search) };
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(window, &mut pid);
    }
    if pid == search.pid {
        let mut class = [0u16; 256];
        let len = unsafe { GetClassNameW(window, class.as_mut_ptr(), class.len() as i32) };
        let class = String::from_utf16_lossy(&class[..len as usize]);
        let mut title = [0u16; 256];
        let len = unsafe { GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32) };
        let title = String::from_utf16_lossy(&title[..len as usize]);
        if (!search.class.is_empty() && class == search.class)
            || (search.class.is_empty() && title.starts_with("Fleab "))
        {
            search.window = window;
            return 0;
        }
    }
    1
}
fn window(pid: u32, class: &str) -> HWND {
    let mut search = Search {
        pid,
        class: class.into(),
        window: std::ptr::null_mut(),
    };
    unsafe {
        EnumWindows(Some(find_window), &mut search as *mut Search as LPARAM);
    }
    search.window
}
unsafe fn find_command(menu: HMENU, label: &str) -> Option<u32> {
    for index in 0..unsafe { GetMenuItemCount(menu) } {
        let mut text = [0u16; 256];
        let mut info = MENUITEMINFOW {
            cbSize: size_of::<MENUITEMINFOW>() as u32,
            fMask: MIIM_ID | MIIM_STRING | MIIM_SUBMENU,
            dwTypeData: text.as_mut_ptr(),
            cch: 255,
            ..unsafe { std::mem::zeroed() }
        };
        if unsafe { GetMenuItemInfoW(menu, index as u32, 1, &mut info) } == 0 {
            continue;
        }
        if String::from_utf16_lossy(&text[..info.cch as usize]) == label {
            return Some(info.wID);
        }
        if !info.hSubMenu.is_null()
            && let Some(id) = unsafe { find_command(info.hSubMenu, label) }
        {
            return Some(id);
        }
    }
    None
}
fn invoke(tray: HWND, label: &str) {
    // Exercise the real context menu and its current dynamic command IDs.
    unsafe {
        PostMessageW(tray, 6002, 0, WM_RBUTTONUP as LPARAM);
    }
    let mut popup = std::ptr::null_mut();
    wait(|| {
        popup = unsafe { FindWindowW(wide("#32768").as_ptr(), std::ptr::null()) };
        !popup.is_null()
    });
    let menu = unsafe { SendMessageW(popup, 0x01e1, 0, 0) as HMENU }; // MN_GETHMENU
    assert!(!menu.is_null(), "Native popup menu handle");
    let id = unsafe { find_command(menu, label) }
        .unwrap_or_else(|| panic!("Missing tray action {label}"));
    unsafe {
        PostMessageW(tray, WM_CANCELMODE, 0, 0);
    }
    std::thread::sleep(Duration::from_millis(100));
    unsafe {
        PostMessageW(tray, WM_COMMAND, id as usize, 0);
    }
}
fn player(profile: &Profile) -> Vec<sysinfo::Pid> {
    let root = platform::resolved(profile.destination(Target::WindowsClient).unwrap()).unwrap();
    let system = sysinfo::System::new_all();
    system
        .processes()
        .iter()
        .filter_map(|(pid, process)| {
            process
                .exe()
                .filter(|exe| platform::resolved(exe).is_ok_and(|exe| exe.starts_with(&root)))
                .map(|_| *pid)
        })
        .collect()
}
fn stop_players(profile: &Profile) {
    let system = sysinfo::System::new_all();
    for pid in player(profile) {
        if let Some(process) = system.process(pid) {
            process.kill();
        }
    }
    wait(|| player(profile).is_empty());
}
struct App(Child);
impl Drop for App {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[ignore = "Actual GUI and game processes; hidden Windows Sandbox only"]
fn native_tray_hides_restores_deploys_runs_and_exits() {
    assert_eq!(
        std::env::var("USERNAME").unwrap(),
        "WDAGUtilityAccount",
        "Sandbox only"
    );
    let step = |text: &str| {
        println!("{text}");
        fs::write("C:/FleabResults/tray-progress.txt", text).unwrap();
    };
    let root = tempfile::tempdir_in("C:/FleabWork").unwrap();
    let source = root.path().join("archives");
    fs::create_dir(&source).unwrap();
    let archive = std::env::var("FLEAB_ACCEPTANCE_ARCHIVE").unwrap();
    for (version, time) in [("1", "080000"), ("2", "090000")] {
        fs::copy(
            &archive,
            source.join(format!("Game_{version}_20261112-{time}_Windows64.tar.zst")),
        )
        .unwrap();
    }
    let mut profile = Profile {
        name: "Tray acceptance game".into(),
        source: source.to_string_lossy().into(),
        ..Default::default()
    };
    profile.destinations.insert(
        Target::WindowsClient,
        root.path().join("installed").to_string_lossy().into(),
    );
    profile.launch.game_arguments = vec![
        "-batchmode".into(),
        "-nographics".into(),
        "-logFile".into(),
        root.path().join("player.log").to_string_lossy().into(),
    ];
    let config = Config {
        selected_profile: Some(profile.id),
        profiles: vec![profile.clone()],
        ..Default::default()
    };
    let path = profiles::config_path().unwrap();
    config.save(&path).unwrap();
    let preferences = path.parent().unwrap().join("desktop.json");
    fs::write(
        &preferences,
        br#"{"enabled":true,"minimise":true,"close":true}"#,
    )
    .unwrap();
    updates::Preferences {
        automatic: false,
        ..Default::default()
    }
    .save(&updates::Paths::current().unwrap().preferences)
    .unwrap();
    let mut app = App(Command::new("C:/FleabInput/fleab.exe").spawn().unwrap());
    let mut main = std::ptr::null_mut();
    let mut tray = std::ptr::null_mut();
    step("Waiting for native window and tray");
    wait(|| {
        main = window(app.0.id(), "");
        tray = window(app.0.id(), "tray_icon_app");
        !main.is_null() && !tray.is_null()
    });
    step("Closing to tray");
    unsafe {
        PostMessageW(main, WM_CLOSE, 0, 0);
    }
    wait(|| unsafe { IsWindowVisible(main) == 0 });
    assert!(app.0.try_wait().unwrap().is_none());
    step("Restoring from native Open Fleab menu");
    invoke(tray, "Open Fleab");
    wait(|| unsafe { IsWindowVisible(main) != 0 });
    step("Minimising to tray");
    unsafe {
        ShowWindow(main, SW_MINIMIZE);
    }
    wait(|| unsafe { IsWindowVisible(main) == 0 });
    step("Deploy latest while hidden");
    invoke(tray, "Deploy latest");
    wait(|| {
        deployment::installed(&profile, Target::WindowsClient)
            .ok()
            .flatten()
            .is_some_and(|r| r.filename.contains("Game_2_"))
    });
    assert_eq!(unsafe { IsWindowVisible(main) }, 0);
    std::thread::sleep(Duration::from_secs(2));
    step("Deploy and run latest while hidden");
    invoke(tray, "Deploy and run latest");
    wait(|| !player(&profile).is_empty());
    stop_players(&profile);
    std::thread::sleep(Duration::from_secs(2));
    assert!(source.starts_with(root.path()));
    fs::rename(&source, root.path().join("offline")).unwrap();
    step("Run installed with source offline");
    invoke(tray, "Run installed");
    wait(|| !player(&profile).is_empty());
    step("Explicit Exit Fleab");
    invoke(tray, "Exit Fleab");
    wait(|| app.0.try_wait().unwrap().is_some());
    stop_players(&profile);
    step("Disabled preference preserves normal close");
    fs::write(&preferences, br#"{"enabled":false}"#).unwrap();
    let mut app = App(Command::new("C:/FleabInput/fleab.exe").spawn().unwrap());
    wait(|| {
        main = window(app.0.id(), "");
        !main.is_null()
    });
    unsafe {
        PostMessageW(main, WM_CLOSE, 0, 0);
    }
    wait(|| app.0.try_wait().unwrap().is_some());
    step("Passed");
}
