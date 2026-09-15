//! Windows 文件关联（.md / .markdown）
//!
//! 设计要点：
//! - 全部写入 **HKCU**（当前用户），不需要管理员权限，也不会影响其他账户；
//! - ProgID 与安装包保持一致，避免"打开方式"里出现两个同名条目：
//!   NSIS 安装包用 `bundle.fileAssociations[].name`（`MDEditor.md`），
//!   MSI 安装包用 `<productName>.<ext>`（`md-editor.md`，Tauri 的 WiX 模板写死的）。
//!   运行时用 [`resolve_prog_id`] 复用已经指向本程序的那个 ProgID，
//!   都没有时再新建 `MDEditor.md`；
//! - Windows 10/11 的 `UserChoice` 带校验哈希，第三方程序无法静默改写"默认应用"，
//!   因此 [`try_set_default`] 只做"能改则改"，改不了时由前端引导用户走系统 UI。
#![cfg(windows)]

use std::ffi::c_void;
use std::os::windows::process::CommandExt;
use std::process::Command;
use winreg::enums::{HKEY_CLASSES_ROOT, HKEY_CURRENT_USER, REG_NONE};
use winreg::{RegKey, RegValue};

/// 默认 ProgID：与 tauri.conf.json 的 `fileAssociations[0].name` 一致（NSIS 用这个）
pub const DEFAULT_PROG_ID: &str = "MDEditor.md";
/// Tauri WiX/MSI 模板写死的 ProgID：`<productName>.<ext>`
const MSI_PROG_ID: &str = "md-editor.md";
/// `HKCU\Software\RegisteredApplications` 下的值名，同时用于 `ms-settings` 深链接
const REG_APP_NAME: &str = "md-editor";
const CAPABILITIES_KEY: &str = r"Software\md-editor\Capabilities";
const APP_DISPLAY_NAME: &str = "Markdown 编辑器";
const APP_DESCRIPTION: &str = "支持实时预览的 Markdown 编辑器";
/// 参与关联的扩展名
pub const ASSOCIATED_EXTS: [&str; 2] = ["md", "markdown"];

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const SHCNE_ASSOCCHANGED: i32 = 0x0800_0000;
const SHCNF_IDLIST: u32 = 0x0000;

#[link(name = "shell32")]
unsafe extern "system" {
    fn SHChangeNotify(event: i32, flags: u32, item1: *const c_void, item2: *const c_void);
}

/// 关联状态快照（回传给前端展示）
#[derive(serde::Serialize, Clone)]
pub struct AssociationStatus {
    /// 本程序 exe 路径
    pub exe: String,
    /// 运行时注册（ProgID + Capabilities + OpenWithProgids）是否已就位
    pub registered: bool,
    /// .md 当前是否默认由本程序打开
    pub is_default: bool,
    /// .md 当前的默认 ProgID（如 `VSCode.md`），未设置时为 null
    pub current_prog_id: Option<String>,
    /// 系统里存在 UserChoice 且不是本程序——此时无法静默改默认，需要用户确认
    pub user_choice_locked: bool,
}

/// 让资源管理器立即刷新图标与关联缓存；否则改动要重启 explorer 才生效
pub fn notify_shell() {
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, std::ptr::null(), std::ptr::null()) }
}

fn exe_path() -> Result<String, String> {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| format!("获取程序路径失败: {e}"))
}

fn command_for(exe: &str) -> String {
    format!("\"{exe}\" \"%1\"")
}

fn io_err(e: std::io::Error) -> String {
    format!("写入注册表失败: {e}")
}

fn read_str(key: &RegKey, name: &str) -> Option<String> {
    key.get_value::<String, _>(name)
        .ok()
        .filter(|s| !s.is_empty())
}

/// 从 `"C:\path\app.exe" "%1"` 这类命令串里取出可执行文件路径
fn command_exe(command: &str) -> &str {
    let trimmed = command.trim();
    if let Some(rest) = trimmed.strip_prefix('"') {
        rest.split('"').next().unwrap_or("")
    } else {
        trimmed.split_whitespace().next().unwrap_or("")
    }
}

/// 读取某个 ProgID 的 shell\open\command（HKCU 与 HKLM 都会被 HKCR 合并进来）
fn prog_id_command(prog_id: &str) -> Option<String> {
    RegKey::predef(HKEY_CLASSES_ROOT)
        .open_subkey(format!(r"{prog_id}\shell\open\command"))
        .ok()
        .and_then(|key| read_str(&key, ""))
}

