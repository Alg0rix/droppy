//! Which kind of app is in front: the eyes act it out.
//!
//! Pure and table-driven so the catalog grows by adding lines:
//! - [`EXES`]: category -> executable names (lowercase, no `.exe`).
//! - [`TITLE_HINTS`]: category -> words in a window title. Used for browser
//!   tabs (web apps), app hosts (UWP frames, Java, Electron shells) and
//!   apps the exe table doesn't know.
//! - Browsers fall back to [`AppCategory::Research`].
//!
//! Users override any of it in `settings.json` (`app_themes`), keyed by exe
//! name or `title:<words>`, and can name their own themes (`eye_themes`):
//! a base category plus an iris colour.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What the user is doing in the app in front. Each one has its own eye
/// animation; adding one is an enum arm here and a pose in `ui::eyes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppCategory {
    /// IDEs and code editors.
    Coding,
    /// Shells, terminals, SSH clients.
    Terminal,
    /// Word processors, Markdown and note apps.
    Writing,
    /// Spreadsheets.
    Spreadsheet,
    /// Presentations.
    Slides,
    /// Tasks, projects, calendars, whiteboards.
    Planning,
    /// Notebooks, BI, databases, statistics.
    Data,
    /// Browsers, references, search.
    Research,
    /// PDF readers, e-books, read-later.
    Reading,
    /// Graphics, UI, 3D, CAD.
    Design,
    /// Messengers and team chat.
    Chat,
    /// Mail clients.
    Email,
    /// Video calls.
    Meeting,
    /// Courses, flashcards, language and maths apps.
    Learning,
    /// Watching or listening.
    Media,
    /// Video, audio and streaming production.
    Studio,
}

impl AppCategory {
    pub const ALL: [AppCategory; 16] = [
        AppCategory::Coding,
        AppCategory::Terminal,
        AppCategory::Writing,
        AppCategory::Spreadsheet,
        AppCategory::Slides,
        AppCategory::Planning,
        AppCategory::Data,
        AppCategory::Research,
        AppCategory::Reading,
        AppCategory::Design,
        AppCategory::Chat,
        AppCategory::Email,
        AppCategory::Meeting,
        AppCategory::Learning,
        AppCategory::Media,
        AppCategory::Studio,
    ];

    /// Id used in `settings.json`.
    pub fn id(self) -> &'static str {
        match self {
            AppCategory::Coding => "coding",
            AppCategory::Terminal => "terminal",
            AppCategory::Writing => "writing",
            AppCategory::Spreadsheet => "spreadsheet",
            AppCategory::Slides => "slides",
            AppCategory::Planning => "planning",
            AppCategory::Data => "data",
            AppCategory::Research => "research",
            AppCategory::Reading => "reading",
            AppCategory::Design => "design",
            AppCategory::Chat => "chat",
            AppCategory::Email => "email",
            AppCategory::Meeting => "meeting",
            AppCategory::Learning => "learning",
            AppCategory::Media => "media",
            AppCategory::Studio => "studio",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            AppCategory::Coding => "Coding",
            AppCategory::Terminal => "Terminal",
            AppCategory::Writing => "Writing",
            AppCategory::Spreadsheet => "Sheets",
            AppCategory::Slides => "Slides",
            AppCategory::Planning => "Planning",
            AppCategory::Data => "Data",
            AppCategory::Research => "Research",
            AppCategory::Reading => "Reading",
            AppCategory::Design => "Design",
            AppCategory::Chat => "Chat",
            AppCategory::Email => "Email",
            AppCategory::Meeting => "Meeting",
            AppCategory::Learning => "Learning",
            AppCategory::Media => "Media",
            AppCategory::Studio => "Studio",
        }
    }

    pub fn from_id(id: &str) -> Option<AppCategory> {
        let id = id.trim();
        AppCategory::ALL.into_iter().find(|c| c.id().eq_ignore_ascii_case(id))
    }
}

/// A user-named theme: one of the built-in animations with its own colour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EyeTheme {
    pub name: String,
    pub base: AppCategory,
    /// Iris colour, `#rrggbb`.
    #[serde(default)]
    pub tint: Option<String>,
}

/// What the eyes play for the app in front.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppTheme {
    pub cat: AppCategory,
    pub tint: Option<[u8; 3]>,
}

/// How a theme was chosen (shown on the Focus page).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// From the built-in catalog.
    Auto,
    /// A user mapping in `app_themes`.
    User,
}

/// Override value meaning "no theme for this app".
pub const NONE: &str = "none";

/// Executable name (lowercase, no directory, no `.exe`). Version-ish
/// suffixes are kept; [`lookup_exe`] strips `64`/`32` itself.
pub fn exe_stem(path: &str) -> String {
    let name = path.rsplit(['\\', '/']).next().unwrap_or("").trim().to_ascii_lowercase();
    match name.strip_suffix(".exe") {
        Some(s) => s.to_string(),
        None => name,
    }
}

/// `#rrggbb` (or `rrggbb`, or `#rgb`) to RGB.
pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let h = s.trim().trim_start_matches('#');
    if !h.is_ascii() {
        return None;
    }
    let v = |a: &str| u8::from_str_radix(a, 16).ok();
    match h.len() {
        6 => Some([v(&h[0..2])?, v(&h[2..4])?, v(&h[4..6])?]),
        3 => {
            let d = |i: usize| v(&h[i..i + 1]).map(|x| x * 17);
            Some([d(0)?, d(1)?, d(2)?])
        }
        _ => None,
    }
}

