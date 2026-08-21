// MD Editor - Tauri 外壳层
// 负责窗口管理、文件对话框与磁盘读写；Markdown 渲染全部由前端网页完成。
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use tauri_plugin_dialog::DialogExt;

/// 打开文件命令返回的结构：文件路径 + 内容
#[derive(Serialize)]
struct OpenedFile {
    path: String,
    content: String,
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            open_file,
            save_file,
            read_file_at,
            export_file,
            list_dir,
            open_folder
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
