//! The "Snapshots" section at the bottom of a node's inspector.
//!
//! A snapshot is a named copy of the node's settings (see `engine::snapshots`).
//! The list is edited here; recalling and fading run in the engine, driven by a
//! click, a select wire or a fade wire.

use egui::{self, Ui};
use egui_phosphor::regular as icon;

use super::nodes::node::NodeWidget;
use crate::engine::snapshots::{FADE_NAME, SELECT_NAME, SnapshotSet, capture, storable};
use crate::engine::types::{PromotedKind, PromotedParam};
use crate::theme;

/// Which snapshot's name is being edited.
fn editing_id() -> egui::Id {
    egui::Id::new("snapshot_name_edit")
}

enum Edit {
    Add,
    Recall(usize),
    Overwrite(usize),
    Move(usize, isize),
    Delete(usize),
    Rename(usize, String),
    FadeTime(f32),
    Spread(bool),
    Input { name: &'static str, kind: PromotedKind, on: bool },
}

pub fn show(ui: &mut Ui, node: &dyn NodeWidget) {
    let native_inputs = node.ui_inputs().len();
    let (set, params, skip, fading, promoted, connected) = {
        let shared = node.shared_state().lock().unwrap();
        let mut set = shared.snapshots.clone();
        set.active = shared.snapshot_active.or(set.active);
        (
            set,
            shared.current_params.clone(),
            shared.non_promotable,
            shared.snapshot_fading,
            shared.promoted.clone(),
            shared.inputs_connected.clone(),
        )
    };
    if !params.iter().any(|p| storable(p, skip)) {
        return;
    }

    let slot_of = |name: &str| promoted.iter().position(|p| p.name == name);
    let wired = |name: &str| {
        slot_of(name).is_some_and(|s| connected.get(native_inputs + s).copied().unwrap_or(false))
    };
    let select_on = slot_of(SELECT_NAME).is_some();
    let fade_on = slot_of(FADE_NAME).is_some();
    let modified = set.modified(&params, skip);

    ui.separator();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Snapshots").strong().size(11.0));
        if fading {
            ui.colored_label(theme::SEM_PRIMARY, "fading");
        }
    });
    ui.add_space(4.0);

    let mut edit: Option<Edit> = None;
    let id = editing_id().with(node.node_id().0);
    let mut editing: Option<u64> = ui.data(|d| d.get_temp(id));

    if ui
        .button(format!("{} Add snapshot", icon::PLUS))
        .on_hover_text("Store all the current values as a new snapshot.")
        .clicked()
    {
        edit = Some(Edit::Add);
    }

    for (i, snap) in set.snapshots.iter().enumerate() {
        let is_active = set.active == Some(snap.id);
        ui.horizontal(|ui| {
            let (mark, color) = match (is_active, modified) {
                (true, true) => (icon::CIRCLE_HALF, theme::SEM_WARNING),
                (true, false) => (icon::CIRCLE, theme::SEM_SUCCESS),
                _ => (icon::CIRCLE, theme::TEXT_DIM),
            };
            let tip = match (is_active, modified) {
                (true, true) => "Changed since it was recalled. Click the name to go back to the stored values.",
                (true, false) => "The values match this snapshot.",
                _ => "Not selected.",
            };
            ui.colored_label(color, mark).on_hover_text(tip);

            if editing == Some(snap.id) {
                let mut name = snap.name.clone();
                let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(110.0));
                r.request_focus();
                if r.changed() {
                    edit = Some(Edit::Rename(i, name));
                }
                if r.lost_focus() {
                    editing = None;
                }
            } else {
                let text = if is_active && modified {
                    egui::RichText::new(format!("{} *", snap.name)).color(theme::SEM_WARNING)
                } else {
                    egui::RichText::new(&snap.name)
                };
                let r = ui.selectable_label(is_active, text)
                    .on_hover_text("Click to recall it.");
                if r.clicked() {
                    edit = Some(Edit::Recall(i));
                }
            }

            if ui.small_button(icon::PENCIL_SIMPLE).on_hover_text("Rename").clicked() {
                editing = Some(snap.id);
            }
            if ui.small_button(icon::FLOPPY_DISK)
                .on_hover_text("Store the current values into this snapshot.").clicked()
            {
                edit = Some(Edit::Overwrite(i));
            }
            if ui.add_enabled(i > 0, egui::Button::new(icon::ARROW_UP).small()).clicked() {
                edit = Some(Edit::Move(i, -1));
            }
            if ui.add_enabled(i + 1 < set.snapshots.len(), egui::Button::new(icon::ARROW_DOWN).small()).clicked() {
                edit = Some(Edit::Move(i, 1));
            }
            if ui.small_button(icon::X).on_hover_text("Delete").clicked() {
                edit = Some(Edit::Delete(i));
            }
        });
    }
    match editing {
        Some(e) => ui.data_mut(|d| d.insert_temp(id, e)),
        None => { ui.data_mut(|d| d.remove_temp::<u64>(id)); }
    }

    ui.add_space(4.0);
    let mut sel = select_on;
    if ui.checkbox(&mut sel, "Select input")
        .on_hover_text("Add an input that picks the snapshot: its number, counting from 0.")
        .changed()
    {
        edit = Some(Edit::Input { name: SELECT_NAME, kind: PromotedKind::SnapshotSelect, on: sel });
    }
    if select_on {
        let mut spread = set.spread;
        if ui.checkbox(&mut spread, "Spread 0..1 over the list")
            .on_hover_text("Read the select input as 0..1 across all snapshots (a fader, a phase) instead of a number.")
            .changed()
        {
            edit = Some(Edit::Spread(spread));
        }
        if wired(SELECT_NAME) {
            ui.colored_label(theme::TEXT_DIM, "Recalls when the input changes.");
        }
    }
    let mut fade = fade_on;
    if ui.checkbox(&mut fade, "Fade input")
        .on_hover_text("Add a phase input that fades from the old snapshot to the new one: a full 0..1 \
                        run is the whole fade, and running back fades back.")
        .changed()
    {
        edit = Some(Edit::Input { name: FADE_NAME, kind: PromotedKind::SnapshotFade, on: fade });
    }
    if fade_on && wired(FADE_NAME) {
        ui.colored_label(theme::TEXT_DIM, "The fade follows the wire.");
    } else {
        ui.horizontal(|ui| {
            ui.label("Fade time");
            let mut t = set.fade_time;
            if ui.add(egui::DragValue::new(&mut t).range(0.0..=60.0).speed(0.01).suffix(" s")).changed() {
                edit = Some(Edit::FadeTime(t));
            }
        });
    }

    if let Some(edit) = edit {
        apply(node, edit, &params, skip);
    }
}

