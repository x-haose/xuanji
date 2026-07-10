//! 设置窗 → 壳 的 IPC 消息（JSON，经 wry `ipc_handler`）。
//! 壳 → 设置窗 方向用 `evaluate_script` 调 `window.__xuanji*`（见 main.rs）。

use serde_json::Value;

/// 设置窗发来的控制消息。
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "cmd", rename_all = "lowercase")]
pub enum Msg {
    /// 设置窗加载完，请求注入当前配置。
    Ready,
    /// 修改一个参数。
    Set { key: String, value: Value },
    /// 请求原生文件/目录选择框。
    Pick { key: String, kind: PickKind },
    /// 存当前 overrides 为命名预设。
    SavePreset { name: String },
    /// 应用命名预设。
    ApplyPreset { name: String },
    /// 删除命名预设。
    DeletePreset { name: String },
    /// 恢复全部默认。
    Reset,
}

/// 文件选择类型。
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PickKind {
    File,
    Directory,
}

/// 解析设置窗 postMessage 载荷；非法一律 None（丢弃，不 panic）。
pub fn parse(body: &str) -> Option<Msg> {
    serde_json::from_str(body).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_set() {
        let m = parse(r#"{"cmd":"set","key":"particlecount","value":120}"#).unwrap();
        assert!(matches!(m, Msg::Set { key, .. } if key == "particlecount"));
    }

    #[test]
    fn parses_pick_dir() {
        let m = parse(r#"{"cmd":"pick","key":"bgdir","kind":"directory"}"#).unwrap();
        assert!(matches!(
            m,
            Msg::Pick {
                kind: PickKind::Directory,
                ..
            }
        ));
    }

    #[test]
    fn parses_ready_and_preset() {
        assert!(matches!(parse(r#"{"cmd":"ready"}"#).unwrap(), Msg::Ready));
        assert!(matches!(
            parse(r#"{"cmd":"applypreset","name":"夜间"}"#).unwrap(),
            Msg::ApplyPreset { .. }
        ));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("not json").is_none());
        assert!(parse(r#"{"cmd":"unknown"}"#).is_none());
    }
}
