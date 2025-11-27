//! ボリューム定義

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// ボリューム定義
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    pub host: PathBuf,
    pub container: PathBuf,
    #[serde(default)]
    pub read_only: bool,
}