/// 该 ProgID 打开的是不是当前这个 exe
fn prog_id_targets_exe(prog_id: &str, exe: &str) -> bool {
    prog_id_command(prog_id)
        .map(|cmd| command_exe(&cmd).eq_ignore_ascii_case(exe))
        .unwrap_or(false)
}

/// 复用安装包已经写好的 ProgID；都没有时用默认的 `MDEditor.md`
fn resolve_prog_id(exe: &str) -> String {
    for candidate in [DEFAULT_PROG_ID, MSI_PROG_ID] {
        if prog_id_targets_exe(candidate, exe) {
            return candidate.to_string();
        }
    }
    DEFAULT_PROG_ID.to_string()
}

/// 读取某个扩展名当前生效的 ProgID。
/// 返回 `(ProgID, 是否来自 UserChoice)`；没有 UserChoice 时退回 `HKEY_CLASSES_ROOT`。
fn current_prog_id(ext: &str) -> (Option<String>, bool) {
    let user_choice = format!(
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.{ext}\UserChoice"
    );
    if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(&user_choice) {
        if let Some(prog) = read_str(&key, "ProgId") {
            return (Some(prog), true);
        }
    }
    let prog = RegKey::predef(HKEY_CLASSES_ROOT)
        .open_subkey(format!(".{ext}"))
        .ok()
        .and_then(|k| read_str(&k, ""));
    (prog, false)
}

/// 运行时注册是否已就位（安装包只写 ProgID + 扩展名，
/// Capabilities / OpenWithProgids / Applications 这些由本程序补）
fn is_registered(exe: &str) -> bool {
    if !prog_id_targets_exe(&resolve_prog_id(exe), exe) {
        return false;
    }
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey(CAPABILITIES_KEY).is_ok()
        && hkcu.open_subkey(r"Software\RegisteredApplications").is_ok()
        && hkcu.open_subkey(r"Software\Classes\.md\OpenWithProgids").is_ok()
}

/// 把 .md / .markdown 关联到本程序（幂等，可反复调用）
pub fn register() -> Result<(), String> {
    let exe = exe_path()?;
    let prog_id = resolve_prog_id(&exe);
    let command = command_for(&exe);
    let icon = format!("\"{exe}\",0");
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    // ---------- 1. ProgID：资源管理器双击文件时走这里 ----------
    let (prog, _) = hkcu
        .create_subkey(format!(r"Software\Classes\{prog_id}"))
        .map_err(io_err)?;
    prog.set_value("", &APP_DISPLAY_NAME).map_err(io_err)?;
    prog.set_value("FriendlyTypeName", &APP_DISPLAY_NAME)
        .map_err(io_err)?;
    // FTA_OpenIsSafe：避免每次双击 .md 都弹"打开文件-安全警告"
    prog.set_value("EditFlags", &0x0001_0000u32).map_err(io_err)?;
    prog.create_subkey("DefaultIcon")
        .map_err(io_err)?
        .0
        .set_value("", &icon)
        .map_err(io_err)?;
    let (shell, _) = prog.create_subkey("shell").map_err(io_err)?;
    shell.set_value("", &"open").map_err(io_err)?;
    let (shell_open, _) = shell.create_subkey("open").map_err(io_err)?;
    shell_open
        .set_value("", &format!("用{APP_DISPLAY_NAME}打开"))
        .map_err(io_err)?;
    shell_open
        .create_subkey("command")
        .map_err(io_err)?
        .0
        .set_value("", &command)
        .map_err(io_err)?;

    for ext in ASSOCIATED_EXTS {
        let (ext_key, _) = hkcu
            .create_subkey(format!(r"Software\Classes\.{ext}"))
            .map_err(io_err)?;
        // ---------- 2. 扩展名默认 ProgID ----------
        // 只在未设置或已指向本程序时写入；已经属于别的程序就不抢，
        // 免得覆盖用户在"默认应用"里做过的选择。
        let existing = read_str(&ext_key, "").unwrap_or_default();
        if existing.is_empty() || prog_id_targets_exe(&existing, &exe) {
            ext_key.set_value("", &prog_id).map_err(io_err)?;
        }
        // ---------- 3. 加入右键"打开方式"候选 ----------
        let (open_with, _) = ext_key.create_subkey("OpenWithProgids").map_err(io_err)?;
        open_with
            .set_raw_value(
                prog_id.as_str(),
                &RegValue {
                    bytes: Vec::new(),
                    vtype: REG_NONE,
                },
            )
            .map_err(io_err)?;
    }

    // ---------- 4. 系统"默认应用"里的条目 + ms-settings 深链接 ----------
    let (cap, _) = hkcu.create_subkey(CAPABILITIES_KEY).map_err(io_err)?;
    cap.set_value("ApplicationName", &APP_DISPLAY_NAME)
        .map_err(io_err)?;
    cap.set_value("ApplicationDescription", &APP_DESCRIPTION)
        .map_err(io_err)?;
    let (file_assoc, _) = cap.create_subkey("FileAssociations").map_err(io_err)?;
    for ext in ASSOCIATED_EXTS {
        file_assoc
            .set_value(format!(".{ext}"), &prog_id)
            .map_err(io_err)?;
    }
    hkcu.create_subkey(r"Software\RegisteredApplications")
        .map_err(io_err)?
        .0
        .set_value(REG_APP_NAME, &CAPABILITIES_KEY)
        .map_err(io_err)?;

    // ---------- 5. "打开方式"对话框中的应用条目 ----------
    let (app_key, _) = hkcu
        .create_subkey(r"Software\Classes\Applications\md-editor.exe")
        .map_err(io_err)?;
    app_key
        .set_value("FriendlyAppName", &APP_DISPLAY_NAME)
        .map_err(io_err)?;
    app_key
        .create_subkey(r"shell\open\command")
        .map_err(io_err)?
        .0
        .set_value("", &command)
        .map_err(io_err)?;
    let (supported, _) = app_key.create_subkey("SupportedTypes").map_err(io_err)?;
    for ext in ASSOCIATED_EXTS {
        supported.set_value(format!(".{ext}"), &"").map_err(io_err)?;
    }

    notify_shell();
    Ok(())
}

