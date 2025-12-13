use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContainerError {
    #[error(
        "Dockerに接続できません: {0}\n\nヒント:\n  • Dockerが起動しているか確認してください\n  • OrbStackまたはDocker Desktopがインストールされているか確認してください"
    )]
    DockerConnectionFailed(String),

    #[error("コンテナ '{container}' が見つかりません")]
    ContainerNotFound { container: String },

    #[error("コンテナ '{container}' は既に起動しています")]
    ContainerAlreadyRunning { container: String },

    #[error("コンテナ '{container}' は既に停止しています")]
    ContainerAlreadyStopped { container: String },

    #[error(
        "イメージ '{image}' が見つかりません\n\nヒント:\n  • イメージ名とタグを確認してください\n  • docker pull {image} でイメージをダウンロードしてください"
    )]
    ImageNotFound { image: String },

    #[error(
        "ポート {port} は既に使用されています\n\nヒント:\n  • 別のポート番号を使用してください\n  • 既存のコンテナを停止してください: flow down --stage={stage}"
    )]
    PortAlreadyInUse { port: u16, stage: String },

    #[error("Docker APIエラー: {0}")]
    DockerApiError(String),

    #[error("設定エラー: {0}")]
    ConfigError(String),

    #[error(
        "サービス '{service}' の準備完了を待機中にタイムアウトしました（{max_retries}回リトライ）\n\nヒント:\n  • 依存サービスが正常に起動しているか確認してください\n  • wait_forのmax_retriesを増やしてみてください"
    )]
    ServiceWaitTimeout { service: String, max_retries: u32 },
}

impl From<bollard::errors::Error> for ContainerError {
    fn from(err: bollard::errors::Error) -> Self {
        match &err {
            bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            } => {
                // 404エラーは呼び出し側で適切に処理されるべき
                ContainerError::DockerApiError(err.to_string())
            }
            bollard::errors::Error::DockerResponseServerError {
                status_code: 409, ..
            } => {
                // 409エラーも呼び出し側で処理
                ContainerError::DockerApiError(err.to_string())
            }
            _ => {
                // 接続エラーの可能性をチェック
                let err_str = err.to_string();
                if err_str.contains("Connection refused")
                    || err_str.contains("No such file or directory")
                {
                    ContainerError::DockerConnectionFailed(err_str)
                } else {
                    ContainerError::DockerApiError(err_str)
                }
            }
        }
    }
}

pub type Result<T> = std::result::Result<T, ContainerError>;