/// `hint` occurs in `hay` as whole words (both already lowercase).
fn has_words(hay: &str, hint: &str) -> bool {
    if hint.is_empty() {
        return false;
    }
    let edge = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric());
    hay.match_indices(hint).any(|(i, _)| edge(hay[..i].chars().next_back()) && edge(hay[i + hint.len()..].chars().next()))
}

/// Category for a known executable.
pub fn lookup_exe(stem: &str) -> Option<AppCategory> {
    let find = |s: &str| EXES.iter().find(|(_, es)| es.contains(&s)).map(|&(c, _)| c);
    find(stem).or_else(|| {
        let suf = ["_x64", "-x64", "x64", "_x86", "x86", "64", "32"].into_iter().find(|s| stem.ends_with(s))?;
        find(stem[..stem.len() - suf.len()].trim_end_matches(['_', '-', '.', ' ']))
    })
}

/// Category from words in a window title.
pub fn lookup_title(title: &str) -> Option<AppCategory> {
    let t = title.to_lowercase();
    // The longest phrase wins ("khan academy" over "linear"); ties keep table order.
    let mut best: Option<(usize, AppCategory)> = None;
    for &(c, hints) in TITLE_HINTS {
        for h in hints {
            if best.is_none_or(|(n, _)| h.len() > n) && has_words(&t, h) {
                best = Some((h.len(), c));
            }
        }
    }
    best.map(|(_, c)| c)
}

pub fn is_browser(stem: &str) -> bool {
    BROWSERS.contains(&stem)
}

/// Turn a user mapping value (category id or custom theme name) into a theme.
/// `Ok(None)` means "explicitly no theme"; `Err` means the value is unknown.
fn resolve(value: &str, themes: &[EyeTheme]) -> Result<Option<AppTheme>, ()> {
    let v = value.trim();
    if v.eq_ignore_ascii_case(NONE) {
        return Ok(None);
    }
    if let Some(t) = themes.iter().find(|t| t.name.trim().eq_ignore_ascii_case(v)) {
        return Ok(Some(AppTheme { cat: t.base, tint: t.tint.as_deref().and_then(parse_hex) }));
    }
    AppCategory::from_id(v).map(|cat| Some(AppTheme { cat, tint: None })).ok_or(())
}

/// The theme for the window in front. User mappings win (title ones first,
/// they are more specific), then the catalog.
pub fn classify(exe_path: &str, title: &str, map: &BTreeMap<String, String>, themes: &[EyeTheme]) -> (Option<AppTheme>, Source) {
    let stem = exe_stem(exe_path);
    let low = title.to_lowercase();
    for (k, v) in map {
        if let Some(words) = k.strip_prefix("title:") {
            if has_words(&low, &words.trim().to_lowercase()) {
                if let Ok(t) = resolve(v, themes) {
                    return (t, Source::User);
                }
            }
        }
    }
    if let Some(v) = map.iter().find(|(k, _)| !k.starts_with("title:") && exe_stem(k) == stem).map(|(_, v)| v) {
        if let Ok(t) = resolve(v, themes) {
            return (t, Source::User);
        }
    }
    (auto(&stem, title), Source::Auto)
}

/// Built-in catalog only.
pub fn auto(stem: &str, title: &str) -> Option<AppTheme> {
    let cat = if is_browser(stem) {
        Some(lookup_title(title).unwrap_or(AppCategory::Research))
    } else if HOSTS.contains(&stem) {
        lookup_title(title)
    } else if NEUTRAL.contains(&stem) {
        None
    } else {
        lookup_exe(stem).or_else(|| lookup_title(title))
    };
    cat.map(|cat| AppTheme { cat, tint: None })
}

/// Next value when the user steps the theme for one app on the Focus page:
/// automatic -> every category -> the user's own themes -> none -> automatic.
pub fn cycle(current: Option<&str>, forward: bool, themes: &[EyeTheme]) -> Option<String> {
    let mut seq: Vec<Option<String>> = vec![None];
    seq.extend(AppCategory::ALL.iter().map(|c| Some(c.id().to_string())));
    seq.extend(themes.iter().map(|t| Some(t.name.trim().to_string())));
    seq.push(Some(NONE.to_string()));
    let same = |s: &Option<String>| match (s, current) {
        (None, None) => true,
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b.trim()),
        _ => false,
    };
    let i = seq.iter().position(same).unwrap_or(0);
    let n = seq.len();
    let j = if forward { (i + 1) % n } else { (i + n - 1) % n };
    seq[j].clone()
}

/// Step the mapping for one executable (see [`cycle`]). Any other spelling
/// of the same exe (`Code.exe`, `code`) is folded into one `stem` key.
pub fn step_override(map: &mut BTreeMap<String, String>, stem: &str, forward: bool, themes: &[EyeTheme]) {
    let keys: Vec<String> = map.keys().filter(|k| !k.starts_with("title:") && exe_stem(k) == stem).cloned().collect();
    let cur = keys.first().and_then(|k| map.get(k)).cloned();
    for k in keys {
        map.remove(&k);
    }
    if let Some(v) = cycle(cur.as_deref(), forward, themes) {
        map.insert(stem.to_string(), v);
    }
}

