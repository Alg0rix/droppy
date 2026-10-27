//! Shell catalogs behind the Windows-page launcher: packaged apps
//! (shell:AppsFolder plus the Start Menu .lnk fallback), Settings pages
//! parsed out of ImmersiveControlPanel's search XML, and the "God Mode"
//! Control Panel task list. Each catalog is built once behind a OnceLock —
//! the launcher worker (an STA thread) is the only caller of the `*_()`
//! builders; the UI thread reads via `loaded_*` and never pays for a build.

use std::collections::HashSet;
use std::sync::OnceLock;

/// One Settings search entry: resolved display name, `;`-separated keyword
/// phrases (resolved the same way), and the URI that opens the page.
#[derive(Debug)]
pub struct SettingEntry {
    pub name: String,
    pub keywords: String,
    pub uri: String,
}

/// One `<SearchableContent>` element, strings still unresolved.
pub struct RawSetting {
    pub filename: String,
    pub description_ref: String,
    pub keywords_refs: Vec<String>,
    pub page_id: Option<String>,
    pub host_id: Option<String>,
}

static APPS: OnceLock<Vec<(String, String)>> = OnceLock::new();
static SETTINGS: OnceLock<Vec<SettingEntry>> = OnceLock::new();
static TASKS: OnceLock<Vec<(String, String)>> = OnceLock::new();

/// Control Panel task paths don't round-trip through the shell parser, so
/// each task's absolute PIDL is captured at build time and kept for the
/// process lifetime (they're a few bytes of CoTaskMem each). Stored as
/// usize: raw pointers aren't Sync. Keyed by the path string `tasks()` hands
/// out — launches and icon loads look the PIDL up from it.
static TASK_PIDLS: OnceLock<Vec<(String, usize)>> = OnceLock::new();

/// (display name, launch path) for every packaged app and Start Menu .lnk,
/// sorted by lowercase name. First call builds it — worker thread only.
pub fn apps() -> &'static [(String, String)] {
    APPS.get_or_init(build_apps)
}

/// (display name, launch uri) for every searchable Settings page.
pub fn settings() -> &'static [SettingEntry] {
    SETTINGS.get_or_init(build_settings)
}

/// (display name, desktop-absolute parsing path) for every Control Panel
/// task in the God Mode folder.
pub fn tasks() -> &'static [(String, String)] {
    TASKS.get_or_init(build_tasks)
}

/// Non-building readers for the UI thread: `Some` once the worker built it.
pub fn loaded_apps() -> Option<&'static [(String, String)]> {
    APPS.get().map(Vec::as_slice)
}

pub fn loaded_settings() -> Option<&'static [SettingEntry]> {
    SETTINGS.get().map(Vec::as_slice)
}

pub fn loaded_tasks() -> Option<&'static [(String, String)]> {
    TASKS.get().map(Vec::as_slice)
}

fn build_apps() -> Vec<(String, String)> {
    let mut out = apps_folder_items();
    let mut names: HashSet<String> = out.iter().map(|(n, _)| n.to_lowercase()).collect();
    for (name, path) in start_menu_lnks() {
        if names.insert(name.to_lowercase()) {
            out.push((name, path));
        }
    }
    out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    out
}

/// Absolute PIDL for a `tasks()` path string — the desktop-absolute parsing
/// name these items produce (`::{CLSID}\0\::{CLSID}\{id}`) can't be re-parsed
/// by SHParseDisplayName, so the launch/icon code needs the real PIDL.
pub fn task_pidl(path: &str) -> *mut windows::Win32::UI::Shell::Common::ITEMIDLIST {
    TASK_PIDLS.get().and_then(|v| v.iter().find(|(p, _)| p == path).map(|(_, p)| *p as *mut _)).unwrap_or(std::ptr::null_mut())
}

