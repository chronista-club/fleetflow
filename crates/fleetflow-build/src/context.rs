use crate::error::{BuildError, BuildResult};
use flate2::Compression;
use flate2::write::GzEncoder;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use tar::Builder;

pub struct ContextBuilder;

impl ContextBuilder {
    /// ビルドコンテキストをtar.gzアーカイブとして作成
    pub fn create_context(context_path: &Path, dockerfile_path: &Path) -> BuildResult<Vec<u8>> {
        tracing::debug!("Creating build context from: {}", context_path.display());

        // tarアーカイブの作成
        let mut archive_data = Vec::new();
        {
            let encoder = GzEncoder::new(&mut archive_data, Compression::default());
            let mut tar = Builder::new(encoder);

            // コンテキストディレクトリを再帰的に追加
            tar.append_dir_all(".", context_path)
                .map_err(BuildError::Io)?;

            // Dockerfileを "Dockerfile" として追加
            let mut dockerfile_file = File::open(dockerfile_path)?;
            let mut dockerfile_content = Vec::new();
            dockerfile_file.read_to_end(&mut dockerfile_content)?;

            let mut header = tar::Header::new_gnu();
            header.set_path("Dockerfile").map_err(|e| {
                BuildError::InvalidConfig(format!("Failed to set Dockerfile path: {}", e))
            })?;
            header.set_size(dockerfile_content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();

            tar.append(&header, &dockerfile_content[..])
                .map_err(BuildError::Io)?;

            tar.finish().map_err(BuildError::Io)?;
        }

        tracing::debug!("Build context created: {} bytes", archive_data.len());

        // コンテキストサイズの警告
        Self::check_context_size(archive_data.len());

        Ok(archive_data)
    }

    /// コンテキストサイズのチェックと警告
    fn check_context_size(size: usize) {
        const MAX_CONTEXT_SIZE: usize = 500 * 1024 * 1024; // 500MB

        if size > MAX_CONTEXT_SIZE {
            tracing::warn!(
                "警告: ビルドコンテキストが大きすぎます（{}MB）\n\
                 .dockerignoreファイルで不要なファイルを除外することを推奨します。",
                size / 1024 / 1024
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_create_context() {
        let temp_dir = tempdir().unwrap();

        // テスト用のファイル構造を作成
        fs::write(temp_dir.path().join("file1.txt"), "content1").unwrap();
        fs::write(temp_dir.path().join("file2.txt"), "content2").unwrap();

        let subdir = temp_dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("file3.txt"), "content3").unwrap();

        // Dockerfileを作成
        let dockerfile = temp_dir.path().join("Dockerfile");
        fs::write(&dockerfile, "FROM alpine\nRUN echo test").unwrap();

        // コンテキストを作成
        let result = ContextBuilder::create_context(temp_dir.path(), &dockerfile);
        assert!(result.is_ok());

        let archive = result.unwrap();
        assert!(!archive.is_empty());

        // tarアーカイブとして展開できるか確認
        let extract_dir = tempdir().unwrap();
        let mut archive_reader = std::io::Cursor::new(archive);
        let decoder = flate2::read::GzDecoder::new(&mut archive_reader);
        let mut tar = tar::Archive::new(decoder);
        tar.unpack(extract_dir.path()).unwrap();

        // Dockerfileが含まれているか確認
        assert!(extract_dir.path().join("Dockerfile").exists());
    }

    #[test]
    fn test_create_context_empty_dir() {
        let temp_dir = tempdir().unwrap();

        // Dockerfileのみ作成
        let dockerfile = temp_dir.path().join("Dockerfile");
        fs::write(&dockerfile, "FROM alpine").unwrap();

        let result = ContextBuilder::create_context(temp_dir.path(), &dockerfile);
        assert!(result.is_ok());
    }
}