/// The user's value for an executable, if any.
pub fn override_for<'a>(map: &'a BTreeMap<String, String>, stem: &str) -> Option<&'a str> {
    map.iter().find(|(k, _)| !k.starts_with("title:") && exe_stem(k) == stem).map(|(_, v)| v.as_str())
}

/// Browsers: the tab title says what the user is doing.
const BROWSERS: &[&str] = &[
    "chrome",
    "msedge",
    "firefox",
    "brave",
    "opera",
    "opera_gx",
    "vivaldi",
    "arc",
    "zen",
    "librewolf",
    "waterfox",
    "floorp",
    "thorium",
    "chromium",
    "iexplore",
    "browser",
    "whale",
    "sidekick",
    "min",
    "palemoon",
    "seamonkey",
    "duckduckgo",
    "maxthon",
    "yandex",
    "mullvadbrowser",
    "tor",
    "falkon",
    "midori",
    "ladybird",
    "orion",
    "supermium",
    "catsxp",
    "cent",
    "slimjet",
    "comodo_dragon",
    "srware_iron",
    "iron",
    "basilisk",
    "k-meleon",
    "qutebrowser",
];

/// Executables that host other apps: only the title tells.
const HOSTS: &[&str] = &["applicationframehost", "javaw", "java", "electron", "python", "pythonw", "wwahost", "msedgewebview2", "nw", "mshta"];

/// Shell and system windows: no theme even if the title has a keyword.
const NEUTRAL: &[&str] =
    &["explorer", "searchhost", "searchapp", "startmenuexperiencehost", "shellexperiencehost", "taskmgr", "systemsettings", "lockapp", "teardrop", "control", "mmc", "regedit"];

