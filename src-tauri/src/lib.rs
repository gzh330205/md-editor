// MD Editor - Tauri 外壳层
// 负责窗口管理、文件对话框与磁盘读写；Markdown 渲染全部由前端网页完成。
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

#[cfg(windows)]
mod file_assoc;

/// 非 Windows 平台的空实现，保证跨平台可编译（关联注册只在 Windows 有意义）
#[cfg(not(windows))]
mod file_assoc {
    #[derive(serde::Serialize, Clone)]
    pub struct AssociationStatus {
        pub exe: String,
        pub registered: bool,
        pub is_default: bool,
        pub current_prog_id: Option<String>,
        pub user_choice_locked: bool,
    }

    pub fn register() -> Result<(), String> {
        Ok(())
    }

    pub fn status() -> AssociationStatus {
        AssociationStatus {
            exe: String::new(),
            registered: false,
            is_default: false,
            current_prog_id: None,
            user_choice_locked: false,
        }
    }

    pub fn try_set_default() -> Result<bool, String> {
        Err("当前系统不支持自动设置默认应用".into())
    }

    pub fn open_default_apps_settings(_app: &tauri::AppHandle) -> Result<(), String> {
        Err("当前系统不支持自动设置默认应用".into())
    }

    pub fn open_with_dialog() -> Result<(), String> {
        Err("当前系统不支持自动设置默认应用".into())
    }
}

/// 打开文件命令返回的结构：文件路径 + 内容
#[derive(Serialize)]
struct OpenedFile {
    path: String,
    content: String,
}

// ---------- 系统"打开方式"传入的文件 ----------

/// 待打开文件的队列。
/// 系统在启动时通过命令行参数传文件，这个队列保证前端尚未挂载监听时也不会丢文件。
#[derive(Default)]
struct PendingFiles(Mutex<Vec<String>>);

/// 参与关联的文本类扩展名（用于过滤命令行参数，避免把开关当成文件名）
const DOC_EXTS: [&str; 6] = ["md", "markdown", "mdown", "mkd", "mdx", "txt"];

fn is_document_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    DOC_EXTS.iter().any(|ext| lower.ends_with(&format!(".{ext}")))
}

/// 从命令行参数里挑出真实存在的文档路径（跳过 argv[0] 与 `-`/`--` 开头的开关）
fn pick_documents<I, S>(args: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    args.into_iter()
        .skip(1)
        .map(|a| a.as_ref().to_string_lossy().into_owned())
        .filter(|a| !a.starts_with('-'))
        .filter(|a| is_document_path(a))
        .filter(|a| std::path::Path::new(a).is_file())
        .collect()
}

/// 把待打开文件放入队列，并通知前端来取
fn queue_files(app: &tauri::AppHandle, paths: Vec<String>) {
    if paths.is_empty() {
        return;
    }
    if let Some(state) = app.try_state::<PendingFiles>() {
        if let Ok(mut queue) = state.0.lock() {
            queue.extend(paths);
        }
    }
    // 事件只当"有新文件"的唤醒信号，数据统一由 take_pending_open_files 取走，
    // 这样即使事件早于前端监听到达（冷启动）也不会丢。
    let _ = app.emit("md-open-files", ());
}

/// 把主窗口拉到前台（从"打开方式"重复启动时）
fn focus_main_window(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}

/// 打开文件对话框并读取文件内容
/// 返回 None 表示用户取消了对话框
#[tauri::command]
fn open_file(app: tauri::AppHandle) -> Result<Option<OpenedFile>, String> {
    let picked = app
        .dialog()
        .file()
        .add_filter("Markdown", &["md", "markdown"])
        .add_filter("纯文本", &["txt"])
        .blocking_pick_file();

    let Some(file) = picked else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|e| e.to_string())?;

    let content = fs::read_to_string(&path)
        .map_err(|e| format!("读取文件失败: {e}"))?;

    Ok(Some(OpenedFile {
        path: path.to_string_lossy().into_owned(),
        content,
    }))
}

/// 保存文件。path 为 None 时弹出"另存为"对话框选择保存位置。
/// 返回 None 表示用户取消了对话框；返回 Some(path) 表示实际写入的路径。
#[tauri::command]
fn save_file(
    app: tauri::AppHandle,
    path: Option<String>,
    content: String,
) -> Result<Option<String>, String> {
    let mut path_buf = match path {
        Some(p) => PathBuf::from(p),
        None => {
            let picked = app
                .dialog()
                .file()
                .add_filter("Markdown", &["md", "markdown"])
                .set_file_name("untitled.md")
                .blocking_save_file();
            let Some(file) = picked else {
                return Ok(None);
            };
            file.into_path().map_err(|e| e.to_string())?
        }
    };

    // 用户没写扩展名时补上 .md
    if path_buf.extension().is_none() {
        path_buf.set_extension("md");
    }

    fs::write(&path_buf, content).map_err(|e| format!("写入文件失败: {e}"))?;

    Ok(Some(path_buf.to_string_lossy().into_owned()))
}

/// 读取指定路径的文件（不弹对话框），用于启动时恢复上次文档
#[tauri::command]
fn read_file_at(path: String) -> Result<String, String> {
    fs::read_to_string(&path).map_err(|e| format!("读取文件失败: {e}"))
}

