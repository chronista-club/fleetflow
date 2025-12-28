//! セットアップコマンドの実装
//!
//! ステージごとの環境セットアップを冪等に実行する。
//! 各ステップの進捗・所要時間を詳細に記録。

use chrono::Local;
use colored::Colorize;
use std::time::{Duration, Instant};

/// セットアップの各ステップ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupStep {
    /// クラウド設定の読み込み
    LoadCloudConfig,
    /// サーバー確認
    CheckServer,
    /// サーバー作成
    CreateServer,
    /// SSH接続確認
    WaitSsh,
    /// ツールインストール（Docker, mise, etc）
    InstallTools,
    /// ディレクトリ作成
    CreateDirectories,
    /// コンテナ起動
    StartContainers,
    /// DB初期化
    InitDatabase,
}

impl SetupStep {
    /// ステップの日本語名
    pub fn name(&self) -> &'static str {
        match self {
            Self::LoadCloudConfig => "クラウド設定読み込み",
            Self::CheckServer => "サーバー確認",
            Self::CreateServer => "サーバー作成",
            Self::WaitSsh => "SSH接続確認",
            Self::InstallTools => "ツールインストール",
            Self::CreateDirectories => "ディレクトリ作成",
            Self::StartContainers => "コンテナ起動",
            Self::InitDatabase => "DB初期化",
        }
    }

    /// ステップのID（--skipで使用）
    pub fn id(&self) -> &'static str {
        match self {
            Self::LoadCloudConfig => "cloud-config",
            Self::CheckServer => "check-server",
            Self::CreateServer => "create-server",
            Self::WaitSsh => "ssh",
            Self::InstallTools => "tools",
            Self::CreateDirectories => "dirs",
            Self::StartContainers => "containers",
            Self::InitDatabase => "db",
        }
    }

    /// localステージで必要なステップか
    pub fn required_for_local(&self) -> bool {
        matches!(
            self,
            Self::CreateDirectories | Self::StartContainers | Self::InitDatabase
        )
    }

    /// remoteステージ用の全ステップ
    pub fn remote_steps() -> Vec<Self> {
        vec![
            Self::LoadCloudConfig,
            Self::CheckServer,
            Self::CreateServer,
            Self::WaitSsh,
            Self::InstallTools,
            Self::CreateDirectories,
            Self::StartContainers,
            Self::InitDatabase,
        ]
    }

    /// localステージ用の全ステップ
    pub fn local_steps() -> Vec<Self> {
        vec![
            Self::CreateDirectories,
            Self::StartContainers,
            Self::InitDatabase,
        ]
    }
}

/// ステップの実行結果
#[derive(Debug, Clone)]
pub enum StepResult {
    /// 成功
    Success {
        duration: Duration,
        message: Option<String>,
    },
    /// スキップ（既に完了済み等）
    Skipped { reason: String },
    /// 失敗
    Failed { error: String, duration: Duration },
    /// リトライ後に成功
    SuccessWithRetry { duration: Duration, retries: u32 },
}

impl StepResult {
    pub fn is_success(&self) -> bool {
        matches!(
            self,
            Self::Success { .. } | Self::SuccessWithRetry { .. } | Self::Skipped { .. }
        )
    }

    pub fn duration(&self) -> Option<Duration> {
        match self {
            Self::Success { duration, .. } => Some(*duration),
            Self::Failed { duration, .. } => Some(*duration),
            Self::SuccessWithRetry { duration, .. } => Some(*duration),
            Self::Skipped { .. } => None,
        }
    }
}

/// セットアップログ出力器
pub struct SetupLogger {
    start_time: Instant,
    step_results: Vec<(SetupStep, StepResult)>,
    current_step: Option<(SetupStep, Instant)>,
}

