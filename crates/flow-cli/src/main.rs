use clap::{Parser, Subcommand};
use colored::Colorize;

#[derive(Parser)]
#[command(name = "unison")]
#[command(about = "Docker Composeよりシンプル。KDLで書く、次世代の環境構築ツール。", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 環境を起動
    Up {
        /// 環境名を指定
        #[arg(short, long)]
        environment: Option<String>,
    },
    /// 環境を停止
    Down {
        /// 環境名を指定
        #[arg(short, long)]
        environment: Option<String>,
    },
    /// 設定を検証
    Validate,
    /// バージョン情報を表示
    Version,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Up { environment } => {
            println!("{}", "環境を起動中...".green());
            if let Some(env) = environment {
                println!("環境: {}", env.cyan());
            }
            // TODO: 実装
        }
        Commands::Down { environment } => {
            println!("{}", "環境を停止中...".yellow());
            if let Some(env) = environment {
                println!("環境: {}", env.cyan());
            }
            // TODO: 実装
        }
        Commands::Validate => {
            println!("{}", "設定を検証中...".blue());
            // TODO: 実装
        }
        Commands::Version => {
            println!("unison {}", env!("CARGO_PKG_VERSION"));
        }
    }

    Ok(())
}
