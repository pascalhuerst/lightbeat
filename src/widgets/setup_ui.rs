//! Building blocks shared by the tabs of the Setup window, so that adding,
//! deleting, duplicating, reordering and status look and read the same
//! everywhere.

use std::hash::Hash;
use std::time::Duration;

use egui::collapsing_header::{paint_default_icon, CollapsingState};
use egui::{Align, Layout, Response, RichText, Ui};
use egui_phosphor::regular as icon;

use crate::theme;

/// How long a destructive button stays armed after its first click.
const CONFIRM_SECS: f64 = 4.0;

// ---------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------

/// The one style of "add something": a plus and a sentence-case caption.
pub fn add_button(ui: &mut Ui, caption: &str) -> bool {
    ui.button(format!("{} {}", icon::PLUS, caption)).clicked()
}

/// A small X that says what it deletes. Returns true when clicked.
pub fn delete_button(ui: &mut Ui, hover: &str) -> bool {
    ui.small_button(icon::X).on_hover_text(hover).clicked()
}

pub fn duplicate_button(ui: &mut Ui, hover: &str) -> bool {
    ui.small_button(icon::COPY).on_hover_text(hover).clicked()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Move {
    Up,
    Down,
}

/// Up and down arrows for the row at `index` of `len`. The arrow that cannot
/// act is greyed out rather than hidden, so the buttons stay in their place.
pub fn move_buttons(ui: &mut Ui, index: usize, len: usize) -> Option<Move> {
    let mut mv = None;
    if ui
        .add_enabled(index > 0, egui::Button::new(icon::ARROW_UP).small())
        .on_hover_text("Move up")
        .clicked()
    {
        mv = Some(Move::Up);
    }
    if ui
        .add_enabled(index + 1 < len, egui::Button::new(icon::ARROW_DOWN).small())
        .on_hover_text("Move down")
        .clicked()
    {
        mv = Some(Move::Down);
    }
    mv
}

/// Carries out a `Move` on a list. Returns false when it would fall off the end.
pub fn apply_move<T>(items: &mut [T], index: usize, mv: Move) -> bool {
    let other = match mv {
        Move::Up if index > 0 => index - 1,
        Move::Down if index + 1 < items.len() => index + 1,
        _ => return false,
    };
    items.swap(index, other);
    true
}

/// A button that needs a second click within a few seconds, for things that
/// replace work. Returns true on the confirming click.
pub fn confirm_button(ui: &mut Ui, id: impl Hash, label: &str, confirm_label: &str, hover: &str) -> bool {
    let id = ui.make_persistent_id(("confirm_button", id));
    let now = ui.input(|i| i.time);
    let armed_at: Option<f64> = ui.data(|d| d.get_temp(id));
    if let Some(t) = armed_at
        && now - t < CONFIRM_SECS
    {
        ui.ctx().request_repaint_after(Duration::from_millis(250));
        let confirm = ui.button(RichText::new(confirm_label).color(theme::SEM_DANGER));
        if confirm.clicked() {
            ui.data_mut(|d| d.remove_temp::<f64>(id));
            return true;
        }
        return false;
    }
    if ui.button(label).on_hover_text(hover).clicked() {
        ui.data_mut(|d| d.insert_temp(id, now));
    }
    false
}

/// A dropdown that works as an "Add ..." button: it lists what can be added
/// and returns the pick. Scales to many candidates, unlike a button each.
pub fn add_menu<T>(ui: &mut Ui, id: impl Hash, caption: &str, items: Vec<(T, String)>) -> Option<T> {
    let mut picked = None;
    egui::ComboBox::from_id_salt(id)
        .selected_text(format!("{} {}", icon::PLUS, caption))
        .show_ui(ui, |ui| {
            for (value, label) in items {
                if ui.selectable_label(false, label).clicked() {
                    picked = Some(value);
                }
            }
        });
    picked
}

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

pub fn empty_state(ui: &mut Ui, text: &str) {
    ui.colored_label(theme::TEXT_DIM, text);
}

pub fn section(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).strong());
}

/// "Name" and a text box on one line. Returns true when edited.
pub fn name_field(ui: &mut Ui, name: &mut String) -> bool {
    ui.horizontal(|ui| {
        ui.label("Name");
        ui.text_edit_singleline(name).changed()
    })
    .inner
}

/// A dim "label value" line, for facts the user cannot edit.
pub fn info_row(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.colored_label(theme::TEXT_DIM, label);
        ui.label(value);
    });
}

/// What a reference to something deleted shows instead of its name.
pub const MISSING: &str = "(missing)";

/// The name a duplicate gets.
pub fn copy_name(name: &str) -> String {
    format!("{} copy", name)
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

/// The four states a thing connected to the outside can be in. Every tab uses
/// these words, whatever its own model calls them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Connected,
    Waiting,
    Disabled,
    NoMapping,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Connected => "Connected",
            Status::Waiting => "Waiting",
            Status::Disabled => "Disabled",
            Status::NoMapping => "No mapping",
        }
    }

    pub fn color(self) -> egui::Color32 {
        match self {
            Status::Connected => theme::SEM_SUCCESS,
            Status::Waiting => theme::SEM_WARNING,
            Status::Disabled | Status::NoMapping => theme::TEXT_DIM,
        }
    }
}

/// Returns the label's response so a caller can attach a hover text.
pub fn status_badge(ui: &mut Ui, status: Status) -> Response {
    ui.colored_label(status.color(), status.label())
}

/// The one wording for hardware that is set up but not present, or none when
/// everything is there. Status bar and both hardware tabs show this text.
pub fn not_found_text(missing: &[String]) -> Option<String> {
    if missing.is_empty() {
        None
    } else {
        Some(format!("Not found: {}", missing.join(", ")))
    }
}

