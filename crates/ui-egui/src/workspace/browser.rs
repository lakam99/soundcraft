use super::{BrowserTab, run, selected};
use crate::SoundApp;
use egui::{RichText, Ui};
use serde_json::json;

pub fn show(app: &mut SoundApp, ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        for (tab, label) in [(BrowserTab::Media, "Project media"), (BrowserTab::Instruments, "Instruments"), (BrowserTab::Effects, "Effects")] {
            let response = ui.selectable_label(app.workspace.browser_tab == tab, label);
            crate::assistant::register(app, &format!("browser.tab.{tab:?}"), &response, label);
            if response.clicked() {
                app.workspace.browser_tab = tab;
                app.workspace.search.clear();
            }
        }
    });
    ui.separator();
    match app.workspace.browser_tab {
        BrowserTab::Media => {
            super::chrome::import_menu(app, ui, "Import audio or MIDI…", "browser.import");
            ui.label(RichText::new("Media already imported into this project. Drag audio from the list to the arrangement.").small().weak());
            crate::panels::clip_list(app, ui);
        }
        BrowserTab::Instruments | BrowserTab::Effects => catalog(app, ui),
    }
}

fn catalog(app: &mut SoundApp, ui: &mut Ui) {
    let instruments = app.workspace.browser_tab == BrowserTab::Instruments;
    let response = ui
        .add(egui::TextEdit::singleline(&mut app.workspace.search).char_limit(128).hint_text("Search by name…").desired_width(ui.available_width()))
        .accessible_name("Search instruments and effects");
    crate::assistant::register(app, "browser.search", &response, "Search instruments and effects");
    ui.label(
        RichText::new(if instruments {
            "New track creates a separate instrument. Replace applies to the selected instrument track."
        } else {
            "Add inserts on the selected track's first empty slot."
        })
        .small()
        .weak(),
    );
    let filter = app.workspace.search.to_lowercase();
    let mut entries: Vec<_> = soundcraft_dsp::plugins()
        .iter()
        .filter(|plugin| plugin.is_instrument == instruments && plugin.name.to_lowercase().contains(&filter))
        .map(|plugin| (plugin.id.to_string(), plugin.name.to_string(), "Built-in".to_string()))
        .collect();
    if let Some(plugins) = soundcraft_clap_host::cached() {
        entries.extend(
            plugins
                .iter()
                .filter(|plugin| plugin.is_instrument == instruments && plugin.name.to_lowercase().contains(&filter))
                .map(|plugin| (plugin.id.clone(), plugin.name.clone(), "CLAP".into())),
        );
    }
    if let Some(plugins) = soundcraft_vst3_host::cached() {
        entries.extend(
            plugins
                .iter()
                .filter(|plugin| plugin.is_instrument == instruments && plugin.name.to_lowercase().contains(&filter))
                .map(|plugin| (plugin.id.clone(), plugin.name.clone(), "VST3".into())),
        );
    }
    entries.sort_by_cached_key(|(_, name, _)| name.to_lowercase());
    egui::ScrollArea::vertical().id_salt("browser.catalog").auto_shrink([false, false]).show(ui, |ui| {
        if entries.is_empty() {
            ui.label("No matching plugins. Try a different name.");
        }
        for (id, name, kind) in entries {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.strong(&name);
                ui.label(RichText::new(kind).small().weak());
                ui.horizontal_wrapped(|ui| {
                    if instruments {
                        instrument_actions(app, ui, &id, &name);
                    } else {
                        effect_action(app, ui, &id);
                    }
                });
            });
        }
    });
}

fn instrument_actions(app: &mut SoundApp, ui: &mut Ui, id: &str, name: &str) {
    let response = ui.add_sized([0.0, 28.0], egui::Button::new("New track"));
    crate::assistant::register(app, &format!("browser.new.{id}"), &response, &format!("New {name} instrument track"));
    if response.clicked() {
        run(app, "workspace.instrumentTrack", json!({"plugin":id}));
    }
    let target =
        selected(app).filter(|track| app.engine.session().track(*track).is_some_and(|track| track.kind == soundcraft_model::TrackKind::Instrument));
    let response = ui.add_enabled(target.is_some(), egui::Button::new("Replace selected"));
    crate::assistant::register(app, &format!("browser.replace.{id}"), &response, "Replace selected track instrument");
    if response.clicked()
        && let Some(track) = target
    {
        run(app, "mix.instrument", json!({"track":track.0,"plugin":id}));
    }
}

fn effect_action(app: &mut SoundApp, ui: &mut Ui, id: &str) {
    let target = selected(app);
    let response = ui.add_enabled(target.is_some(), egui::Button::new("Add to selected track"));
    crate::assistant::register(app, &format!("browser.effect.{id}"), &response, "Add effect to selected track");
    if response.clicked()
        && let Some(track) = target
    {
        run(app, "mix.insert", json!({"track":track.0,"plugin":id}));
    }
}
