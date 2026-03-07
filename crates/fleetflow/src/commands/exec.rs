use crate::docker;
use crate::utils;
use colored::Colorize;

pub async fn handle(
    config: &fleetflow_core::Flow,
    stage: Option<String>,
    service: String,
    command: Vec<String>,
    interactive: bool,
    tty: bool,
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

    // シェル起動時は自動的に interactive + tty を有効にする
    let is_shell = cmd.len() == 1
        && (cmd[0] == "/bin/sh" || cmd[0] == "/bin/bash" || cmd[0] == "sh" || cmd[0] == "bash");
    let use_interactive = interactive || is_shell;
    let use_tty = tty || is_shell;

    println!(
        "{}",
        format!("コンテナ '{}' でコマンドを実行中...", container_name).green()
    );
    println!("コマンド: {}", cmd.join(" ").cyan());
    println!();

    // Docker接続
    let docker_conn = docker::init_docker_with_error_handling().await?;

    // Docker exec を作成
    use bollard::exec::{CreateExecOptions, StartExecResults};
    let exec_config = CreateExecOptions {
        cmd: Some(cmd),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        attach_stdin: Some(use_interactive),
        tty: Some(use_tty),
        ..Default::default()
    };

    let message = docker_conn
        .create_exec(&container_name, exec_config)
        .await?;

    if use_interactive {
        // インタラクティブモード: stdin/stdout を双方向接続
        match docker_conn
            .start_exec(&message.id, None::<bollard::exec::StartExecOptions>)
            .await?
        {
            StartExecResults::Attached {
                mut output,
                mut input,
            } => {
                use futures_util::stream::StreamExt;
                use tokio::io::{AsyncReadExt, AsyncWriteExt};

                // TTY の場合は raw mode を有効にする
                if use_tty {
                    crossterm::terminal::enable_raw_mode()?;
                }

                // stdin → container input のタスク
                let stdin_handle = tokio::spawn(async move {
                    let mut stdin = tokio::io::stdin();
                    let mut buf = [0u8; 1024];
                    loop {
                        match stdin.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                if input.write_all(&buf[..n]).await.is_err() {
                                    break;
                                }
                                if input.flush().await.is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                });

                // container output → stdout のタスク
                while let Some(Ok(output_chunk)) = output.next().await {
                    let bytes = output_chunk.into_bytes();
                    let mut stdout = std::io::stdout();
                    use std::io::Write;
                    stdout.write_all(&bytes)?;
                    stdout.flush()?;
                }

                stdin_handle.abort();

                // TTY の場合は raw mode を無効にする
                if use_tty {
                    crossterm::terminal::disable_raw_mode()?;
                }
            }
            StartExecResults::Detached => {
                println!("{}", "コマンドをデタッチモードで実行しました".green());
            }
        }
    } else {
        // 非インタラクティブモード: 出力のみ表示（従来の動作）
        use bollard::exec::StartExecOptions;
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
