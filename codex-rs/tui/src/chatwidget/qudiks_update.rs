use super::*;

impl ChatWidget {
    pub(super) fn open_qudiks_update_prompt(&mut self) {
        if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            self.add_error_message(
                "Binary updates are only available on Linux x86_64. Use the source installer on this platform."
                    .to_string(),
            );
            return;
        }
        let installed_binary = std::env::current_exe()
            .ok()
            .is_some_and(|path| path.file_name().is_some_and(|name| name == "qudiks-bin"));
        if !installed_binary {
            self.add_error_message(
                "Run the installed qudiks launcher to update. Development binaries must be rebuilt from source."
                    .to_string(),
            );
            return;
        }
        self.show_selection_view(SelectionViewParams {
            header: self.model_menu_header(
                "Update Qudiks and exit?",
                "Install the latest published Linux binary and launcher in the current install directory. Keep credentials and settings; no clone needed. This replaces source builds too. Relaunch qudiks afterward.",
            ),
            items: vec![
                SelectionItem {
                    name: "Cancel".to_string(),
                    dismiss_on_select: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: "Update and exit".to_string(),
                    actions: vec![Box::new(|tx| tx.send(AppEvent::RunQudiksUpdate))],
                    require_explicit_confirmation: true,
                    dismiss_on_select: true,
                    ..Default::default()
                },
            ],
            ..SelectionViewParams::picker()
        });
    }
}
