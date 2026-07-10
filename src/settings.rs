//! 配置单一数据源。`project.json`（编入二进制）声明 schema 与默认值；
//! 用户改动仅存 overrides，`resolved = 默认 ⊕ overrides`。加参数只改 project.json。

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{Map, Value};

/// project.json 随二进制编入，作 schema/默认值的唯一来源。
const PROJECT_JSON: &str = include_str!("../web/project.json");

/// 一个属性的最小 schema（校验+默认所需）。
struct Prop {
    /// slider / bool / color / combo / file / directory
    kind: String,
    default: Value,
}

/// 落盘结构：仅存用户改动与预设，向后兼容靠 serde default。
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Persisted {
    #[serde(default = "one")]
    version: u32,
    /// 全局覆盖：适用于所有屏（未被某屏单独覆盖时）。
    #[serde(default)]
    overrides: Map<String, Value>,
    /// 每屏覆盖：屏 id → 该屏专属覆盖，优先级高于全局。
    #[serde(default)]
    screens: BTreeMap<String, Map<String, Value>>,
    #[serde(default)]
    presets: BTreeMap<String, Map<String, Value>>,
}

fn one() -> u32 {
    1
}

/// 配置状态：schema（只读，来自 project.json）+ 用户覆盖 + 预设。
pub struct Settings {
    schema: BTreeMap<String, Prop>,
    persisted: Persisted,
    path: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("未知配置项: {0}")]
    UnknownKey(String),
    #[error("配置项 {key} 类型不符（期望 {kind}）")]
    TypeMismatch { key: String, kind: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl Settings {
    /// 跨平台配置文件路径（mac: ~/Library/Application Support/dev.xuanji.app/settings.json）。
    pub fn config_path() -> PathBuf {
        directories::ProjectDirs::from("dev", "xuanji", "app")
            .map(|d| d.config_dir().join("settings.json"))
            .unwrap_or_else(|| PathBuf::from("xuanji-settings.json"))
    }

    /// 载入 schema（恒成功）+ 已存配置（缺失/损坏则空）。
    pub fn load(path: PathBuf) -> Self {
        let schema = parse_schema();
        let persisted = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self {
            schema,
            persisted,
            path,
        }
    }

    /// 某屏的最终配置：默认 ⊕ 全局 overrides ⊕ 该屏 overrides（后者优先）。
    /// `screen = None` 取全局层（供托盘/无屏上下文用）。
    pub fn resolved_for(&self, screen: Option<&str>) -> Map<String, Value> {
        let per = screen.and_then(|s| self.persisted.screens.get(s));
        self.schema
            .iter()
            .map(|(k, p)| {
                let v = per
                    .and_then(|m| m.get(k))
                    .or_else(|| self.persisted.overrides.get(k))
                    .cloned()
                    .unwrap_or_else(|| p.default.clone());
                (k.clone(), v)
            })
            .collect()
    }

    /// 全局层最终配置（默认 ⊕ 全局 overrides）。
    pub fn resolved(&self) -> Map<String, Value> {
        self.resolved_for(None)
    }

    /// 已存预设名列表。
    pub fn presets(&self) -> Vec<String> {
        self.persisted.presets.keys().cloned().collect()
    }

    /// bgtype 下拉的 (value, label) 列表——托盘与设置窗共用同一份（来自 project.json），
    /// 保证两处背景菜单永远一致。
    pub fn bgtype_options() -> Vec<(String, String)> {
        let root: Value = serde_json::from_str(PROJECT_JSON).unwrap_or(Value::Null);
        root.get("general")
            .and_then(|g| g.get("properties"))
            .and_then(|p| p.get("bgtype"))
            .and_then(|b| b.get("options"))
            .and_then(|o| o.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|o| {
                        let v = o.get("value")?.as_str()?.to_string();
                        let l = o.get("label")?.as_str()?.to_string();
                        Some((v, l))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 校验并写覆盖项。
    /// `screen = None`（托盘 / 设置窗「所有屏」）= **所有屏统一**：写全局层（等于默认则移除以精简），
    /// 并清掉各屏对该键的专属覆盖——「改所有屏」就该压过任何某屏专属，杜绝静默劫持切不动。
    /// `screen = Some(id)`（设置窗选了某屏）= 只写该屏专属层（始终显式存，即便等于默认——
    /// 用户明确要此屏用该值，不因等于 schema 默认而回落全局）。
    pub fn set(
        &mut self,
        key: &str,
        value: Value,
        screen: Option<&str>,
    ) -> Result<(), SettingsError> {
        let prop = self
            .schema
            .get(key)
            .ok_or_else(|| SettingsError::UnknownKey(key.into()))?;
        if !type_ok(&prop.kind, &value) {
            return Err(SettingsError::TypeMismatch {
                key: key.into(),
                kind: prop.kind.clone(),
            });
        }
        match screen {
            None => {
                for m in self.persisted.screens.values_mut() {
                    m.remove(key);
                }
                self.persisted.screens.retain(|_, m| !m.is_empty());
                if value == prop.default {
                    self.persisted.overrides.remove(key);
                } else {
                    self.persisted.overrides.insert(key.into(), value);
                }
            }
            Some(id) => {
                self.persisted
                    .screens
                    .entry(id.to_string())
                    .or_default()
                    .insert(key.into(), value);
            }
        }
        Ok(())
    }

    /// 写盘（自动建父目录）。
    pub fn save(&self) -> Result<(), SettingsError> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&self.path, serde_json::to_string_pretty(&self.persisted)?)?;
        Ok(())
    }

    /// 把当前 overrides 存为命名预设。
    pub fn save_preset(&mut self, name: &str) {
        self.persisted
            .presets
            .insert(name.into(), self.persisted.overrides.clone());
    }

    /// 应用命名预设（覆盖当前 overrides）；不存在返回 false。
    pub fn apply_preset(&mut self, name: &str) -> bool {
        match self.persisted.presets.get(name).cloned() {
            Some(o) => {
                self.persisted.overrides = o;
                true
            }
            None => false,
        }
    }

    /// 删除命名预设。
    pub fn delete_preset(&mut self, name: &str) {
        self.persisted.presets.remove(name);
    }

    /// 清空全部覆盖（全局层 + 每屏层），恢复默认。
    pub fn reset(&mut self) {
        self.persisted.overrides.clear();
        self.persisted.screens.clear();
    }
}

/// 解析 project.json 的 `general.properties` → key→Prop。
fn parse_schema() -> BTreeMap<String, Prop> {
    let root: Value = serde_json::from_str(PROJECT_JSON).expect("project.json 应为合法 JSON");
    let mut m = BTreeMap::new();
    if let Some(props) = root
        .get("general")
        .and_then(|g| g.get("properties"))
        .and_then(|p| p.as_object())
    {
        for (k, v) in props {
            if let (Some(kind), Some(default)) =
                (v.get("type").and_then(|t| t.as_str()), v.get("value"))
            {
                m.insert(
                    k.clone(),
                    Prop {
                        kind: kind.to_string(),
                        default: default.clone(),
                    },
                );
            }
        }
    }
    m
}

/// JSON 值类型是否匹配属性声明类型。
fn type_ok(kind: &str, v: &Value) -> bool {
    match kind {
        "slider" => v.is_number(),
        "bool" => v.is_boolean(),
        "color" | "combo" | "file" | "directory" => v.is_string(),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn temp_settings(name: &str) -> Settings {
        let path = std::env::temp_dir().join(format!("xuanji-test-{name}.json"));
        let _ = std::fs::remove_file(&path);
        Settings::load(path)
    }

    #[test]
    fn schema_covers_all_declared_props() {
        let s = temp_settings("schema");
        assert_eq!(s.schema.len(), 35);
        assert!(s.schema.contains_key("particlecount"));
    }

    #[test]
    fn resolved_falls_back_to_defaults() {
        let s = temp_settings("defaults");
        let r = s.resolved();
        assert_eq!(r.get("bgtype"), Some(&json!("color")));
        assert_eq!(r.get("particlecount"), Some(&json!(80)));
    }

    #[test]
    fn set_rejects_unknown_key() {
        let mut s = temp_settings("unknown");
        assert!(matches!(
            s.set("nope", json!(1), None),
            Err(SettingsError::UnknownKey(_))
        ));
    }

    #[test]
    fn set_rejects_type_mismatch() {
        let mut s = temp_settings("mismatch");
        assert!(matches!(
            s.set("particlecount", json!(true), None),
            Err(SettingsError::TypeMismatch { .. })
        ));
    }

    #[test]
    fn set_then_resolved_reflects_change() {
        let mut s = temp_settings("change");
        s.set("particlecount", json!(120), None).unwrap();
        assert_eq!(s.resolved().get("particlecount"), Some(&json!(120)));
    }

    #[test]
    fn set_to_default_clears_override() {
        let mut s = temp_settings("cleardefault");
        s.set("particlecount", json!(120), None).unwrap();
        s.set("particlecount", json!(80), None).unwrap();
        assert!(!s.persisted.overrides.contains_key("particlecount"));
    }

    #[test]
    fn save_load_roundtrip() {
        let path = std::env::temp_dir().join("xuanji-test-roundtrip.json");
        let _ = std::fs::remove_file(&path);
        {
            let mut s = Settings::load(path.clone());
            s.set("darktheme", Value::Bool(false), None).unwrap();
            s.save_preset("夜间");
            s.save().unwrap();
        }
        let s2 = Settings::load(path);
        assert_eq!(s2.resolved().get("darktheme"), Some(&Value::Bool(false)));
        assert_eq!(s2.presets(), vec!["夜间".to_string()]);
    }

    #[test]
    fn preset_apply_swaps_overrides() {
        let mut s = temp_settings("preset");
        s.set("particlecount", json!(200), None).unwrap();
        s.save_preset("满屏");
        s.reset();
        assert!(s.apply_preset("满屏"));
        assert_eq!(s.resolved().get("particlecount"), Some(&json!(200)));
    }

    #[test]
    fn per_screen_override_wins_over_global() {
        let mut s = temp_settings("perscreen");
        s.set("bgtype", json!("ink"), None).unwrap(); // 全局
        s.set("bgtype", json!("thunder"), Some("HDMI-1")).unwrap(); // 某屏
        assert_eq!(s.resolved_for(None).get("bgtype"), Some(&json!("ink")));
        assert_eq!(
            s.resolved_for(Some("HDMI-1")).get("bgtype"),
            Some(&json!("thunder"))
        );
        // 未覆盖的屏回落到全局。
        assert_eq!(
            s.resolved_for(Some("other")).get("bgtype"),
            Some(&json!("ink"))
        );
    }

    #[test]
    fn global_set_clears_per_screen_override() {
        let mut s = temp_settings("globalclears");
        s.set("bgtype", json!("ink"), Some("A")).unwrap(); // 屏A 专属
        assert_eq!(s.resolved_for(Some("A")).get("bgtype"), Some(&json!("ink")));
        s.set("bgtype", json!("thunder"), None).unwrap(); // 托盘/所有屏统一
        // 屏A 专属被清，跟随全局；空屏条目一并移除。
        assert_eq!(
            s.resolved_for(Some("A")).get("bgtype"),
            Some(&json!("thunder"))
        );
        assert!(s.persisted.screens.is_empty());
    }

    #[test]
    fn per_screen_keeps_value_equal_to_default() {
        let mut s = temp_settings("perscreendefault");
        s.set("bgtype", json!("ink"), None).unwrap(); // 全局改
        s.set("bgtype", json!("color"), Some("A")).unwrap(); // 该屏钉回默认值
        // 即便等于 schema 默认，也不回落到全局 ink。
        assert_eq!(
            s.resolved_for(Some("A")).get("bgtype"),
            Some(&json!("color"))
        );
    }
}
