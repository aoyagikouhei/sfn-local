//! 起動時に `SFN_LOCAL_STATE_MACHINES_DIR` の `<名前>.asl.json` をステートマシンとして登録する。
//! 1 つでも読めなければ起動を止める（黙って飛ばすと、実行したときに StateMachineDoesNotExist になって原因が遠い）。

use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::arn::{NAME_MAX_CHARS, has_forbidden_name_char, state_machine_arn};
use crate::asl;
use crate::config::Config;
use crate::store::{Created, StateMachine, Store};
use crate::time::now_millis;

const SUFFIX: &str = ".asl.json";

/// 読み込んだステートマシンの ARN を名前順で返す。
pub fn load(dir: &Path, config: &Config, store: &Store) -> Result<Vec<String>, String> {
    let prefix = format!("SFN_LOCAL_STATE_MACHINES_DIR: {}", dir.display());
    let entries = fs::read_dir(dir).map_err(|e| format!("{prefix}: 読めません: {e}"))?;

    let mut files: Vec<(String, std::path::PathBuf)> = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|e| format!("{prefix}: 読めません: {e}"))?
            .path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if let Some(name) = file_name.strip_suffix(SUFFIX) {
            files.push((name.to_string(), path.clone()));
        }
    }
    files.sort();

    let mut arns = Vec::new();
    for (name, path) in files {
        let at = format!("{prefix}: {}", path.display());
        if name.is_empty()
            || name.chars().count() > NAME_MAX_CHARS
            || has_forbidden_name_char(&name)
        {
            return Err(format!("{at}: ステートマシン名に使えないファイル名です"));
        }
        let text = fs::read_to_string(&path).map_err(|e| format!("{at}: 読めません: {e}"))?;
        let definition = asl::parse(&text).map_err(|e| format!("{at}: {e}"))?;
        let arn = state_machine_arn(&config.region, &config.account_id, &name);
        let state_machine = StateMachine {
            arn: arn.clone(),
            role_arn: format!("arn:aws:iam::{}:role/sfn-local", config.account_id),
            name,
            definition_text: text,
            definition: Arc::new(definition),
            kind: "STANDARD".into(),
            creation_ms: now_millis(),
        };
        match store.create_state_machine(state_machine) {
            Created::New(_) => arns.push(arn),
            Created::Existing(_) | Created::Conflict => {
                return Err(format!("{at}: 同じ名前のステートマシンがすでにあります"));
            }
        }
    }
    Ok(arns)
}
