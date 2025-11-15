use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("設定ディレクトリが見つかりません")]
    ConfigDirNotFound,

    #[error(
        "設定ファイルが見つかりません。以下の場所を確認してください:\n\
        - カレントディレクトリ: flow.kdl, flow.local.kdl, .flow.kdl, .flow.local.kdl\n\
        - ./.fleetflow/ ディレクトリ\n\
        - ~/.config/fleetflow/flow.kdl\n\
        または FLOW_CONFIG_PATH 環境変数で直接指定できます"
    )]
    FlowFileNotFound,

    #[error("IO エラー: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ConfigError>;
