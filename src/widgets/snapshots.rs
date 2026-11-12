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

/// Whether the node has anything a snapshot could hold.
pub fn applicable(node: &dyn NodeWidget) -> bool {
    let shared = node.shared_state().lock().unwrap();
    shared.current_params.iter().any(|p| storable(p, shared.non_promotable))
}

/// A small clock: while a transition runs, a wedge fills as it goes; when it
/// is over the face is solid (green when the values still match, orange when
/// they were changed since); a snapshot that is not selected is an empty ring.
fn clock(ui: &mut Ui, active: bool, modified: bool, fading: bool, progress: f32) -> egui::Response {
    let (resp, painter) = ui.allocate_painter(egui::vec2(16.0, 16.0), egui::Sense::hover());
    let c = resp.rect.center();
    let r = 6.5;
    let color = match (active, fading, modified) {
        (false, _, _) => theme::TEXT_DIM,
        (true, true, _) => theme::SEM_PRIMARY,
        (true, false, true) => theme::SEM_WARNING,
        (true, false, false) => theme::SEM_SUCCESS,
    };
    if active && !fading && !modified {
        painter.circle_filled(c, r, color);
    } else {
        painter.circle_stroke(c, r, egui::Stroke::new(1.5, color));
        let sweep = if fading { progress.clamp(0.0, 1.0) } else if active { 0.5 } else { 0.0 };
        if sweep > 0.0 {
            let steps = ((sweep * 32.0).ceil() as usize).max(1);
            let mut pts = vec![c];
            for i in 0..=steps {
                let a = -std::f32::consts::FRAC_PI_2 + sweep * std::f32::consts::TAU * i as f32 / steps as f32;
                pts.push(c + egui::vec2(a.cos(), a.sin()) * (r - 1.5));
            }
            painter.add(egui::Shape::convex_polygon(pts, color.linear_multiply(0.8), egui::Stroke::NONE));
        }
    }
    resp
}

/// The snapshot panel at the bottom of the inspector.
pub fn show(ui: &mut Ui, node: &dyn NodeWidget) {
    let native_inputs = node.ui_inputs().len();
    let (set, params, skip, fading, progress, promoted, connected) = {
        let shared = node.shared_state().lock().unwrap();
        let mut set = shared.snapshots.clone();
        set.active = shared.snapshot_active.or(set.active);
        (
            set,
            shared.current_params.clone(),
            shared.non_promotable,
            shared.snapshot_fading,
            shared.snapshot_progress,
            shared.promoted.clone(),
            shared.inputs_connected.clone(),
        )
    };

    let slot_of = |name: &str| promoted.iter().position(|p| p.name == name);
    let wired = |name: &str| {
        slot_of(name).is_some_and(|s| connected.get(native_inputs + s).copied().unwrap_or(false))
    };
    let select_on = slot_of(SELECT_NAME).is_some();
    let fade_on = slot_of(FADE_NAME).is_some();
    let modified = set.modified(&params, skip);

    let mut edit: Option<Edit> = None;
    let id = editing_id().with(node.node_id().0);
    let mut editing: Option<u64> = ui.data(|d| d.get_temp(id));

    ui.add_space(2.0);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            if ui
                .button(format!("{} Add snapshot", icon::PLUS))
                .on_hover_text("Store all the current values as a new snapshot.")
                .clicked()
            {
                edit = Some(Edit::Add);
            }
        });
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
        }
        // Time or phase, not both: with the phase input on, the time is moot.
        let mut fade = fade_on;
        if ui.checkbox(&mut fade, "Fade by phase input")
            .on_hover_text("Fade with a phase wire instead of a time. Reaching 1 completes the change; \
                            going back to 0 undoes it.")
            .changed()
        {
            edit = Some(Edit::Input { name: FADE_NAME, kind: PromotedKind::SnapshotFade, on: fade });
        }
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!fade_on, |ui| {
                ui.label("Fade time");
                let mut t = set.fade_time;
                if ui.add(egui::DragValue::new(&mut t).range(0.0..=60.0).speed(0.01).suffix(" s")).changed() {
                    edit = Some(Edit::FadeTime(t));
                }
            });
            if fade_on && !wired(FADE_NAME) {
                ui.colored_label(theme::TEXT_DIM, "waiting for a wire");
            }
        });
    });

    ui.add_space(4.0);
    ui.label(egui::RichText::new("Snapshots").strong());
    let list_fill = ui.visuals().extreme_bg_color;
    egui::Frame::NONE
        .fill(list_fill)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(3.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::vertical()
                .id_salt(("snapshot_list", node.node_id().0))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if set.snapshots.is_empty() {
                        ui.add_space(6.0);
                        ui.colored_label(theme::TEXT_DIM, "  No snapshots yet.");
                    }
                    for (i, snap) in set.snapshots.iter().enumerate() {
                        let is_active = set.active == Some(snap.id);
                        let row_fill = if is_active {
                            theme::SEM_PRIMARY_FILL
                        } else if i % 2 == 1 {
                            egui::Color32::from_white_alpha(6)
                        } else {
                            egui::Color32::TRANSPARENT
                        };
                        egui::Frame::NONE.fill(row_fill).inner_margin(egui::Margin::symmetric(4, 2)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let tip = match (is_active, fading, modified) {
                                    (true, true, _) => "The change is running.",
                                    (true, false, true) => "Changed since it was recalled. Click the name to go back to the stored values.",
                                    (true, false, false) => "Done: the values match this snapshot.",
                                    _ => "Not selected.",
                                };
                                clock(ui, is_active, modified, fading, progress).on_hover_text(tip);

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
                                    let text = if is_active && modified && !fading {
                                        egui::RichText::new(format!("{} *", snap.name)).color(theme::SEM_WARNING)
                                    } else {
                                        egui::RichText::new(&snap.name)
                                    };
                                    let r = ui.add(egui::Label::new(text).sense(egui::Sense::click()))
                                        .on_hover_text("Click to recall it.");
                                    if r.clicked() {
                                        edit = Some(Edit::Recall(i));
                                    }
                                }

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.small_button(icon::X).on_hover_text("Delete").clicked() {
                                        edit = Some(Edit::Delete(i));
                                    }
                                    if ui.add_enabled(i + 1 < set.snapshots.len(), egui::Button::new(icon::ARROW_DOWN).small()).clicked() {
                                        edit = Some(Edit::Move(i, 1));
                                    }
                                    if ui.add_enabled(i > 0, egui::Button::new(icon::ARROW_UP).small()).clicked() {
                                        edit = Some(Edit::Move(i, -1));
                                    }
                                    if ui.small_button(icon::FLOPPY_DISK)
                                        .on_hover_text("Store the current values into this snapshot.").clicked()
                                    {
                                        edit = Some(Edit::Overwrite(i));
                                    }
                                    if ui.small_button(icon::PENCIL_SIMPLE).on_hover_text("Rename").clicked() {
                                        editing = Some(snap.id);
                                    }
                                });
                            });
                        });
                    }
                });
        });
    match editing {
        Some(e) => ui.data_mut(|d| d.insert_temp(id, e)),
        None => { ui.data_mut(|d| d.remove_temp::<u64>(id)); }
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