impl SetupLogger {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            step_results: Vec::new(),
            current_step: None,
        }
    }

    /// ステップ開始をログ出力
    pub fn start_step(&mut self, step: SetupStep) {
        let timestamp = Local::now().format("%H:%M:%S").to_string();
        println!("[{}] {} {}", timestamp.dimmed(), "▶".cyan(), step.name());
        self.current_step = Some((step, Instant::now()));
    }

    /// ステップ成功をログ出力
    pub fn step_success(&mut self, message: Option<&str>) {
        if let Some((step, start)) = self.current_step.take() {
            let duration = start.elapsed();
            let timestamp = Local::now().format("%H:%M:%S").to_string();
            let duration_str = format_duration(duration);

            if let Some(msg) = message {
                println!(
                    "[{}] {} {} ({})",
                    timestamp.dimmed(),
                    "✓".green().bold(),
                    msg,
                    duration_str.dimmed()
                );
            } else {
                println!(
                    "[{}] {} {} 完了 ({})",
                    timestamp.dimmed(),
                    "✓".green().bold(),
                    step.name(),
                    duration_str.dimmed()
                );
            }

            self.step_results.push((
                step,
                StepResult::Success {
                    duration,
                    message: message.map(String::from),
                },
            ));
        }
    }

    /// ステップスキップをログ出力
    pub fn step_skipped(&mut self, reason: &str) {
        if let Some((step, _)) = self.current_step.take() {
            let timestamp = Local::now().format("%H:%M:%S").to_string();
            println!(
                "[{}] {} {} ({})",
                timestamp.dimmed(),
                "⏭".yellow(),
                step.name(),
                reason.dimmed()
            );

            self.step_results.push((
                step,
                StepResult::Skipped {
                    reason: reason.to_string(),
                },
            ));
        }
    }

    /// ステップ失敗をログ出力
    pub fn step_failed(&mut self, error: &str) {
        if let Some((step, start)) = self.current_step.take() {
            let duration = start.elapsed();
            let timestamp = Local::now().format("%H:%M:%S").to_string();

            println!(
                "[{}] {} {}: {}",
                timestamp.dimmed(),
                "✗".red().bold(),
                step.name(),
                error.red()
            );

            self.step_results.push((
                step,
                StepResult::Failed {
                    error: error.to_string(),
                    duration,
                },
            ));
        }
    }

    /// リトライ中のログ出力
    pub fn log_retry(&self, attempt: u32, max_attempts: u32, error: &str) {
        let timestamp = Local::now().format("%H:%M:%S").to_string();
        println!(
            "[{}] {} リトライ {}/{}: {}",
            timestamp.dimmed(),
            "⟳".yellow(),
            attempt,
            max_attempts,
            error.dimmed()
        );
    }

    /// リトライ後の成功をログ出力
    pub fn step_success_with_retry(&mut self, retries: u32, message: Option<&str>) {
        if let Some((step, start)) = self.current_step.take() {
            let duration = start.elapsed();
            let timestamp = Local::now().format("%H:%M:%S").to_string();
            let duration_str = format_duration(duration);

            let msg = message.unwrap_or("完了");
            println!(
                "[{}] {} {} ({}, {} retries)",
                timestamp.dimmed(),
                "✓".green().bold(),
                msg,
                duration_str.dimmed(),
                retries
            );

            self.step_results
                .push((step, StepResult::SuccessWithRetry { duration, retries }));
        }
    }

    /// 詳細メッセージをログ出力
    pub fn log_detail(&self, message: &str) {
        let timestamp = Local::now().format("%H:%M:%S").to_string();
        println!("[{}]   → {}", timestamp.dimmed(), message.cyan());
    }

    /// サマリーを出力
    pub fn print_summary(&self, stage_name: &str) {
        let total_duration = self.start_time.elapsed();
        let total_retries: u32 = self
            .step_results
            .iter()
            .filter_map(|(_, result)| {
                if let StepResult::SuccessWithRetry { retries, .. } = result {
                    Some(*retries)
                } else {
                    None
                }
            })
            .sum();

        let error_count = self
            .step_results
            .iter()
            .filter(|(_, result)| matches!(result, StepResult::Failed { .. }))
            .count();

        let slowest_step = self
            .step_results
            .iter()
            .filter_map(|(step, result)| result.duration().map(|d| (step, d)))
            .max_by_key(|(_, d)| *d);

        println!();
        println!("{}", "═".repeat(44));
        println!("Setup Summary: {}", stage_name.cyan().bold());
        println!("{}", "─".repeat(44));
        println!("Total time:    {}", format_duration(total_duration).green());

        if let Some((step, duration)) = slowest_step {
            println!(
                "Slowest step:  {} ({})",
                step.name(),
                format_duration(duration)
            );
        }

        if total_retries > 0 {
            println!("Retries:       {}", total_retries.to_string().yellow());
        } else {
            println!("Retries:       0");
        }

        if error_count > 0 {
            println!("Errors:        {}", error_count.to_string().red().bold());
        } else {
            println!("Errors:        {}", "0".green());
        }
        println!("{}", "═".repeat(44));
    }

    /// 全ステップが成功したか
    pub fn all_success(&self) -> bool {
        self.step_results
            .iter()
            .all(|(_, result)| result.is_success())
    }
}

impl Default for SetupLogger {
    fn default() -> Self {
        Self::new()
    }
}

/// Duration を読みやすい形式にフォーマット
fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let millis = duration.subsec_millis();

    if total_secs >= 60 {
        let minutes = total_secs / 60;
        let secs = total_secs % 60;
        format!("{}m {}s", minutes, secs)
    } else if total_secs >= 1 {
        format!("{}.{}s", total_secs, millis / 100)
    } else {
        format!("{}ms", millis)
    }
}

/// スキップするステップを解析
pub fn parse_skip_steps(skip_arg: Option<&str>) -> Vec<SetupStep> {
    let Some(skip_str) = skip_arg else {
        return Vec::new();
    };

    skip_str
        .split(',')
        .filter_map(|s| {
            let s = s.trim();
            SetupStep::remote_steps()
                .into_iter()
                .find(|step| step.id() == s)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(Duration::from_millis(50)), "50ms");
        assert_eq!(format_duration(Duration::from_millis(1500)), "1.5s");
        assert_eq!(format_duration(Duration::from_secs(90)), "1m 30s");
    }

    #[test]
    fn test_parse_skip_steps() {
        let steps = parse_skip_steps(Some("ssh,db"));
        assert_eq!(steps.len(), 2);
        assert!(steps.contains(&SetupStep::WaitSsh));
        assert!(steps.contains(&SetupStep::InitDatabase));
    }

    #[test]
    fn test_local_steps() {
        let steps = SetupStep::local_steps();
        assert!(!steps.contains(&SetupStep::CheckServer));
        assert!(steps.contains(&SetupStep::StartContainers));
    }
}