/// 尝试把 .md / .markdown 直接设为本程序默认。
/// 返回 `true` 表示已经生效；`false` 表示被系统 UserChoice 挡住，需要用户确认。
pub fn try_set_default() -> Result<bool, String> {
    register()?;
    let exe = exe_path()?;
    let prog_id = resolve_prog_id(&exe);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for ext in ASSOCIATED_EXTS {
        hkcu.create_subkey(format!(r"Software\Classes\.{ext}"))
            .map_err(io_err)?
            .0
            .set_value("", &prog_id)
            .map_err(io_err)?;
    }
    notify_shell();
    Ok(is_default())
}

pub fn is_default() -> bool {
    let Ok(exe) = exe_path() else {
        return false;
    };
    match current_prog_id("md").0 {
        Some(prog) => prog_id_targets_exe(&prog, &exe),
        None => false,
    }
}

pub fn status() -> AssociationStatus {
    let exe = exe_path().unwrap_or_default();
    let (prog, from_user_choice) = current_prog_id("md");
    let is_default = !exe.is_empty()
        && prog
            .as_deref()
            .map(|p| prog_id_targets_exe(p, &exe))
            .unwrap_or(false);
    AssociationStatus {
        registered: !exe.is_empty() && is_registered(&exe),
        is_default,
        current_prog_id: prog,
        user_choice_locked: from_user_choice && !is_default,
        exe,
    }
}

/// 打开 Windows「默认应用」设置页；Win11 会直接定位到本程序
pub fn open_default_apps_settings(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let uri = format!("ms-settings:defaultapps?registeredAppUser={REG_APP_NAME}");
    app.opener()
        .open_url(uri, None::<&str>)
        .map_err(|e| format!("打开系统设置失败: {e}"))
}

/// 调起系统「打开方式」对话框：勾选"始终使用此应用"即可写入正确的 UserChoice
pub fn open_with_dialog() -> Result<(), String> {
    // 该对话框需要一个真实文件路径，这里用临时 .md 文件
    let sample = std::env::temp_dir().join("md-editor-open-with.md");
    if !sample.exists() {
        let _ = std::fs::write(
            &sample,
            "# Markdown 编辑器\n\n勾选下方「始终使用此应用打开 .md 文件」即可设为默认。\n",
        );
    }
    Command::new("rundll32.exe")
        .arg("shell32.dll,OpenAs_RunDLL")
        .arg(&sample)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("打开\"打开方式\"对话框失败: {e}"))?;
    Ok(())
}
