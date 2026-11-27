use crate::app::ReDox;
use eframe::egui;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UiAction {
    AddFiles,
    Chats,
    History,
    NewChat,
    Next,
    OllamaAgent,
    OpenProject,
    Previous,
    Rename,
    Send,
    Settings,
    Sidebar,
    Stop,
}

impl ReDox {
    pub(crate) fn dispatch_ui_action(&mut self, action: UiAction, ctx: &egui::Context) {
        match action {
            UiAction::History => {
                self.sidebar_visible = true;
                self.refresh_history(ctx, false);
            }
            UiAction::Chats => {
                self.sidebar_visible = true;
                self.search_open = false;
            }
            UiAction::OpenProject => self.pick_workspace(ctx),
            UiAction::NewChat => {
                if self.can_new_chat() {
                    self.new_chat();
                }
            }
            UiAction::Send => self.send_prompt(ctx),
            UiAction::Stop => self.stop_response(),
            UiAction::Previous => self.navigate(-1),
            UiAction::Next => self.navigate(1),
            UiAction::Rename => self.rename_open = true,
            UiAction::OllamaAgent => {
                self.ui_preferences.agents_open = !self.ui_preferences.agents_open;
            }
            UiAction::Settings => {
                self.settings_page = crate::app::ui::SettingsPage::Account;
                self.settings_open = true;
            }
            UiAction::Sidebar => self.sidebar_visible = !self.sidebar_visible,
            UiAction::AddFiles => self.pick_files(ctx),
        }
    }
}