/// Category -> executable names. Lowercase, no `.exe`; trailing `64`/`32`
/// are stripped before a second lookup, so `idea64` finds `idea`.
pub const EXES: &[(AppCategory, &[&str])] = {
    use AppCategory::*;
    &[
        (
            Coding,
            &[
                "code", "code - insiders", "codium", "vscodium", "cursor", "windsurf", "zed", "trae", "kiro", "void", "positron", "devenv", "blend", "idea",
                "pycharm", "webstorm", "phpstorm", "rubymine", "goland", "clion", "rider", "rustrover", "fleet", "aqua", "studio", "android studio", "eclipse",
                "netbeans", "sublime_text", "notepad++", "atom", "brackets", "lapce", "helix", "hx", "nvim", "nvim-qt", "neovide", "gvim", "vim", "emacs", "runemacs",
                "kate", "geany", "codeblocks", "devcpp", "qtcreator", "xamarin", "unity", "unityhub", "unrealeditor", "ue4editor", "godot", "gamemaker",
                "arduino ide", "arduino", "thonny", "mu-editor", "spyder", "bluej", "greenfoot", "processing", "lazarus", "delphi", "bds", "ultraedit", "uedit",
                "editplus", "textpad", "pspad", "github desktop", "githubdesktop", "gitkraken", "sourcetree", "fork", "smartgit", "tortoisegitproc", "gitextensions",
                "sublime_merge", "postman", "insomnia", "bruno", "hoppscotch", "httpie", "docker desktop", "podman desktop", "lens", "x64dbg", "ollydbg", "ida",
                "ghidrarun", "wireshark", "fiddler", "cheatengine",
            ],
        ),
        (
            Terminal,
            &[
                "windowsterminal", "wt", "cmd", "powershell", "pwsh", "powershell_ise", "conhost", "openconsole", "wezterm-gui", "wezterm", "alacritty", "kitty",
                "hyper", "tabby", "warp", "wave", "waveterm", "rio", "ghostty", "mintty", "cmder", "conemu", "conemu64", "fluent terminal", "putty", "kitty_portable",
                "mobaxterm", "mremoteng", "termius", "securecrt", "xshell", "winscp", "filezilla", "bash", "wsl", "wslhost", "ubuntu", "debian", "kali", "git-bash",
            ],
        ),
        (
            Writing,
            &[
                "winword", "wordpad", "write", "notepad", "soffice.bin", "swriter", "wps", "abiword", "scrivener", "ulysses", "typora", "marktext", "ghostwriter",
                "zettlr", "obsidian", "logseq", "joplin", "notion", "anytype", "capacities", "craft", "bear", "simplenote", "standard notes", "onenote", "onenoteim",
                "evernote", "notable", "trilium", "siyuan", "affine", "reflect", "tana", "roam", "remnote", "heptabase", "focuswriter", "wordgrinder", "manuskript",
                "bibisco", "novelwriter", "lyx", "texstudio", "texmaker", "texworks", "winedt", "grammarly", "hemingway editor", "prowritingaid", "final draft",
                "fade in", "trelby", "highland", "calmly writer", "iawriter", "ia writer", "sticky notes", "microsoft.notes",
            ],
        ),
        (
            Spreadsheet,
            &[
                "excel", "scalc", "et", "gnumeric", "smartsheet", "airtable", "grist", "quattro", "modern csv", "tablecruncher", "csved", "quickbooks", "qbw",
                "qbw32", "gnucash", "moneymanagerex", "ynab",
            ],
        ),
        (
            Slides,
            &[
                "powerpnt", "simpress", "wpp", "keynote", "prezi", "pitch", "gamma", "beautiful.ai", "mentimeter", "slido", "sozi", "impress", "ppttc",
            ],
        ),
        (
            Planning,
            &[
                "outlookcal", "hxcalendarappimm", "todo", "microsoft to do", "todoist", "ticktick", "things", "any.do", "trello", "asana", "linear", "clickup",
                "monday", "jira", "youtrack", "height", "plane", "basecamp", "wrike", "teamwork", "msproject", "winproj", "projectlibre", "ganttproject", "omniplan",
                "planner", "akiflow", "sunsama", "motion", "amie", "morgen", "notion calendar", "cron", "fantastical", "miro", "mural", "whimsical", "xmind",
                "mindmanager", "freemind", "freeplane", "mindnode", "simplemind", "visio", "drawio", "draw.io", "lucidchart", "yed", "excalidraw", "tldraw",
                "whiteboard", "microsoftwhiteboard", "loop", "habitica", "toggl track", "toggltrack", "clockify", "rescuetime", "super productivity",
                "superproductivity",
            ],
        ),
        (
            Data,
            &[
                "jupyter", "jupyter-lab", "jupyterlab", "anaconda-navigator", "rstudio", "rgui", "rterm", "matlab", "octave", "octave-gui", "scilab", "mathematica",
                "wolframnb", "maple", "maplew", "stata", "statase", "statamp", "spss", "stats", "sas", "jmp", "minitab", "eviews", "jasp", "jamovi", "orange",
                "knime", "rapidminer", "weka", "tableau", "tableau public", "pbidesktop", "powerbi", "qlik sense desktop", "qlikview", "metabase", "datagrip",
                "dbeaver", "ssms", "azuredatastudio", "heidisql", "mysqlworkbench", "pgadmin4", "navicat", "tableplus", "beekeeper studio", "beekeeper-studio",
                "dbbrowser for sqlite", "db browser for sqlite", "sqlitestudio", "mongodbcompass", "mongodb compass", "studio 3t", "redisinsight", "toad",
                "sqldeveloper", "sqlyog", "dataspell", "qgis", "qgis-bin", "qgis-ltr-bin", "arcgispro", "arcmap", "origin64", "graphpad prism", "prism", "labview",
                "geogebra", "desmos",
            ],
        ),
        (
            Research,
            &[
                "zotero", "mendeley", "mendeley reference manager", "endnote", "jabref", "citavi", "papers", "readcube papers", "paperpile", "docear",
                "connected papers", "devonthink", "everything", "listary", "perplexity", "chatgpt", "claude", "copilot", "gemini", "dictionary", "goldendict",
                "artha", "kiwix-desktop", "encyclopaedia britannica",
            ],
        ),
        (
            Reading,
            &[
                "acrord32", "acrobat", "foxitpdfreader", "foxitreader", "foxitphantompdf", "foxitpdfeditor", "sumatrapdf", "pdfxedit", "pdfxcview", "okular",
                "evince", "mupdf", "nitropdf", "nitro_pdf", "pdfgear", "pdf24", "drawboard pdf", "xodo", "kindle", "calibre", "ebook-viewer", "thorium reader",
                "koodo reader", "freda", "adobe digital editions", "digitaleditions", "bookviser", "kobo", "readwise reader", "pocket", "instapaper", "feedly",
                "fluent reader", "quiterss", "rssowl", "newsflow", "marginnote", "liquidtext", "polar bookshelf", "comicrack", "cdisplayex", "yacreader",
            ],
        ),
        (
            Design,
            &[
                "figma", "figma_agent", "penpot", "sketch", "lunacy", "framer", "adobe xd", "xd", "photoshop", "illustrator", "indesign", "lightroom",
                "lightroomclassic", "adobe substance 3d painter", "substance painter", "dimension", "fresco", "animate", "afdesign", "affinity designer", "designer",
                "photo", "affinity photo", "publisher", "affinity publisher", "affinity", "mspub", "scribus", "quarkxpress", "coreldrw", "corelpp", "painter", "gimp",
                "gimp-2.10", "gimp-3.0", "krita", "inkscape", "paintdotnet", "mspaint", "paint", "paint 3d", "pinta", "photopea", "clipstudiopaint",
                "clip studio paint", "paintstorm", "rebelle", "artrage", "mypaint", "sai", "sai2", "medibang paint pro", "medibangpaintpro", "firealpaca", "aseprite",
                "pyxel edit", "pixelorama", "libresprite", "canva", "vectornator", "linearity curve", "gravit designer", "vectr", "boxy-svg", "darktable",
                "rawtherapee", "capture one", "captureone", "dxophotolab", "luminar neo", "on1 photo raw", "blender", "cinema 4d", "3dsmax", "maya", "houdini",
                "houdinifx", "zbrush", "modo", "rhino", "sketchup", "fusion360", "acad", "autocad", "revit", "archicad", "solidworks", "sldworks", "inventor",
                "freecad", "onshape", "librecad", "kicad", "eeschema", "pcbnew", "easyeda", "magicavoxel", "vectorworks", "lumion", "enscape", "twinmotion",
                "marmoset toolbag", "spline", "protopie", "axure rp", "axure", "balsamiq", "pencil", "fontforge", "glyphs", "fontlab", "coolors", "pure ref",
                "pureref", "eagle",
            ],
        ),
        (
            Chat,
            &[
                "slack", "discord", "discordptb", "discordcanary", "vesktop", "telegram", "ayugram", "whatsapp", "whatsapp.root", "signal", "element", "riot",
                "mattermost", "rocket.chat", "zulip", "messenger", "caprine", "wechat", "weixin", "qq", "tim", "line", "kakaotalk", "viber", "threema", "wire",
                "session", "beeper", "ferdium", "rambox", "franz", "station", "pidgin", "hexchat", "mirc", "revolt", "guilded", "chatwork", "dingtalk", "feishu",
                "lark", "workplace chat", "google chat", "phonelink", "phoneexperiencehost", "yourphone",
            ],
        ),
        (
            Email,
            &[
                "outlook", "olk", "hxoutlook", "thunderbird", "betterbird", "mailspring", "em client", "mailclient", "postbox", "the bat!", "thebat", "mailbird",
                "spark", "spark desktop", "canary mail", "airmail", "newton", "shift", "kiwi for gmail", "wmail", "wavebox", "superhuman", "hey", "proton mail",
                "protonmail", "tutanota", "tuta mail", "claws-mail", "sylpheed", "evolution", "kmail", "winmail", "hxmail", "front", "missive",
            ],
        ),
        (
            Meeting,
            &[
                "zoom", "cpthost", "ms-teams", "teams", "webex", "ciscowebexstart", "webexmta", "atmgr", "skype", "lync", "gotomeeting", "g2mcomm", "goto",
                "bluejeans", "ringcentral", "ringcentral video", "jitsi meet", "whereby", "around", "tuple", "pop", "gather", "butter", "livestorm", "dialpad",
                "zoho meeting", "lifesize", "vmware horizon client", "anydesk", "teamviewer", "rustdesk", "parsec", "mstsc", "msrdc", "tencent meeting", "wemeetapp",
                "voov", "lark meetings", "loom", "mmhmm",
            ],
        ),
        (
            Learning,
            &[
                "anki", "mnemosyne", "supermemo", "quizlet", "duolingo", "babbel", "busuu", "memrise", "rosetta stone", "rosettastone", "lingq", "pimsleur",
                "khan academy", "coursera", "udemy", "edx", "skillshare", "linkedin learning", "pluralsight", "brilliant", "datacamp", "codecademy", "sololearn",
                "mimo", "scratch", "scratch 3", "scratch desktop", "snap", "tinkercad", "minecraft education", "minecraft.education", "kahoot", "classdojo", "moodle",
                "moodledesktop", "blackboard", "canvas student", "schoology", "google classroom", "onenote class notebook", "speechify", "typingmaster", "typing.com",
                "klavaro", "tipp10", "ankiapp", "stellarium", "celestia", "photomath", "wolfram alpha", "mathway", "symbolab", "musescore", "musescore4", "synthesia",
                "yousician", "simply piano", "chessbase", "lichess",
            ],
        ),
        (
            Media,
            &[
                "spotify", "music.ui", "applemusic", "itunes", "tidal", "deezer", "amazon music", "amazonmusic", "youtube music", "youtubemusic",
                "youtube music desktop app", "qobuz", "pandora", "soundcloud", "audible", "pocket casts", "pocketcasts", "foobar2000", "musicbee", "aimp", "winamp",
                "clementine", "strawberry", "dopamine", "mediamonkey", "cider", "nuclear", "tauon", "vlc", "mpc-hc", "mpc-be", "mpv", "potplayer", "potplayermini",
                "kmplayer", "gom", "smplayer", "wmplayer", "video.ui", "microsoft.media.player", "mediaplayer", "kodi", "plex", "plex htpc", "jellyfin media player",
                "jellyfinmediaplayer", "emby theater", "stremio", "netflix", "disney+", "prime video", "primevideo", "apple tv", "appletv", "hulu", "crunchyroll",
                "twitch", "freetube", "popcorn-time", "photos", "microsoft.photos", "irfanview", "imageglass", "xnview", "xnviewmp", "honeyview",
                "faststone image viewer", "fsviewer", "nomacs", "qimgv",
            ],
        ),
        (
            Studio,
            &[
                "premiere pro", "adobe premiere pro", "premiere", "afterfx", "adobe audition", "audition", "adobe media encoder", "resolve", "davinci resolve",
                "fusion", "vegas", "vegas pro", "vegas200", "vegas210", "vegas220", "hitfilm", "filmora", "wondershare filmora", "camtasia", "camtasiastudio",
                "capcut", "clipchamp", "shotcut", "kdenlive", "openshot-qt", "olive-editor", "lightworks", "avid media composer", "powerdirector", "pdr",
                "movavi video editor", "handbrake", "avidemux", "losslesscut", "natron", "nuke", "obs64", "obs", "streamlabs obs", "streamlabs desktop",
                "xsplit.core", "xsplit broadcaster", "prism live studio", "audacity", "ocenaudio", "reaper", "ableton live 12 suite", "ableton live 11 suite",
                "ableton live", "ableton", "fl", "fl studio", "fl64", "cubase", "cubase13", "cubase14", "nuendo", "studio one", "bitwig studio", "protools",
                "pro tools", "cakewalk", "sonar", "lmms", "ardour", "reason", "renoise", "waveform", "mixcraft", "bandlab", "izotope rx", "rx", "sound forge",
                "wavelab", "adobe character animator", "vtube studio", "voicemeeter", "voicemeeterpro", "descript", "riverside", "spline studio", "toonboom harmony",
                "harmony", "moho", "opentoonz", "synfig", "pencil2d", "krita animation", "dragonframe", "sharex", "screentogif", "snagit", "snagiteditor",
            ],
        ),
    ]
};

