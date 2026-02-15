use crate::docker;
use crate::utils;
use colored::Colorize;

pub async fn handle(
    config: &fleetflow_core::Flow,
    stage: Option<String>,
    service: String,
    command: Vec<String>,
) -> anyhow::Result<()> {
    let stage_name = utils::determine_stage_name(stage, config)?;

    // サービスの存在確認
    if !config.services.contains_key(&service) {
        return Err(anyhow::anyhow!(
            "サービス '{}' が見つかりません\n利用可能なサービス: {}",
            service,
            config
                .services
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    // コンテナ名
    let container_name = format!("{}-{}-{}", config.name, stage_name, service);

    // コマンドが省略された場合は /bin/sh
    let cmd: Vec<String> = if command.is_empty() {
        vec!["/bin/sh".to_string()]
    } else {
        command
    };

    println!(
        "{}",
        format!("コンテナ '{}' でコマンドを実行中...", container_name).green()
    );
    println!("コマンド: {}", cmd.join(" ").cyan());
    println!();

    // Docker接続
    let docker_conn = docker::init_docker_with_error_handling().await?;

    // Docker exec を作成
    use bollard::exec::{CreateExecOptions, StartExecOptions, StartExecResults};
    let exec_config = CreateExecOptions {
        cmd: Some(cmd),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        ..Default::default()
    };

    let message = docker_conn
        .create_exec(&container_name, exec_config)
        .await?;

    // 開始・出力処理
    let start_config = StartExecOptions {
        ..Default::default()
    };
    match docker_conn
        .start_exec(&message.id, Some(start_config))
        .await?
    {
        StartExecResults::Attached { mut output, .. } => {
            use bollard::container::LogOutput;
            use futures_util::stream::StreamExt;

            while let Some(msg) = output.next().await {
                match msg {
                    Ok(log_output) => match log_output {
                        LogOutput::StdOut { message } => {
                            let text = String::from_utf8_lossy(&message);
                            print!("{}", text);
                        }
                        LogOutput::StdErr { message } => {
                            let text = String::from_utf8_lossy(&message);
                            eprint!("{}", text);
                        }
                        LogOutput::Console { message } => {
                            let text = String::from_utf8_lossy(&message);
                            print!("{}", text);
                        }
                        LogOutput::StdIn { .. } => {}
                    },
                    Err(e) => {
                        eprintln!("{}", format!("Docker exec エラー: {}", e).red());
                        break;
                    }
                }
            }
        }
        StartExecResults::Detached => {
            println!("{}", "コマンドをデタッチモードで実行しました".green());
        }
    }

    // 終了コードの取得
    let inspect = docker_conn.inspect_exec(&message.id).await?;
    if let Some(exit_code) = inspect.exit_code
        && exit_code != 0
    {
        std::process::exit(exit_code as i32);
    }

    Ok(())
}
