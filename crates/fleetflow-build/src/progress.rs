use indicatif::{ProgressBar, ProgressStyle};

pub struct BuildProgress {
    progress_bar: ProgressBar,
}

impl BuildProgress {
    pub fn new(service_name: &str) -> Self {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {msg}")
                .unwrap(),
        );
        pb.set_message(format!("Building {}...", service_name));

        Self { progress_bar: pb }
    }

    pub fn set_message(&self, msg: &str) {
        self.progress_bar.set_message(msg.to_string());
    }

    pub fn finish(&self, message: &str) {
        self.progress_bar.finish_with_message(message.to_string());
    }

    pub fn finish_success(&self) {
        self.progress_bar.finish_with_message("Build completed ✓");
    }

    pub fn finish_error(&self, error: &str) {
        self.progress_bar
            .finish_with_message(format!("Build failed: {}", error));
    }
}