/// Category -> words in a window title. Matched as whole words; the longest
/// matching phrase wins.
pub const TITLE_HINTS: &[(AppCategory, &[&str])] = {
    use AppCategory::*;
    &[
        (
            Meeting,
            &[
                "google meet", "meet.google.com", "zoom meeting", "zoom workplace", "microsoft teams meeting", "meeting in", "webex", "jitsi meet", "whereby",
            ],
        ),
        (
            Writing,
            &[
                "google docs", "google keep", "microsoft word", "onenote", "libreoffice writer", "onlyoffice", "zoho writer", "dropbox paper", "quip", "coda",
                "notion", "obsidian", "hackmd", "stackedit", "dillinger", "overleaf", "grammarly", "hemingway", "wordpress", "substack",
            ],
        ),
        (
            Spreadsheet,
            &[
                "google sheets", "microsoft excel", "excel", "libreoffice calc", "zoho sheet", "smartsheet", "airtable",
            ],
        ),
        (
            Slides,
            &[
                "google slides", "microsoft powerpoint", "powerpoint", "libreoffice impress", "zoho show", "pitch", "gamma", "prezi", "tome", "beautiful.ai",
                "mentimeter", "slido",
            ],
        ),
        (
            Planning,
            &[
                "google forms", "google calendar", "google drive", "microsoft loop", "microsoft planner", "microsoft to do", "jira", "confluence", "trello", "asana",
                "linear", "clickup", "monday.com", "todoist", "ticktick", "basecamp", "wrike", "youtrack", "miro", "mural", "figjam", "whimsical", "excalidraw",
                "tldraw", "lucidchart", "draw.io", "diagrams.net", "calendar", "cal.com", "calendly", "toggl",
            ],
        ),
        (
            Learning,
            &[
                "google classroom", "khan academy", "coursera", "udemy", "edx", "skillshare", "linkedin learning", "pluralsight", "brilliant", "datacamp",
                "codecademy", "freecodecamp", "the odin project", "w3schools", "duolingo", "memrise", "babbel", "busuu", "quizlet", "anki", "kahoot", "moodle",
                "canvas lms", "instructure", "blackboard", "schoology", "brightspace", "d2l", "mit opencourseware", "open university", "chegg", "studocu",
                "course hero", "photomath", "mathway", "symbolab", "lesson", "lecture", "tutorial", "course", "homework", "quiz",
            ],
        ),
        (
            Research,
            &[
                "google scholar", "wikipedia", "wiktionary", "arxiv", "pubmed", "jstor", "researchgate", "semantic scholar", "sciencedirect", "springer",
                "ieee xplore", "acm digital library", "ssrn", "elicit", "consensus", "connected papers", "zotero", "perplexity", "chatgpt", "claude", "gemini",
                "copilot", "phind", "kagi", "duckduckgo", "google search", "bing", "britannica", "wolframalpha",
            ],
        ),
        (
            Data,
            &[
                "google colab", "colab", "looker studio", "jupyter", "jupyterlab", "kaggle", "hugging face", "weights & biases", "tableau", "power bi", "looker",
                "metabase", "grafana", "kibana", "superset", "redash", "observable", "deepnote", "databricks", "snowflake", "bigquery", "google analytics",
                "mixpanel", "amplitude", "posthog", "desmos", "geogebra", "wolfram", "rstudio", "posit cloud",
            ],
        ),
        (
            Email,
            &[
                "outlook", "gmail", "inbox", "outlook.com", "yahoo mail", "proton mail", "protonmail", "fastmail", "hey", "superhuman", "zoho mail", "icloud mail",
                "mail", "compose",
            ],
        ),
        (
            Reading,
            &[
                "medium", ".pdf", "pdf", ".epub", "kindle", "goodreads", "project gutenberg", "standard ebooks", "internet archive", "pocket", "instapaper",
                "readwise", "feedly", "inoreader", "hacker news", "the verge", "new york times", "the guardian", "bbc news", "blog", "webtoon", "mangadex",
            ],
        ),
        (
            Design,
            &[
                "canva", "figma", "penpot", "framer", "dribbble", "behance", "pinterest", "adobe express", "photopea", "pixlr", "vectr", "coolors", "spline",
                "tinkercad", "onshape", "sketchfab", "photoshop", "illustrator", "blender", "krita", "inkscape", "gimp", "procreate", "fontshare", "google fonts",
                "unsplash",
            ],
        ),
        (
            Coding,
            &[
                "visual studio code", "vs code", "github.dev", "vscode.dev", "github", "gitlab", "bitbucket", "codeberg", "gitea", "pull request", "stack overflow",
                "stackoverflow", "stack exchange", "mdn web docs", "docs.rs", "crates.io", "npm", "pypi", "codepen", "codesandbox", "stackblitz", "replit", "glitch",
                "jsfiddle", "leetcode", "hackerrank", "codewars", "exercism", "advent of code", "rust playground", "go playground", "godbolt", "compiler explorer",
                "vercel", "netlify", "cloudflare", "aws management console", "azure portal", "google cloud console", "firebase", "supabase", "sentry", "datadog",
                "bolt.new", "lovable", "claude code", "intellij idea", "pycharm", "android studio", "eclipse ide",
            ],
        ),
        (
            Terminal,
            &[
                "terminal", "command prompt", "windows powershell", "powershell", "ssh",
            ],
        ),
        (
            Chat,
            &[
                "slack", "discord", "whatsapp", "telegram", "messenger", "microsoft teams", "teams", "google chat", "mattermost", "rocket.chat", "zulip", "instagram",
                "linkedin", "messages", "chat",
            ],
        ),
        (
            Studio,
            &[
                "capcut", "clipchamp", "descript", "riverside", "bandlab", "soundtrap", "kapwing", "veed", "youtube studio", "twitch dashboard", "stream manager",
                "premiere pro", "after effects", "davinci resolve", "audacity",
            ],
        ),
        (
            Media,
            &[
                "youtube", "netflix", "prime video", "disney+", "hulu", "hbo max", "apple tv", "crunchyroll", "twitch", "vimeo", "spotify", "soundcloud",
                "apple music", "youtube music", "tidal", "deezer", "bandcamp", "pandora", "audible", "podcast", "tiktok", "reddit", "twitter", "bluesky", "mastodon",
                "threads", "facebook", "photos",
            ],
        ),
    ]
};