/// The Rescan button of the hardware tabs. Returns true when clicked; the app
/// then rescans controllers, audio inputs and MIDI together.
pub fn rescan_button(ui: &mut Ui) -> bool {
    ui.button(format!("{} Rescan", icon::ARROWS_CLOCKWISE))
        .on_hover_text("Look again for controllers, audio inputs and MIDI devices")
        .clicked()
}

/// Rescan button plus the "Not found" banner. Returns true when Rescan was clicked.
pub fn hardware_bar(ui: &mut Ui, missing: &[String]) -> bool {
    ui.horizontal(|ui| {
        let clicked = rescan_button(ui);
        if let Some(text) = not_found_text(missing) {
            ui.colored_label(theme::SEM_WARNING, text);
        }
        clicked
    })
    .inner
}

// ---------------------------------------------------------------------------
// Collapsible item
// ---------------------------------------------------------------------------

/// What goes into the header row of a collapsible item. The name is edited
/// inside the body, the header only shows it.
pub struct ItemHeader<'a> {
    pub title: &'a str,
    pub status: Option<Status>,
    /// Shows an "Enabled" checkbox with this value.
    pub enabled: Option<bool>,
    /// Hover text of the duplicate button; none for items that cannot be copied.
    pub duplicate: Option<&'a str>,
    pub delete: &'a str,
}

impl<'a> ItemHeader<'a> {
    pub fn new(title: &'a str, delete: &'a str) -> Self {
        Self { title, status: None, enabled: None, duplicate: None, delete }
    }
}

#[derive(Default)]
pub struct HeaderActions {
    pub delete: bool,
    pub duplicate: bool,
    /// The new value when the Enabled checkbox was toggled.
    pub enabled: Option<bool>,
}

/// How many items start open: all of them when there is only one.
pub fn open_by_default(item_count: usize) -> bool {
    item_count == 1
}

/// A collapsible item: arrow and name on the left, then on the right status,
/// enabled, duplicate and delete. Clicking the name opens and closes it.
pub fn item_header(
    ui: &mut Ui,
    id: impl Hash,
    default_open: bool,
    header: ItemHeader,
    body: impl FnOnce(&mut Ui),
) -> HeaderActions {
    let state_id = ui.make_persistent_id(("setup_item", id));
    let mut state = CollapsingState::load_with_default_open(ui.ctx(), state_id, default_open);
    let mut actions = HeaderActions::default();

    let row = ui.horizontal(|ui| {
        let prev_spacing = ui.spacing().item_spacing.x;
        ui.spacing_mut().item_spacing.x = 0.0;
        state.show_toggle_button(ui, paint_default_icon);
        ui.spacing_mut().item_spacing.x = prev_spacing;

        let title = if header.title.is_empty() { "(unnamed)" } else { header.title };
        let name = ui.add(egui::Label::new(RichText::new(title).strong()).sense(egui::Sense::click()));
        if name.clicked() {
            state.toggle(ui);
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if delete_button(ui, header.delete) {
                actions.delete = true;
            }
            if let Some(hover) = header.duplicate
                && duplicate_button(ui, hover)
            {
                actions.duplicate = true;
            }
            if let Some(on) = header.enabled {
                let mut v = on;
                if ui.checkbox(&mut v, "Enabled").changed() {
                    actions.enabled = Some(v);
                }
            }
            if let Some(status) = header.status {
                status_badge(ui, status);
            }
        });
    });

    // Scoped by the item, so widgets in two bodies never share an id.
    state.show_body_indented(&row.response, ui, |ui| {
        ui.push_id(state_id, body);
    });
    actions
}

// ---------------------------------------------------------------------------
// DMX address
// ---------------------------------------------------------------------------

/// Net, subnet and universe as `net.sub.universe`, the order Art-Net writes a
/// port address in. Returns true when any of them changed.
pub fn dmx_address(ui: &mut Ui, net: &mut u8, sub: &mut u8, universe: &mut u8) -> bool {
    fn part(ui: &mut Ui, value: &mut u8, max: i32, name: &str) -> bool {
        let mut v = i32::from(*value);
        let changed = ui.add(egui::DragValue::new(&mut v).range(0..=max)).on_hover_text(name).changed();
        if changed {
            *value = v as u8;
        }
        changed
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let mut changed = part(ui, net, 127, "Net");
        ui.label(".");
        changed |= part(ui, sub, 15, "Subnet");
        ui.label(".");
        changed |= part(ui, universe, 15, "Universe");
        changed
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_swaps_neighbours() {
        let mut v = vec![1, 2, 3];
        assert!(apply_move(&mut v, 1, Move::Up));
        assert_eq!(v, [2, 1, 3]);
        assert!(apply_move(&mut v, 1, Move::Down));
        assert_eq!(v, [2, 3, 1]);
    }

    #[test]
    fn move_stops_at_the_ends() {
        let mut v = vec![1, 2];
        assert!(!apply_move(&mut v, 0, Move::Up));
        assert!(!apply_move(&mut v, 1, Move::Down));
        assert_eq!(v, [1, 2]);
    }

    #[test]
    fn copies_are_named_copy() {
        assert_eq!(copy_name("Par"), "Par copy");
    }

    #[test]
    fn not_found_text_lists_names_or_nothing() {
        assert_eq!(not_found_text(&[]), None);
        let m = vec!["X1".to_string(), "Mic".to_string()];
        assert_eq!(not_found_text(&m).as_deref(), Some("Not found: X1, Mic"));
    }

    #[test]
    fn single_item_starts_open() {
        assert!(open_by_default(1));
        assert!(!open_by_default(0));
        assert!(!open_by_default(2));
    }
}