fn build_tasks() -> Vec<(String, String)> {
    use windows::core::w;
    use windows::Win32::System::Com::IBindCtx;
    use windows::Win32::UI::Shell::{IShellItem, SHCreateItemFromParsingName, SHGetIDListFromObject, SIGDN_DESKTOPABSOLUTEPARSING, SIGDN_NORMALDISPLAY};
    let mut pidls: Vec<(String, usize)> = Vec::new();
    let out = unsafe {
        let folder: IShellItem = match SHCreateItemFromParsingName(w!("shell:::{ED7BA470-8E54-465E-825C-99712043E01C}"), None::<&IBindCtx>) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };
        enum_items(&folder)
            .iter()
            .filter_map(|i| {
                let name = display_name(i, SIGDN_NORMALDISPLAY);
                let path = display_name(i, SIGDN_DESKTOPABSOLUTEPARSING);
                if name.is_empty() || path.is_empty() {
                    None
                } else {
                    if let Ok(pidl) = SHGetIDListFromObject(i) {
                        pidls.push((path.clone(), pidl as usize));
                    }
                    Some((name, path))
                }
            })
            .collect()
    };
    let _ = TASK_PIDLS.set(pidls);
    out
}

/// Packaged apps + shell items: FOLDERID_AppsFolder children named
/// `shell:AppsFolder\<parsing name>` so ShellExecuteW can open them.
fn apps_folder_items() -> Vec<(String, String)> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::UI::Shell::{FOLDERID_AppsFolder, IShellItem, SHGetKnownFolderItem, KF_FLAG_DEFAULT, SIGDN_NORMALDISPLAY, SIGDN_PARENTRELATIVEPARSING};
    unsafe {
        let folder: IShellItem = match SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, HANDLE::default()) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };
        enum_items(&folder)
            .iter()
            .filter_map(|i| {
                let name = display_name(i, SIGDN_NORMALDISPLAY);
                let parsing = display_name(i, SIGDN_PARENTRELATIVEPARSING);
                if name.is_empty() || parsing.is_empty() {
                    None
                } else {
                    Some((name, format!("shell:AppsFolder\\{parsing}")))
                }
            })
            .collect()
    }
}

/// Every .lnk under both Start Menu Programs roots (per-user + machine).
fn start_menu_lnks() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for dir in [
        std::env::var("APPDATA").map(|a| format!("{a}\\Microsoft\\Windows\\Start Menu\\Programs")),
        std::env::var("PROGRAMDATA").map(|a| format!("{a}\\Microsoft\\Windows\\Start Menu\\Programs")),
    ]
    .into_iter()
    .flatten()
    {
        scan_dir(std::path::Path::new(&dir), &mut out);
    }
    out
}

fn scan_dir(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            scan_dir(&p, out);
        } else if p.extension().map(|x| x == "lnk").unwrap_or(false) {
            let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
            if !name.is_empty() {
                out.push((name, p.to_string_lossy().into_owned()));
            }
        }
    }
}

/// Children of a shell folder item via the standard items enumerator.
fn enum_items(folder: &windows::Win32::UI::Shell::IShellItem) -> Vec<windows::Win32::UI::Shell::IShellItem> {
    use windows::Win32::System::Com::IBindCtx;
    use windows::Win32::UI::Shell::{BHID_EnumItems, IEnumShellItems};
    let mut out = Vec::new();
    unsafe {
        let Ok(items) = folder.BindToHandler::<_, IEnumShellItems>(None::<&IBindCtx>, &BHID_EnumItems) else {
            return out;
        };
        loop {
            let mut buf: [Option<windows::Win32::UI::Shell::IShellItem>; 16] = std::array::from_fn(|_| None);
            let mut got = 0u32;
            if items.Next(&mut buf, Some(&mut got)).is_err() || got == 0 {
                break;
            }
            out.extend(buf.into_iter().flatten());
        }
    }
    out
}

/// IShellItem::GetDisplayName + free — the PWSTR is CoTaskMem-owned.
fn display_name(item: &windows::Win32::UI::Shell::IShellItem, sigdn: windows::Win32::UI::Shell::SIGDN) -> String {
    use windows::Win32::System::Com::CoTaskMemFree;
    unsafe {
        let Ok(p) = item.GetDisplayName(sigdn) else { return String::new() };
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as *const _));
        s
    }
}

// ------------------------------------------------------------------ settings

