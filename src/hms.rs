//! Optional local Bambu Studio HMS descriptions. No vendor data is bundled.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::OnceLock,
};

type Tables = HashMap<String, HashMap<String, String>>;

fn directory() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("BAMBUTOP_HMS_DIR") {
        return Some(path.into());
    }
    Some(dirs::config_dir()?.join("BambuStudio").join("hms"))
}

fn parse_table(bytes: &[u8]) -> Option<HashMap<String, String>> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let rows = value.pointer("/data/device_hms/en")?.as_array()?;
    Some(
        rows.iter()
            .filter_map(|row| {
                let key = row["ecode"].as_str()?;
                let text = row["intro"].as_str()?;
                if key.len() != 16
                    || !key.bytes().all(|b| b.is_ascii_hexdigit())
                    || text.trim().is_empty()
                {
                    return None;
                }
                Some((key.to_ascii_uppercase(), text.to_owned()))
            })
            .collect(),
    )
}

fn load_tables(dir: &Path) -> Tables {
    let mut tables = Tables::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name != "hms_en.json" && !(name.starts_with("hms_en_") && name.ends_with(".json")) {
                continue;
            }
            if let Ok(bytes) = std::fs::read(entry.path()) {
                if let Some(table) = parse_table(&bytes) {
                    tables.insert(name, table);
                }
            }
        }
    }
    tables
}

fn lookup<'a>(tables: &'a Tables, model: &str, attr: u32, code: u32) -> Option<&'a str> {
    use crate::printer::*;
    let prefix = match model {
        MODEL_P2S => "22E",
        MODEL_P1S => "01P",
        MODEL_P1P => "01S",
        MODEL_X1C => "00M",
        MODEL_X1E => "03W",
        MODEL_A1 => "039",
        MODEL_A1_MINI => "030",
        MODEL_H2C => "31B",
        MODEL_H2S => "093",
        MODEL_H2D => "094",
        MODEL_H2D_PRO => "239",
        _ => "",
    };
    let key = format!("{attr:08X}{code:08X}");
    tables
        .get(&format!("hms_en_{prefix}.json"))
        .and_then(|t| t.get(&key))
        .or_else(|| tables.get("hms_en.json").and_then(|t| t.get(&key)))
        .map(String::as_str)
}

pub fn description(model: &str, attr: u32, code: u32) -> Option<String> {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    let tables = TABLES.get_or_init(|| directory().map(|p| load_tables(&p)).unwrap_or_default());
    lookup(tables, model, attr, code).map(str::to_owned)
}

/// Link unknown identifiers without guessing from a partial code.
pub fn fallback(attr: u32, code: u32) -> String {
    format!(
        "HMS {attr:08X}{code:08X}: https://wiki.bambulab.com/en/hms/{:04x}_{:04x}_{:04x}_{:04x}",
        attr >> 16,
        attr & 0xffff,
        code >> 16,
        code & 0xffff
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_valid_entries_only() {
        let bytes = br#"{"data":{"device_hms":{"en":[
            {"ecode":"abcdef0000030002","intro":"Maintenance"},
            {"ecode":"0501040000030002","intro":" "},
            {"ecode":"invalid","intro":"Bad"}, {}
        ]}}}"#;
        let table = parse_table(bytes).unwrap();
        assert_eq!(table.len(), 1);
        assert_eq!(table["ABCDEF0000030002"], "Maintenance");
        assert!(parse_table(b"not json").is_none());
        assert!(parse_table(b"{}").is_none());
    }
    #[test]
    fn prefers_model_table_then_generic() {
        let key = "0501040000030002".to_owned();
        let mut tables = Tables::new();
        tables.insert(
            "hms_en.json".into(),
            HashMap::from([(key.clone(), "Generic".into())]),
        );
        tables.insert(
            "hms_en_22E.json".into(),
            HashMap::from([(key, "P2S".into())]),
        );
        assert_eq!(
            lookup(&tables, crate::printer::MODEL_P2S, 0x05010400, 0x00030002),
            Some("P2S")
        );
        assert_eq!(
            lookup(&tables, crate::printer::MODEL_A1, 0x05010400, 0x00030002),
            Some("Generic")
        );
        assert_eq!(lookup(&tables, "unknown", 0, 0), None);
    }
    #[test]
    fn missing_directory_is_optional() {
        assert!(load_tables(
            &std::env::temp_dir().join(format!("bambutop-missing-hms-{}", std::process::id()))
        )
        .is_empty());
    }
    #[test]
    fn unknown_identifier_links_to_full_code() {
        let text = fallback(0x05010400, 0x00030002);
        assert!(text.contains("0501040000030002"));
        assert!(text.ends_with("0501_0400_0003_0002"));
    }
}
