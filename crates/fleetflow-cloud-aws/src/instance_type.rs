//! EC2 インスタンスタイプマッピング
//!
//! KDL の cpu / memory 指定から最適な EC2 インスタンスタイプを自動選択する。

use crate::error::AwsError;

/// インスタンスタイプ候補
struct InstanceTypeEntry {
    name: &'static str,
    cpu: i32,
    memory_gb: i32,
}

/// サポートするインスタンスタイプ一覧（cpu, memory 昇順）
const INSTANCE_TYPES: &[InstanceTypeEntry] = &[
    // t3 ファミリー（バースト可能、汎用）
    InstanceTypeEntry {
        name: "t3.micro",
        cpu: 2,
        memory_gb: 1,
    },
    InstanceTypeEntry {
        name: "t3.small",
        cpu: 2,
        memory_gb: 2,
    },
    InstanceTypeEntry {
        name: "t3.medium",
        cpu: 2,
        memory_gb: 4,
    },
    InstanceTypeEntry {
        name: "t3.large",
        cpu: 2,
        memory_gb: 8,
    },
    InstanceTypeEntry {
        name: "t3.xlarge",
        cpu: 4,
        memory_gb: 16,
    },
    InstanceTypeEntry {
        name: "t3.2xlarge",
        cpu: 8,
        memory_gb: 32,
    },
    // m6i ファミリー（汎用、安定パフォーマンス）
    InstanceTypeEntry {
        name: "m6i.large",
        cpu: 2,
        memory_gb: 8,
    },
    InstanceTypeEntry {
        name: "m6i.xlarge",
        cpu: 4,
        memory_gb: 16,
    },
    InstanceTypeEntry {
        name: "m6i.2xlarge",
        cpu: 8,
        memory_gb: 32,
    },
    InstanceTypeEntry {
        name: "m6i.4xlarge",
        cpu: 16,
        memory_gb: 64,
    },
    // c6i ファミリー（コンピューティング最適化）
    InstanceTypeEntry {
        name: "c6i.large",
        cpu: 2,
        memory_gb: 4,
    },
    InstanceTypeEntry {
        name: "c6i.xlarge",
        cpu: 4,
        memory_gb: 8,
    },
    InstanceTypeEntry {
        name: "c6i.2xlarge",
        cpu: 8,
        memory_gb: 16,
    },
    InstanceTypeEntry {
        name: "c6i.4xlarge",
        cpu: 16,
        memory_gb: 32,
    },
];

/// cpu / memory から最適なインスタンスタイプを解決する
///
/// 完全一致を優先し、なければ要件を満たす最小のインスタンスを選択。
/// t3 ファミリーを優先（コスト効率が良い）。
pub fn resolve_instance_type(cpu: i32, memory_gb: i32) -> Result<&'static str, AwsError> {
    // 1. 完全一致（t3 優先）
    for entry in INSTANCE_TYPES {
        if entry.cpu == cpu && entry.memory_gb == memory_gb && entry.name.starts_with("t3") {
            return Ok(entry.name);
        }
    }

    // 2. 完全一致（全ファミリー）
    for entry in INSTANCE_TYPES {
        if entry.cpu == cpu && entry.memory_gb == memory_gb {
            return Ok(entry.name);
        }
    }

    // 3. 要件を満たす最小（t3 優先）
    let mut best: Option<&InstanceTypeEntry> = None;
    for entry in INSTANCE_TYPES {
        if entry.cpu >= cpu && entry.memory_gb >= memory_gb {
            match best {
                None => best = Some(entry),
                Some(current) => {
                    let entry_score = entry.cpu + entry.memory_gb;
                    let current_score = current.cpu + current.memory_gb;
                    // スコアが同じなら t3 優先
                    if entry_score < current_score
                        || (entry_score == current_score
                            && entry.name.starts_with("t3")
                            && !current.name.starts_with("t3"))
                    {
                        best = Some(entry);
                    }
                }
            }
        }
    }

    match best {
        Some(entry) => Ok(entry.name),
        None => Err(AwsError::InvalidInstanceType { cpu, memory_gb }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match_t3_micro() {
        assert_eq!(resolve_instance_type(2, 1).unwrap(), "t3.micro");
    }

    #[test]
    fn test_exact_match_t3_medium() {
        assert_eq!(resolve_instance_type(2, 4).unwrap(), "t3.medium");
    }

    #[test]
    fn test_exact_match_t3_large() {
        // t3.large と m6i.large は両方 2cpu/8GB だが t3 優先
        assert_eq!(resolve_instance_type(2, 8).unwrap(), "t3.large");
    }

    #[test]
    fn test_exact_match_t3_xlarge() {
        // t3.xlarge と m6i.xlarge は両方 4cpu/16GB だが t3 優先
        assert_eq!(resolve_instance_type(4, 16).unwrap(), "t3.xlarge");
    }

    #[test]
    fn test_exact_match_t3_2xlarge() {
        assert_eq!(resolve_instance_type(8, 32).unwrap(), "t3.2xlarge");
    }

    #[test]
    fn test_nearest_upper_match() {
        // 1cpu/1GB → t3.micro (2cpu/1GB) が最小
        assert_eq!(resolve_instance_type(1, 1).unwrap(), "t3.micro");
    }

    #[test]
    fn test_nearest_upper_3cpu() {
        // 3cpu/8GB → t3.xlarge (4cpu/16GB) or c6i.xlarge (4cpu/8GB)
        // c6i.xlarge のほうが合計スコア小さい
        assert_eq!(resolve_instance_type(3, 8).unwrap(), "c6i.xlarge");
    }

    #[test]
    fn test_large_instance() {
        assert_eq!(resolve_instance_type(16, 64).unwrap(), "m6i.4xlarge");
    }

    #[test]
    fn test_too_large_fails() {
        let result = resolve_instance_type(32, 128);
        assert!(result.is_err());
    }

    #[test]
    fn test_one_cpu_two_gb() {
        // 1cpu/2GB → t3.small (2cpu/2GB) が最小
        assert_eq!(resolve_instance_type(1, 2).unwrap(), "t3.small");
    }
}