fn apply(node: &dyn NodeWidget, edit: Edit, params: &[crate::engine::types::ParamDef], skip: &[&str]) {
    let mut shared = node.shared_state().lock().unwrap();
    let mut set: SnapshotSet = shared.snapshots.clone();
    set.active = shared.snapshot_active.or(set.active);
    let mut changed_set = true;
    match edit {
        Edit::Add => {
            let id = set.add(None, capture(params, skip));
            set.active = Some(id);
        }
        Edit::Recall(i) => {
            shared.pending_recall = Some(i);
            changed_set = false;
        }
        Edit::Overwrite(i) => {
            if let Some(s) = set.snapshots.get_mut(i) {
                s.values = capture(params, skip);
                set.active = Some(s.id);
            }
        }
        Edit::Move(i, d) => {
            let j = i as isize + d;
            if j >= 0 && (j as usize) < set.snapshots.len() {
                set.snapshots.swap(i, j as usize);
            }
        }
        Edit::Delete(i) => {
            if i < set.snapshots.len() {
                let gone = set.snapshots.remove(i);
                if set.active == Some(gone.id) {
                    set.active = None;
                }
            }
        }
        Edit::Rename(i, name) => {
            if let Some(s) = set.snapshots.get_mut(i) {
                s.name = name;
            }
        }
        Edit::FadeTime(t) => set.fade_time = t,
        Edit::Spread(on) => set.spread = on,
        Edit::Input { name, kind, on } => {
            if on {
                shared.promoted.push(PromotedParam { name: name.to_string(), kind });
            } else {
                shared.promoted.retain(|p| p.name != name);
            }
            shared.pending_promoted = Some(shared.promoted.clone());
            changed_set = false;
        }
    }
    if changed_set {
        shared.snapshots = set.clone();
        shared.pending_snapshots = Some(set);
    }
}