fn build_settings() -> Vec<SettingEntry> {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    let dir = std::path::Path::new(&root).join(r"ImmersiveControlPanel\Settings");
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    let n = p.file_name().and_then(|s| s.to_str()).unwrap_or_default();
                    n.starts_with("AllSystemSettings_") && n.ends_with(".xml")
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    // Files duplicate each other's entries heavily — dedupe on Filename
    // before spending SHLoadIndirectString calls on repeats.
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut out: Vec<SettingEntry> = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else { continue };
        for raw in parse_settings_xml(&text) {
            if !seen_ids.insert(raw.filename) {
                continue;
            }
            let name = resolve_indirect(&raw.description_ref);
            if name.is_empty() {
                continue;
            }
            let keywords = raw.keywords_refs.iter().map(|r| resolve_indirect(r)).filter(|k| !k.is_empty()).collect::<Vec<_>>().join(";");
            if !seen_names.insert(name.to_lowercase()) {
                continue;
            }
            out.push(SettingEntry { name, keywords, uri: page_uri(raw.page_id.as_deref(), raw.host_id.as_deref()) });
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// Split one AllSystemSettings_*.xml into its <SearchableContent> entries.
/// Entries gated behind a <Condition> are skipped — they describe classic
/// Control Panel items the tasks catalog already covers.
pub fn parse_settings_xml(text: &str) -> Vec<RawSetting> {
    let mut out = Vec::new();
    for chunk in text.split("<SearchableContent").skip(1) {
        if chunk.contains("<Condition") {
            continue;
        }
        let Some(filename) = tag_text(chunk, "Filename") else { continue };
        let Some(description) = tag_text(chunk, "Description") else { continue };
        let kw = tag_text(chunk, "HighKeywords").or_else(|| tag_text(chunk, "Keywords")).unwrap_or_default();
        out.push(RawSetting { filename, description_ref: description, keywords_refs: keyword_refs(&kw), page_id: page_id_of(chunk), host_id: tag_text(chunk, "HostID") });
    }
    out
}

/// First `<tag>text</tag>` body in `chunk` (no attributes on these tags).
fn tag_text(chunk: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let start = chunk.find(&open)? + open.len();
    let end = chunk[start..].find(&format!("</{tag}>"))? + start;
    let s = chunk[start..end].trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// A keywords body is either one indirect string, several concatenated
/// `@@a@b` indirect strings, or plain text — each form becomes ref(s).
fn keyword_refs(raw: &str) -> Vec<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Vec::new();
    }
    if !raw.starts_with('@') {
        return vec![raw.to_string()];
    }
    raw.split('@').filter(|s| !s.is_empty()).map(|s| format!("@{s}")).collect()
}

/// Page id for a chunk: `<PageID>` inside a `<Path>` wins (several Paths =
/// several routes to the page; the first is canonical), then any top-level
/// `<PageID>` (GUID pages), then `<Node Type="Page">` (the other schema).
fn page_id_of(chunk: &str) -> Option<String> {
    let mut rest = chunk;
    while let Some(i) = rest.find("<Path>") {
        let body = &rest[i + "<Path>".len()..];
        let end = body.find("</Path>").unwrap_or(body.len());
        if let Some(id) = tag_text(&body[..end], "PageID") {
            return Some(id);
        }
        if end == body.len() {
            break;
        }
        rest = &body[end + "</Path>".len()..];
    }
    if let Some(id) = tag_text(chunk, "PageID") {
        return Some(id);
    }
    const MARK: &str = "<Node Type=\"Page\">";
    let i = chunk.find(MARK)? + MARK.len();
    let e = chunk[i..].find("</Node>")? + i;
    let s = chunk[i..e].trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// `@{...}`/`@dll,-id` indirect strings go through SHLoadIndirectString;
/// anything else is already literal text. Empty on failure (e.g. packaged
/// resources like Microsoft.SecHealthUI that won't resolve unelevated).
fn resolve_indirect(s: &str) -> String {
    use windows::core::HSTRING;
    use windows::Win32::UI::Shell::SHLoadIndirectString;
    let t = s.trim();
    if !t.starts_with('@') {
        return t.to_string();
    }
    unsafe {
        let mut buf = [0u16; 2048];
        match SHLoadIndirectString(&HSTRING::from(t), &mut buf, None) {
            Ok(()) => {
                let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                String::from_utf16_lossy(&buf[..end]).trim().to_string()
            }
            Err(_) => String::new(),
        }
    }
}

/// Launch URI for a Settings search entry: the Settings app's own ms-settings
/// deep links, Defender pages through windowsdefender:, and classic Control
/// Panel CLSIDs through their shell namespace path.
pub fn page_uri(page_id: Option<&str>, host_id: Option<&str>) -> String {
    const DEFENDER: &str = "{6E6DDBCB-9C89-434B-A994-D5F22239523B}";
    let id = page_id.unwrap_or_default();
    if id.eq_ignore_ascii_case(DEFENDER) || host_id.map(|h| h.eq_ignore_ascii_case(DEFENDER)).unwrap_or(false) {
        return "windowsdefender:".to_string();
    }
    if id.starts_with('{') && id.ends_with('}') {
        return format!("shell:::{id}");
    }
    match id {
        "SettingsPageA9" => "ms-settings:a9",
        "SettingsPageAbout" => "ms-settings:about",
        "SettingsPagePCSystemInfo" => "ms-settings:about",
        "SettingsPageAccountsEmailApp" => "ms-settings:emailandaccounts",
        "SettingsPageAccountsPicture" => "ms-settings:yourinfo",
        "SettingsPageAccountsSync" => "ms-settings:sync",
        "SettingsPageAccountsUsers" => "ms-settings:otherusers",
        "SettingsPageOtherUsers" => "ms-settings:otherusers",
        "SettingsPageActivate" => "ms-settings:activation",
        "SettingsPageAdvancedApps" => "ms-settings:advanced-apps",
        "SettingsPageAdvancedSystemSettings" => "ms-settings:developers",
        "SettingsPageRestoreDeveloperOptions" => "ms-settings:developers",
        "SettingsPageAIComponents" => "ms-settings:aicomponents",
        "SettingsPageAppActions" => "ms-settings:appactions",
        "SettingsPageAppsDefaults" => "ms-settings:defaultapps",
        "SettingsPageAppsForWebsites" => "ms-settings:appsforwebsites",
        "SettingsPageAppsNotifications" => "ms-settings:notifications",
        "SettingsPageInstalledApps" => "ms-settings:appsfeatures",
        "SettingsPageAppsSizes" => "ms-settings:appsfeatures",
        "SettingsPageAudio" => "ms-settings:sound",
        "SettingsPageBackground" => "ms-settings:personalization-background",
        "SettingsPageC2D" => "ms-settings:clicktodo",
        "SettingsPageClipboard" => "ms-settings:clipboard",
        "SettingsPageColors" => "ms-settings:colors",
        "SettingsPageContinuum" => "ms-settings:project",
        "SettingsPageDeviceEncryption" => "ms-settings:deviceencryption",
        "SettingsPageDevices" => "ms-settings:bluetooth",
        "SettingsPagePCSystemBluetooth" => "ms-settings:bluetooth",
        "SettingsPagePCSystemDevices" => "ms-settings:connecteddevices",
        "SettingsPageDevicesCameras" => "ms-settings:camera",
        "SettingsPageDevicesDL" => "ms-settings:personalization-lighting",
        "SettingsPageDevicesKeyboard" => "ms-settings:devices-keyboard",
        "SettingsPageDevicesMobileDevices" => "ms-settings:mobile-devices",
        "SettingsPageDevicesPen" => "ms-settings:pen",
        "SettingsPageDevicesPrinters" => "ms-settings:printers",
        "SettingsPageDevicesTouch" => "ms-settings:devices-touch",
        "SettingsPageDevicesTouchpad" => "ms-settings:devices-touchpad",
        "SettingsPageEaseOfAccessAudio" => "ms-settings:easeofaccess-audio",
        "SettingsPageEaseOfAccessClosedCaptioning" => "ms-settings:easeofaccess-closedcaptioning",
        "SettingsPageEaseOfAccessColorFilter" => "ms-settings:easeofaccess-colorfilter",
        "SettingsPageEaseOfAccessEyeGaze" => "ms-settings:easeofaccess-eyecontrol",
        "SettingsPageEaseOfAccessHearingAids" => "ms-settings:easeofaccess-hearingaids",
        "SettingsPageEaseOfAccessHighContrast" => "ms-settings:easeofaccess-highcontrast",
        "SettingsPageEaseOfAccessKeyboard" => "ms-settings:easeofaccess-keyboard",
        "SettingsPageEaseOfAccessMagnifier" => "ms-settings:easeofaccess-magnifier",
        "SettingsPageEaseOfAccessMouse" => "ms-settings:easeofaccess-mouse",
        "SettingsPageEaseOfAccessMousePointer" => "ms-settings:easeofaccess-mousepointer",
        "SettingsPageEaseOfAccessNarrator" => "ms-settings:easeofaccess-narrator",
        "SettingsPageEaseOfAccessScreenTint" => "ms-settings:easeofaccess-screentint",
        "SettingsPageEaseOfAccessSpeechRecognition" => "ms-settings:easeofaccess-speechrecognition",
        "SettingsPageEaseOfAccessTextCursor" => "ms-settings:easeofaccess-cursor",
        "SettingsPageEaseOfAccessTextSize" => "ms-settings:easeofaccess-display",
        "SettingsPageEaseOfAccessVisualEffects" => "ms-settings:easeofaccess-visualeffects",
        "SettingsPageExtensibility" => "ms-settings:extras",
        "SettingsPageFindMyDevice" => "ms-settings:findmydevice",
        "SettingsPageFlights" => "ms-settings:windowsinsider",
        "SettingsPageFocus" => "ms-settings:quiethours",
        "SettingsPageFonts" => "ms-settings:fonts",
        "SettingsPageGameBar" => "ms-settings:gaming-gamebar",
        "SettingsPageGameDVR" => "ms-settings:gaming-gamedvr",
        "SettingsPageGameMode" => "ms-settings:gaming-gamemode",
        "SettingsPageGamingPosture" => "ms-settings:gaming-fullscreen",
        "SettingsPageLockScreen" => "ms-settings:lockscreen",
        "SettingsPageMaps" => "ms-settings:maps",
        "SettingsPageMultiTasking" => "ms-settings:multitasking",
        "SettingsPageNetworkAirplaneMode" => "ms-settings:network-airplanemode",
        "SettingsPageNetworkDialup" => "ms-settings:network-dialup",
        "SettingsPageNetworkDirectAccess" => "ms-settings:network-directaccess",
        "SettingsPageNetworkEthernet" => "ms-settings:network-ethernet",
        "SettingsPageNetworkFirewall" => "windowsdefender://network",
        "SettingsPageNetworkManageAdapterOptions" => "ms-settings:network-advancedsettings",
        "SettingsPageNetworkMobileBroadband" => "ms-settings:network-cellular",
        "SettingsPageNetworkMobileHotspot" => "ms-settings:network-mobilehotspot",
        "SettingsPageNetworkProxy" => "ms-settings:network-proxy",
        "SettingsPageNetworkVPN" => "ms-settings:network-vpn",
        "SettingsPageNetworkWiFi" => "ms-settings:network-wifi",
        "SettingsPageNetworkWorkplace" => "ms-settings:workplace",
        "SettingsPageWorkAccess" => "ms-settings:workplace",
        "SettingsPageOptionalFeatures" => "ms-settings:optionalfeatures",
        "SettingsPagePasskeysManagement" => "ms-settings:passkeys-advancedoptions",
        "SettingsPagePCSystemAutoPlay" => "ms-settings:autoplay",
        "SettingsPagePCSystemDeviceSettings" => "ms-settings:mousetouchpad",
        "SettingsPagePCSystemDisplay" => "ms-settings:display",
        "SettingsPagePersonalization60813048" => "ms-settings:personalization-contextmenu",
        "SettingsPagePersonalizationTextInput" => "ms-settings:personalization-textinput",
        "SettingsPagePersonalizeCopilot" => "ms-settings:personalization-copilot",
        "SettingsPagePowerAndBattery" => "ms-settings:powersleep",
        "SettingsPageScreenPowerAndSleep" => "ms-settings:powersleep",
        "SettingsPagePrivacyAccountInfo" => "ms-settings:privacy-accountinfo",
        "SettingsPagePrivacyActivityHistory" => "ms-settings:privacy-activityhistory",
        "SettingsPagePrivacyAgentTools" => "ms-settings:privacy-agenttools",
        "SettingsPagePrivacyAppDiagnostics" => "ms-settings:privacy-appdiagnostics",
        "SettingsPagePrivacyAppRequestedDownloads" => "ms-settings:privacy-automaticfiledownloads",
        "SettingsPagePrivacyBackgroundAITasks" => "ms-settings:privacy-backgroundaitasks",
        "SettingsPagePrivacyBackgroundApps" => "ms-settings:privacy-backgroundapps",
        "SettingsPagePrivacyBroadFileSystemAccess" => "ms-settings:privacy-broadfilesystemaccess",
        "SettingsPagePrivacyCalendar" => "ms-settings:privacy-calendar",
        "SettingsPagePrivacyCallHistory" => "ms-settings:privacy-callhistory",
        "SettingsPagePrivacyContacts" => "ms-settings:privacy-contacts",
        "SettingsPagePrivacyCustomPeripherals" => "ms-settings:privacy-customdevices",
        "SettingsPagePrivacyDocuments" => "ms-settings:privacy-documents",
        "SettingsPagePrivacyDownloadsFolder" => "ms-settings:privacy-downloadsfolder",
        "SettingsPagePrivacyEmail" => "ms-settings:privacy-email",
        "SettingsPagePrivacyForegroundTextAccess" => "ms-settings:privacy-foregroundtextaccess",
        "SettingsPagePrivacyGaze" => "ms-settings:privacy-eyetracker",
        "SettingsPagePrivacyGeneral" => "ms-settings:privacy-general",
        "SettingsPagePrivacyGraphicsCaptureProgrammatic" => "ms-settings:privacy-graphicscaptureprogrammatic",
        "SettingsPagePrivacyGraphicsCaptureWithoutBorder" => "ms-settings:privacy-graphicscapturewithoutborder",
        "SettingsPagePrivacyHumanPresence" => "ms-settings:privacy-presencesensing",
        "SettingsPagePrivacyLocation" => "ms-settings:privacy-location",
        "SettingsPagePrivacyMessaging" => "ms-settings:privacy-messaging",
        "SettingsPagePrivacyMicrophone" => "ms-settings:privacy-microphone",
        "SettingsPagePrivacyMotionData" => "ms-settings:privacy-motion",
        "SettingsPagePrivacyMusicLibrary" => "ms-settings:privacy-musiclibrary",
        "SettingsPagePrivacyNotifications" => "ms-settings:privacy-notifications",
        "SettingsPagePrivacyPasskeys" => "ms-settings:privacy-passkeys",
        "SettingsPagePrivacyPersonalization" => "ms-settings:privacy-speechtyping",
        "SettingsPagePrivacyPhoneCall" => "ms-settings:privacy-phonecalls",
        "SettingsPagePrivacyPictures" => "ms-settings:privacy-pictures",
        "SettingsPagePrivacyRadios" => "ms-settings:privacy-radios",
        "SettingsPagePrivacySIUFSettings" => "ms-settings:privacy-feedback",
        "SettingsPagePrivacySpeech" => "ms-settings:privacy-speech",
        "SettingsPagePrivacySystemAIModels" => "ms-settings:privacy-systemaimodels",
        "SettingsPagePrivacyTasks" => "ms-settings:privacy-tasks",
        "SettingsPagePrivacyVideos" => "ms-settings:privacy-videos",
        "SettingsPagePrivacyVoiceActivation" => "ms-settings:privacy-voiceactivation",
        "SettingsPagePrivacyWebcam" => "ms-settings:privacy-webcam",
        "SettingsPageRadialController" => "ms-settings:wheel",
        "SettingsPageRationalizedSearchSettings" => "ms-settings:cortana-windowssearch",
        "SettingsPageSearchIndex" => "ms-settings:cortana-windowssearch",
        "SettingsPageRemoteDesktop" => "ms-settings:remotedesktop",
        "SettingsPageRestoreMusUpdate" => "ms-settings:windowsupdate",
        "SettingsPageRestoreOneBackup" => "ms-settings:backup",
        "SettingsPageRestoreRestore" => "ms-settings:recovery",
        "SettingsPageResume" => "ms-settings:system-resume",
        "SettingsPageSearchPermissionsAndHistory" => "ms-settings:search-permissions",
        "SettingsPageSecurityDeviceProtection" => "windowsdefender://devicesecurity",
        "SettingsPageSecurityVirusThreat" => "windowsdefender://threat",
        "SettingsPageSharedExperiences" => "ms-settings:crossdevice",
        "SettingsPageSignInOptions" => "ms-settings:signinoptions",
        "SettingsPageSpeech" => "ms-settings:speech",
        "SettingsPageStart" => "ms-settings:personalization-start",
        "SettingsPageStart2" => "ms-settings:personalization-start",
        "SettingsPageStartup" => "ms-settings:startupapps",
        "SettingsPageStorageSenseStorageOverview" => "ms-settings:storagesense",
        "SettingsPageSystemComponents" => "ms-settings:systemcomponents",
        "SettingsPageTaskbar" => "ms-settings:taskbar",
        "SettingsPageThemes" => "ms-settings:themes",
        "SettingsPageTimeRegionDateTime" => "ms-settings:dateandtime",
        "SettingsPageTimeRegionKeyboard" => "ms-settings:typing",
        "SettingsPageTimeRegionLanguage" => "ms-settings:regionlanguage",
        "SettingsPageTimeRegionSpelling" => "ms-settings:typing",
        "SettingsPageTroubleshoot" => "ms-settings:troubleshoot",
        "SettingsPageUsb" => "ms-settings:usb",
        "SettingsPageUserIntent" => "ms-settings:deviceusage",
        "SettingsPageVideo" => "ms-settings:videoplayback",
        "SettingsPageWindowsDefender" => "ms-settings:windowsdefender",
        "SettingsPageYourFamilyGroup" => "ms-settings:family-group",
        _ => "ms-settings:",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// One fixture entry per schema, plus a <Condition> entry that must
    /// never make it into the catalog.
    const FIXTURE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<PCSettings>
  <SearchableContent>
    <Filename>Path_PageID</Filename>
    <SettingIdentity>
      <SettingPaths>
        <Path><PageID>SettingsPageTaskbar</PageID><GroupID>G</GroupID></Path>
        <Path><PageID>SettingsPageColors</PageID></Path>
      </SettingPaths>
    </SettingIdentity>
    <SettingInformation>
      <Description>taskbar words</Description>
      <HighKeywords>@one.dll,-1@one.dll,-2</HighKeywords>
    </SettingInformation>
  </SearchableContent>
  <SearchableContent>
    <Filename>Node_Page</Filename>
    <SettingIdentity>
      <SettingPaths>
        <Path><Node Type="Page">SettingsPageThemes</Node><Node Type="Group">G</Node></Path>
      </SettingPaths>
    </SettingIdentity>
    <SettingInformation>
      <Description>themes words</Description>
      <Keywords>plain keywords</Keywords>
    </SettingInformation>
  </SearchableContent>
  <SearchableContent>
    <Filename>TopLevel_Guid</Filename>
    <SettingIdentity>
      <PageID>{BB06C0E4-D293-4F75-8A90-CB05B6477EEE}</PageID>
      <HostID>{12B1697E-D3A0-4DBC-B568-CCF64A3F934D}</HostID>
    </SettingIdentity>
    <SettingInformation><Description>classic</Description></SettingInformation>
  </SearchableContent>
  <SearchableContent>
    <Filename>Conditional</Filename>
    <SettingIdentity>
      <PageID>{BB06C0E4-D293-4F75-8A90-CB05B6477EEE}</PageID>
      <Condition>shcond://v1#IsIrDASupported;0</Condition>
    </SettingIdentity>
    <SettingInformation><Description>skip me</Description></SettingInformation>
  </SearchableContent>
</PCSettings>"#;

    #[test]
    fn parses_settings_xml() {
        let raws = parse_settings_xml(FIXTURE);
        assert_eq!(raws.len(), 3, "Condition-gated entry must be skipped");
        assert_eq!(raws[0].filename, "Path_PageID");
        assert_eq!(raws[0].description_ref, "taskbar words");
        assert_eq!(raws[0].page_id.as_deref(), Some("SettingsPageTaskbar"));
        assert_eq!(raws[0].keywords_refs, ["@one.dll,-1", "@one.dll,-2"]);
        assert_eq!(raws[1].page_id.as_deref(), Some("SettingsPageThemes"));
        assert_eq!(raws[1].keywords_refs, ["plain keywords"]);
        assert_eq!(raws[2].page_id.as_deref(), Some("{BB06C0E4-D293-4F75-8A90-CB05B6477EEE}"));
        assert_eq!(raws[2].host_id.as_deref(), Some("{12B1697E-D3A0-4DBC-B568-CCF64A3F934D}"));
    }

    #[test]
    fn maps_page_uris() {
        assert_eq!(page_uri(Some("SettingsPageTaskbar"), None), "ms-settings:taskbar");
        assert_eq!(page_uri(Some("{BB06C0E4-D293-4F75-8A90-CB05B6477EEE}"), None), "shell:::{BB06C0E4-D293-4F75-8A90-CB05B6477EEE}");
        assert_eq!(page_uri(Some("{6E6DDBCB-9C89-434B-A994-D5F22239523B}"), None), "windowsdefender:");
        assert_eq!(page_uri(Some("SettingsPageNetworkFirewall"), None), "windowsdefender://network");
        assert_eq!(page_uri(Some("SettingsPagePrivacyWebcam"), None), "ms-settings:privacy-webcam");
        assert_eq!(page_uri(Some("SettingsPageNobody"), None), "ms-settings:");
        assert_eq!(page_uri(None, None), "ms-settings:");
    }

    /// Live catalogs: time each build (straight through the builders, so a
    /// parallel test that already warmed a OnceLock doesn't zero the
    /// measurement), then spot-check the launch paths actually parse.
    #[test]
    fn catalogs_build_and_paths_parse() {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
        }
        let t = Instant::now();
        let built_apps = build_apps();
        eprintln!("apps build: {} entries in {:?}", built_apps.len(), t.elapsed());
        let a = apps();
        assert!(!a.is_empty(), "no apps at all — AppsFolder and Start Menu both empty?");
        assert!(a.iter().all(|(_, p)| p.starts_with("shell:AppsFolder\\") || p.to_lowercase().ends_with(".lnk")));

        let t = Instant::now();
        let built_settings = build_settings();
        eprintln!("settings build: {} entries in {:?}", built_settings.len(), t.elapsed());
        let s = settings();
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
        if std::path::Path::new(&root).join(r"ImmersiveControlPanel\Settings").exists() {
            assert!(!s.is_empty(), "settings XML exists but catalog is empty");
        }
        let terminal: Vec<_> = s.iter().filter(|e| e.name.to_lowercase().contains("terminal")).collect();
        for e in &terminal {
            eprintln!("  settings hit: {:?} -> {} [{}]", e.name, e.uri, e.keywords);
        }
        eprintln!("  \"Terminal settings\" present: {}", terminal.iter().any(|e| e.name == "Terminal settings"));

        let t = Instant::now();
        let built_tasks = build_tasks();
        eprintln!("tasks build: {} entries in {:?}", built_tasks.len(), t.elapsed());
        let k = tasks();
        assert!(!k.is_empty(), "God Mode folder enumeration returned nothing");

        // Launch paths must resolve to a real shell item: app + settings
        // icon paths parse; Control Panel task paths can't re-parse (their
        // `\0\` segment is a CPL page id, not a child name) so they go
        // through the catalog's stored absolute PIDL instead.
        use windows::core::HSTRING;
        use windows::Win32::System::Com::IBindCtx;
        use windows::Win32::UI::Shell::{IShellItem, SHCreateItemFromIDList, SHCreateItemFromParsingName};
        let mut paths = vec![("settings icon", crate::launcher::SETTINGS_ICON.to_string())];
        if let Some((_, p)) = a.iter().find(|(_, p)| p.starts_with("shell:AppsFolder\\")) {
            paths.push(("app", p.clone()));
        }
        for (what, path) in paths {
            unsafe {
                let r: windows::core::Result<IShellItem> = SHCreateItemFromParsingName(&HSTRING::from(&path), None::<&IBindCtx>);
                assert!(r.is_ok(), "{what} path did not parse: {path} -> {:?}", r.err());
            }
        }
        let Some((_, task_path)) = k.first() else { return };
        let pidl = task_pidl(task_path);
        assert!(!pidl.is_null(), "no stored PIDL for task path {task_path}");
        unsafe {
            let r: windows::core::Result<IShellItem> = SHCreateItemFromIDList(pidl);
            assert!(r.is_ok(), "task PIDL did not produce an item: {task_path} -> {:?}", r.err());
        }
        // shell:::{GUID} settings URIs (classic Control Panel pages) launch
        // through SHParseDisplayName — make sure that route parses.
        if let Some(e) = s.iter().find(|e| e.uri.starts_with("shell:::")) {
            unsafe {
                use windows::Win32::UI::Shell::Common::ITEMIDLIST;
                use windows::Win32::UI::Shell::SHParseDisplayName;
                let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
                let r = SHParseDisplayName(&HSTRING::from(&e.uri), None::<&IBindCtx>, &mut pidl, 0, None);
                assert!(r.is_ok() && !pidl.is_null(), "shell::: uri did not parse: {} -> {r:?}", e.uri);
                windows::Win32::System::Com::CoTaskMemFree(Some(pidl as *const _));
            }
        }
    }
}
