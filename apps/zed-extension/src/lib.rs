use zed_extension_api::{self as zed, LanguageServerId, Result};

struct TypedMarkExtension;

impl zed::Extension for TypedMarkExtension {
    fn new() -> Self {
        TypedMarkExtension
    }

    // `typedmark-lsp` has no published release binary yet (see
    // `apps/typedmark-lsp`), so unlike most extensions this doesn't
    // download one -- it expects the binary already on `$PATH` (e.g. via
    // the nix package), matching the "zed extension: nixでインストール /
    // lsp" roadmap bullets in `docs/roadmap.ja.tm`.
    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let path = worktree
            .which("typedmark-lsp")
            .ok_or_else(|| "typedmark-lsp not found on $PATH".to_string())?;
        Ok(zed::Command {
            command: path,
            args: Vec::new(),
            env: Default::default(),
        })
    }
}

zed::register_extension!(TypedMarkExtension);
