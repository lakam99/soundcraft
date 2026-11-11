//! A compact named-input list: mono ports or explicit stereo L/R, beside the device settings.
use crate::SoundApp;
use serde_json::json;

pub fn show(app: &mut SoundApp, ui: &mut egui::Ui, device: Option<(&str, usize)>) {
    ui.heading("Audio inputs");
    let count = device.map_or(0, |(_, n)| n).min(soundcraft_model::MAX_INPUT_CHANNELS);
    ui.horizontal_wrapped(|ui| {
        ui.label(device.map_or("No available input device".into(), |(name, n)| format!("{name} · {n} input channels")));
        if ui.button("Audio device…").clicked() {
            app.ui.show_playback_engine = true;
        }
    });
    ui.label("Name your inputs, assign physical channels, then choose the name on a track. Mono uses one port; stereo uses left and right.");
    ui.horizontal_wrapped(|ui| {
        for (label, format, width) in [("+ Mono input", "mono", 1), ("+ Stereo input", "stereo", 2)] {
            if ui.button(label).clicked() {
                let mut number = app.engine.session().inputs.len().saturating_add(1);
                while app.engine.session().inputs.iter().any(|p| p.name == format!("Input {number}")) { number = number.saturating_add(1); }
                let _ = app.run("setup.inputs", json!({"action":"create","name":format!("Input {number}"),"format":format,"channels":vec![None::<usize>; width]}));
            }
        }
        let response = ui.add_enabled(count > 0, egui::Button::new("Map all device inputs"));
        response.clone().on_hover_text("Replace the list with one mono input per hardware channel and adjacent stereo pairs. Track names that no longer match stay visibly unavailable.");
        if response.clicked() { let _ = app.run("setup.inputs", json!({"action":"defaults","count":count})); }
    });
    let paths = app.engine.session().inputs.clone();
    egui::ScrollArea::vertical().id_salt("audio_input_paths").max_height(300.0).show(ui, |ui| {
        egui::Grid::new("input_path_rows").striped(true).spacing([12.0, 9.0]).show(ui, |ui| {
            for title in ["On", "Input name", "Format", "Hardware channels", ""] {
                ui.strong(title);
            }
            ui.end_row();
            for path in paths {
                let mut active = path.active;
                if ui.checkbox(&mut active, "").on_hover_text("Enable this input").changed() {
                    let _ = app.run("setup.inputs", json!({"action":"update","name":path.name,"active":active}));
                }
                let key = egui::Id::unique(("input_path_name", &path.name));
                let mut name = ui.ctx().data(|d| d.get_temp::<String>(key)).unwrap_or_else(|| path.name.clone());
                // Grid sizing otherwise collapses names to the short default labels.
                let response = crate::widgets::named_text_field(
                    ui.add(egui::TextEdit::singleline(&mut name).id(key).desired_width(180.0).min_size(egui::vec2(180.0, 22.0))),
                    &format!("Input name: {}", path.name),
                );
                ui.ctx().data_mut(|d| d.insert_temp(key, name.clone()));
                if response.lost_focus() && name != path.name {
                    let _ = app.run("setup.inputs", json!({"action":"update","name":path.name,"new_name":name}));
                }
                ui.label(path.format.label());
                let mut ports = path.channels.clone();
                ui.horizontal(|ui| {
                    for (channel, port) in ports.iter_mut().enumerate() {
                        let label =
                            if path.channels.len() == 2 { if channel == 0 { "L".into() } else { "R".into() } } else { (channel + 1).to_string() };
                        if path.channels.len() > 1 {
                            ui.label(label);
                        }
                        let text = port
                            .map_or("Unassigned".into(), |p| if p < count { format!("Ch {}", p + 1) } else { format!("Ch {} unavailable", p + 1) });
                        craft_ui::ComboBox::from_id_salt(("input_port", &path.name, channel))
                            .width(105.0)
                            .selected_text(text)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(port, None, "Unassigned");
                                for p in 0..count {
                                    ui.selectable_value(port, Some(p), format!("Channel {}", p + 1));
                                }
                            })
                            .response
                            .accessible_name(format!("{} hardware channel {}", path.name, channel + 1));
                    }
                });
                if ui.small_button("Remove").clicked() {
                    let _ = app.run("setup.inputs", json!({"action":"delete","name":path.name}));
                }
                if ports != path.channels {
                    let _ = app.run("setup.inputs", json!({"action":"update","name":path.name,"channels":ports}));
                }
                ui.end_row();
            }
        });
    });
}
