use crate::LightBeatApp;
use crate::theme;
use crate::update;
use crate::version;
use eframe::egui;

impl LightBeatApp {
    /// Check for updates at start and every few hours, if the user allows it.
    pub(crate) fn update_check_tick(&mut self) {
        const EVERY: std::time::Duration = std::time::Duration::from_secs(6 * 3600);
        if !self.config.check_for_updates || !version::is_release() {
            return;
        }
        if self.last_update_check.is_none_or(|t| t.elapsed() >= EVERY) {
            self.last_update_check = Some(std::time::Instant::now());
            self.updater.check(self.config.update_channel);
        }
    }

    pub(crate) fn show_update_ui(&mut self, ctx: &egui::Context) {
        use update::State;
        let state = self.updater.state();
        if matches!(state, State::Installing | State::Checking) {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        if self.show_about {
            let mut open = true;
            egui::Window::new("About LightBeat").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.heading("LightBeat");
                ui.label("Beat-synchronized, node-based lighting control.");
                ui.add_space(4.0);
                ui.label(format!("Version {}", version::label()));
                ui.hyperlink_to("github.com/pascalhuerst/lightbeat", format!("https://github.com/{}", update::REPO));
            });
            self.show_about = open;
        }

        if !self.show_update_window {
            return;
        }
        let mut open = true;
        egui::Window::new("Software Update").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label(format!("You have {}.", version::label()));
            ui.add_space(4.0);
            match &state {
                State::Idle => {
                    if ui.button("Check now").clicked() {
                        self.updater.check(self.config.update_channel);
                    }
                }
                State::Checking => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Looking for updates...");
                    });
                }
                State::UpToDate => {
                    ui.colored_label(theme::SEM_SUCCESS, "LightBeat is up to date.");
                }
                State::Failed(e) => {
                    ui.colored_label(theme::SEM_DANGER, e);
                    if ui.button("Try again").clicked() {
                        self.updater.check(self.config.update_channel);
                    }
                }
                State::Available(release) => {
                    let name = if release.title.is_empty() { &release.tag } else { &release.title };
                    ui.label(egui::RichText::new(format!("Available: {name}")).strong());
                    if !release.notes.trim().is_empty() {
                        egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                            ui.colored_label(theme::TEXT_MUTED, release.notes.trim());
                        });
                    }
                    ui.add_space(4.0);
                    ui.colored_label(theme::TEXT_DIM, "The update replaces the app and restarts it. Your projects and setup are kept. Do not do it during a show.");
                    ui.horizontal(|ui| {
                        if ui.button("Update").clicked() {
                            self.updater.install(release.clone());
                        }
                        ui.hyperlink_to("Release page", release.page.clone());
                    });
                }
                State::Installing => {
                    let (done, total) = self.updater.progress();
                    ui.label("Downloading and installing...");
                    if total > 0 {
                        ui.add(egui::ProgressBar::new(done as f32 / total as f32).show_percentage());
                    } else {
                        ui.spinner();
                    }
                }
                State::Installed(_) => {
                    ui.colored_label(theme::SEM_SUCCESS, "Updated. Restart to run the new version.");
                    if ui.button("Restart now").clicked() {
                        match update::relaunch_after_exit() {
                            Ok(()) => self.quit_requested = true,
                            Err(e) => eprintln!("Could not arrange the restart: {e}"),
                        }
                    }
                }
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let mut auto = self.config.check_for_updates;
                if ui.checkbox(&mut auto, "Check automatically").changed() {
                    self.config.check_for_updates = auto;
                    self.config.save();
                }
                egui::ComboBox::from_id_salt("update_channel")
                    .selected_text(self.config.update_channel.label())
                    .show_ui(ui, |ui| {
                        for channel in update::Channel::ALL {
                            if ui.selectable_label(self.config.update_channel == channel, channel.label()).clicked() {
                                self.config.update_channel = channel;
                                self.config.save();
                                self.updater.check(channel);
                            }
                        }
                    });
            });
        });
        if !open {
            self.show_update_window = false;
            self.updater.dismiss();
        }
    }
}
