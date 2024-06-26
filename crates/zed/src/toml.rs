use zed::LanguageServerId;
use zed_extension_api::{self as zed, Result};

struct CargoTomlExtension {}

impl zed::Extension for CargoTomlExtension {
    fn new() -> Self {
        Self {}
    }

    fn language_server_command(
        &mut self,
        _: &LanguageServerId,
        _worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        Ok(zed::Command {
            command: "/Users/frederik/.cargo/bin/cargo-dependency-language-server".to_string(),
            args: vec![
                "--storage".to_string(),
                std::env::current_dir()
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
            ],
            env: Default::default(),
        })
    }
}

zed::register_extension!(CargoTomlExtension);