#[cfg(test)]
mod tests {
    use super::*;

    fn cat(exe: &str, title: &str) -> Option<AppCategory> {
        classify(exe, title, &BTreeMap::new(), &[]).0.map(|t| t.cat)
    }

    #[test]
    fn popular_apps_by_exe() {
        use AppCategory::*;
        assert_eq!(cat("C:\\Users\\a\\AppData\\Local\\Programs\\Microsoft VS Code\\Code.exe", "main.rs - teardrop"), Some(Coding));
        assert_eq!(cat("C:\\Program Files\\JetBrains\\IntelliJ IDEA\\bin\\idea64.exe", "x"), Some(Coding));
        assert_eq!(cat("WindowsTerminal.exe", "pwsh"), Some(Terminal));
        assert_eq!(cat("WINWORD.EXE", "Report.docx - Word"), Some(Writing));
        assert_eq!(cat("EXCEL.EXE", "Budget.xlsx - Excel"), Some(Spreadsheet));
        assert_eq!(cat("POWERPNT.EXE", "Deck.pptx"), Some(Slides));
        assert_eq!(cat("Figma.exe", "Untitled"), Some(Design));
        assert_eq!(cat("slack.exe", "general"), Some(Chat));
        assert_eq!(cat("thunderbird.exe", "Inbox"), Some(Email));
        assert_eq!(cat("Zoom.exe", "Zoom"), Some(Meeting));
        assert_eq!(cat("anki.exe", "Deck"), Some(Learning));
        assert_eq!(cat("vlc.exe", "movie.mkv"), Some(Media));
        assert_eq!(cat("obs64.exe", "OBS 30"), Some(Studio));
        assert_eq!(cat("SumatraPDF.exe", "paper.pdf"), Some(Reading));
        assert_eq!(cat("rstudio.exe", "analysis.R"), Some(Data));
        assert_eq!(cat("Todoist.exe", "Today"), Some(Planning));
        assert_eq!(cat("zotero.exe", "My Library"), Some(Research));
    }

