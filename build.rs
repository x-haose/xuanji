//! 构建脚本：仅当目标为 Windows 时，把北斗 `.ico` 嵌成 exe 资源（Explorer/任务栏图标）。
//! 在 host（可能是 mac 交叉编译）上运行，故按 `CARGO_CFG_TARGET_OS` 判定，非致命失败仅告警。

fn main() {
    println!("cargo:rerun-if-changed=assets/xuanji.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/xuanji.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=exe 图标资源嵌入失败（不影响功能）: {e}");
        }
    }
}