/// 目录条目（文件树用）
#[derive(Serialize)]
struct DirEntry {
    name: String,
    path: String,
    is_dir: bool,
}

/// 列出目录内容：目录在前、按名称排序
#[tauri::command]
fn list_dir(path: String) -> Result<Vec<DirEntry>, String> {
    let rd = fs::read_dir(&path).map_err(|e| format!("读取目录失败: {e}"))?;
    let mut entries = Vec::new();
    for item in rd.flatten() {
        let Ok(ft) = item.file_type() else { continue };
        entries.push(DirEntry {
            name: item.file_name().to_string_lossy().into_owned(),
            path: item.path().to_string_lossy().into_owned(),
            is_dir: ft.is_dir(),
        });
    }
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

/// 选择文件夹（文件树根目录），返回 None 表示取消
#[tauri::command]
fn open_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let picked = app.dialog().file().blocking_pick_folder();
    let Some(folder) = picked else {
        return Ok(None);
    };
    let path = folder.into_path().map_err(|e| e.to_string())?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// 导出文件：弹出"另存为"对话框并写入任意内容（HTML 导出用）
/// 返回 None 表示用户取消了对话框
#[tauri::command]
fn export_file(
    app: tauri::AppHandle,
    content: String,
    default_name: String,
    filter_name: String,
    extensions: Vec<String>,
) -> Result<Option<String>, String> {
    let exts: Vec<&str> = extensions.iter().map(|s| s.as_str()).collect();
    let picked = app
        .dialog()
        .file()
        .add_filter(filter_name, &exts)
        .set_file_name(default_name)
        .blocking_save_file();
    let Some(file) = picked else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|e| e.to_string())?;
    fs::write(&path, content).map_err(|e| format!("写入文件失败: {e}"))?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// 前端启动时 / 收到 `md-open-files` 事件后调用：取走并清空待打开文件队列
#[tauri::command]
fn take_pending_open_files(state: tauri::State<'_, PendingFiles>) -> Vec<String> {
    match state.0.lock() {
        Ok(mut queue) => std::mem::take(&mut *queue),
        Err(_) => Vec::new(),
    }
}

// ---------- 文件关联（默认用本程序打开 .md） ----------

/// 「设为默认」的结果
#[derive(Serialize)]
struct SetDefaultResult {
    /// 已经真正成为默认程序
    ok: bool,
    /// 被系统 UserChoice 锁定，需要在系统界面里确认
    need_confirm: bool,
    /// 是否已自动拉起系统的「默认应用」设置页
    settings_opened: bool,
}

#[tauri::command]
fn association_status() -> file_assoc::AssociationStatus {
    file_assoc::status()
}

/// 注册（或修复）文件关联，返回最新状态
#[tauri::command]
fn register_file_association() -> Result<file_assoc::AssociationStatus, String> {
    file_assoc::register()?;
    Ok(file_assoc::status())
}

/// 一键设为默认：能静默生效就直接生效，否则打开系统「默认应用」页面让用户确认
#[tauri::command]
fn set_default_editor(app: tauri::AppHandle) -> Result<SetDefaultResult, String> {
    if file_assoc::try_set_default()? {
        return Ok(SetDefaultResult {
            ok: true,
            need_confirm: false,
            settings_opened: false,
        });
    }
    let settings_opened = file_assoc::open_default_apps_settings(&app).is_ok();
    Ok(SetDefaultResult {
        ok: false,
        need_confirm: true,
        settings_opened,
    })
}

/// 直接打开系统「默认应用」设置页
#[tauri::command]
fn open_association_settings(app: tauri::AppHandle) -> Result<(), String> {
    file_assoc::open_default_apps_settings(&app)
}

/// 调起系统「打开方式」对话框（勾选"始终使用"即可设为默认）
#[tauri::command]
fn open_with_dialog() -> Result<(), String> {
    file_assoc::open_with_dialog()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default().manage(PendingFiles::default());

    // 单实例插件要最先注册：从"打开方式"再次启动时，
    // 新进程把参数转交给已运行实例后立即退出。
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            queue_files(app, pick_documents(args));
            focus_main_window(app);
        }));
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();
            // 1) 冷启动时命令行里带的文件（双击 .md、"打开方式"）
            queue_files(&handle, pick_documents(std::env::args_os()));
            // 2) 首次运行或换过安装目录时补一次关联注册（幂等；失败不影响启动）。
            //    开发模式（debug 产物）不自动写：否则会把已安装版本的关联
            //    悄悄改成 target/debug 下的临时 exe，之后双击 .md 就打不开了。
            //    开发时需要的话，在设置里手动点「设为默认 / 修复关联」。
            if !cfg!(debug_assertions) && !file_assoc::status().registered {
                let _ = file_assoc::register();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_file,
            save_file,
            read_file_at,
            export_file,
            list_dir,
            open_folder,
            take_pending_open_files,
            association_status,
            register_file_association,
            set_default_editor,
            open_association_settings,
            open_with_dialog
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, _event| {
            // macOS 通过 RunEvent::Opened 下发文件；Windows / Linux 走命令行参数
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Opened { urls } = _event {
                let paths: Vec<String> = urls
                    .iter()
                    .filter_map(|u| u.to_file_path().ok())
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
                queue_files(_app, paths);
                focus_main_window(_app);
            }
        });
}