    #[test]
    fn lesser_known_apps_too() {
        use AppCategory::*;
        assert_eq!(cat("neovide.exe", ""), Some(Coding));
        assert_eq!(cat("wezterm-gui.exe", ""), Some(Terminal));
        assert_eq!(cat("Zettlr.exe", ""), Some(Writing));
        assert_eq!(cat("gnumeric.exe", ""), Some(Spreadsheet));
        assert_eq!(cat("aseprite.exe", ""), Some(Design));
        assert_eq!(cat("jamovi.exe", ""), Some(Data));
        assert_eq!(cat("Mailspring.exe", ""), Some(Email));
        assert_eq!(cat("Mnemosyne.exe", ""), Some(Learning));
        assert_eq!(cat("foobar2000.exe", ""), Some(Media));
        assert_eq!(cat("reaper.exe", ""), Some(Studio));
        assert_eq!(cat("koodo reader.exe", ""), Some(Reading));
        assert_eq!(cat("freeplane.exe", ""), Some(Planning));
    }

    #[test]
    fn browser_tabs_read_the_title() {
        use AppCategory::*;
        assert_eq!(cat("chrome.exe", "Q3 plan - Google Docs - Google Chrome"), Some(Writing));
        assert_eq!(cat("msedge.exe", "Budget - Google Sheets and 3 more pages - Work - Microsoft Edge"), Some(Spreadsheet));
        assert_eq!(cat("firefox.exe", "alg0rix/teardrop: a living notch — Mozilla Firefox"), Some(Research));
        assert_eq!(cat("firefox.exe", "Pull requests · alg0rix/teardrop · GitHub — Mozilla Firefox"), Some(Coding));
        assert_eq!(cat("brave.exe", "Inbox (3) - me@example.com - Gmail"), Some(Email));
        assert_eq!(cat("chrome.exe", "Meet - abc-defg-hij - Google Meet"), Some(Meeting));
        assert_eq!(cat("chrome.exe", "Lofi beats - YouTube"), Some(Media));
        assert_eq!(cat("chrome.exe", "Linear algebra | Khan Academy"), Some(Learning));
        assert_eq!(cat("vivaldi.exe", "Untitled - Figma"), Some(Design));
        assert_eq!(cat("chrome.exe", "Rust (programming language) - Wikipedia"), Some(Research));
        assert_eq!(cat("chrome.exe", "Some random page"), Some(Research));
    }

