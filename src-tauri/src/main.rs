// 在 Windows 上隐藏控制台窗口（debug 和 release 均生效，
// 否则 dev 模式下运行会弹出一个黑色 cmd 窗口）
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    md_editor_lib::run()
}
