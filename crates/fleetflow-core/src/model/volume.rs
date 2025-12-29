//! ボリューム定義

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use unison_kdl::{KdlDeserialize, KdlSerialize};

/// ボリューム定義
///
/// KDL形式：
/// ```kdl
/// volume "./data/postgres" "/var/lib/postgresql/data" read_only=#false
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, KdlDeserialize, KdlSerialize)]
#[kdl(name = "volume")]
pub struct Volume {
    /// ホスト側パス（第1引数）
    #[kdl(argument)]
    pub host: PathBuf,
    /// コンテナ側パス（第2引数）
    #[kdl(argument)]
    pub container: PathBuf,
    /// 読み取り専用（プロパティ）
    #[serde(default)]
    #[kdl(property, default)]
    pub read_only: bool,
}