    #[test]
    fn whole_words_only() {
        // "canvas" is not Canva, "wordle" is not Word.
        assert_eq!(lookup_title("drawing on a canvas"), None);
        assert_eq!(lookup_title("mailbox rental"), None);
        assert_eq!(lookup_title("Untitled - Canva"), Some(AppCategory::Design));
        assert!(has_words("a - word", "word"));
        assert!(!has_words("wordle", "word"));
    }

    #[test]
    fn shell_and_hosts() {
        assert_eq!(cat("explorer.exe", "zoom"), None);
        assert_eq!(cat("ApplicationFrameHost.exe", "Calculator"), None);
        assert_eq!(cat("ApplicationFrameHost.exe", "Inbox - Mail"), Some(AppCategory::Email));
        assert_eq!(cat("unknown-thing.exe", "Untitled"), None);
    }

    #[test]
    fn user_mappings_and_themes_win() {
        let mut map = BTreeMap::new();
        map.insert("Code.exe".to_string(), "writing".to_string());
        map.insert("title:jira".to_string(), "Sprint".to_string());
        map.insert("vlc".to_string(), "none".to_string());
        map.insert("slack".to_string(), "bogus".to_string());
        let themes = vec![EyeTheme { name: "Sprint".into(), base: AppCategory::Planning, tint: Some("#22c55e".into()) }];
        let (t, src) = classify("C:\\x\\code.exe", "notes.md", &map, &themes);
        assert_eq!((t.map(|t| t.cat), src), (Some(AppCategory::Writing), Source::User));
        let (t, _) = classify("chrome.exe", "TD-12 - Jira", &map, &themes);
        assert_eq!(t, Some(AppTheme { cat: AppCategory::Planning, tint: Some([0x22, 0xc5, 0x5e]) }));
        assert_eq!(classify("vlc.exe", "a.mkv", &map, &themes), (None, Source::User));
        // Unknown values fall back to the catalog.
        assert_eq!(classify("slack.exe", "x", &map, &themes).0.map(|t| t.cat), Some(AppCategory::Chat));
    }

    #[test]
    fn cycle_walks_every_category_then_themes_then_none() {
        let themes = vec![EyeTheme { name: "Sprint".into(), base: AppCategory::Planning, tint: None }];
        let n = AppCategory::ALL.len();
        let mut v = None;
        let mut seen = Vec::new();
        for _ in 0..n + 3 {
            v = cycle(v.as_deref(), true, &themes);
            seen.push(v.clone());
        }
        assert_eq!(seen[0].as_deref(), Some("coding"));
        assert_eq!(seen[n].as_deref(), Some("Sprint"));
        assert_eq!(seen[n + 1].as_deref(), Some(NONE));
        assert_eq!(seen[n + 2], None);
        assert_eq!(cycle(None, false, &[]).as_deref(), Some(NONE));
        assert_eq!(cycle(Some("Design"), true, &[]).as_deref(), Some("chat"));
    }

    #[test]
    fn stepping_folds_spellings_of_one_exe() {
        let mut map = BTreeMap::new();
        map.insert("Code.exe".to_string(), "writing".to_string());
        map.insert("title:jira".to_string(), "planning".to_string());
        step_override(&mut map, "code", true, &[]);
        assert_eq!(map.get("code").map(String::as_str), Some("spreadsheet"));
        assert!(!map.contains_key("Code.exe") && map.contains_key("title:jira"));
        assert_eq!(override_for(&map, "code"), Some("spreadsheet"));
        // Back past "automatic" removes the key.
        map.insert("vlc".to_string(), "coding".to_string());
        step_override(&mut map, "vlc", false, &[]);
        assert_eq!(override_for(&map, "vlc"), None);
    }

    #[test]
    fn ids_round_trip_and_hex() {
        for c in AppCategory::ALL {
            assert_eq!(AppCategory::from_id(c.id()), Some(c));
            let j = serde_json::to_string(&c).unwrap();
            assert_eq!(j, format!("\"{}\"", c.id()));
        }
        assert_eq!(parse_hex("#fff"), Some([255, 255, 255]));
        assert_eq!(parse_hex("10a0ff"), Some([0x10, 0xa0, 0xff]));
        assert_eq!(parse_hex("nope"), None);
    }

    #[test]
    fn catalog_has_no_duplicate_exes() {
        let mut seen = std::collections::HashSet::new();
        for e in EXES.iter().flat_map(|(_, es)| es.iter()) {
            assert!(seen.insert(*e), "duplicate exe {e}");
            assert_eq!(*e, e.to_ascii_lowercase());
        }
        assert!(seen.len() > 500);
    }
}
