//! The in-window menu bar: PhotoCraft v0.3.0's bar (`menus.rs` `menu_bar`, `switch_on_hover`,
//! `pointer_reaches_title`, the press-drag gesture and `menu_nav`), drawn from the shared
//! [`MenuBar`] instead of PhotoCraft's flat list. On macOS this is what the apps show today, inside
//! the window under the title bar, where Mac users don't look for it.
//!
//! [`Hover::Legacy`] brings back PhotoCraft v0.2.0's behaviour for comparison: the hover switch
//! hit-tests title rects only, and menus aren't kept below the bar, so a tall submenu slides up
//! over the titles and moving inside it opens another menu (photocraft#92).

use crate::menu_nav::{self, Nav};
use crate::model::{MenuBar, Node};
use crate::shortcut::Chord;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hover {
    /// PhotoCraft v0.3.0: titles behind a popup don't react, and every menu stays below the bar.
    Fixed,
    /// PhotoCraft v0.2.0.
    Legacy,
}

/// A chosen item: its key, and whether ⌥ was held (Photoshop's ⌥-click variants).
pub type Chosen = (String, bool);

/// Draw the bar. Returns the item chosen this frame, if any.
pub fn menu_bar(ui: &mut egui::Ui, bar: &MenuBar, hover: Hover) -> Option<Chosen> {
    let mac = cfg!(target_os = "macos");
    let mut clicked: Option<String> = None;
    let mut nav = Nav::load(ui.ctx());
    let bounded = hover == Hover::Fixed;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let v = &mut ui.style_mut().visuals;
        v.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
        v.widgets.inactive.bg_stroke = egui::Stroke::NONE;
        v.widgets.hovered.bg_stroke = egui::Stroke::NONE;
        egui::MenuBar::new().ui(ui, |ui| {
            ui.spacing_mut().button_padding = egui::vec2(6.0, 3.0);
            nav.bar_bottom = Some(ui.max_rect().bottom());
            let mut buttons = Vec::with_capacity(bar.menus.len());
            for menu in &bar.menus {
                // Titles open on the press, so one gesture can press, drag to an item and release.
                let title = ui.add(egui::Button::new(menu.title.as_str()));
                let cfg = egui::containers::menu::MenuConfig::find(ui);
                let config = egui::containers::menu::MenuConfig::new().close_behavior(cfg.close_behavior).style(cfg.style.clone());
                let open = title_press(ui.ctx(), &title);
                // The release ending the press that opened this menu must not close it again.
                let opening = title.clicked() && ui.ctx().data(|d| d.get_temp::<bool>(press_gesture_id())).unwrap_or(false);
                let close = if opening { egui::PopupCloseBehavior::IgnoreClicks } else { config.close_behavior };
                egui::Popup::menu(&title)
                    .open_memory(open)
                    .close_behavior(close)
                    .style(config.style.clone())
                    .info(egui::UiStackInfo::new(egui::UiKind::Menu).with_tag_value(egui::containers::menu::MenuConfig::MENU_CONFIG_TAG, config))
                    .show(|ui| {
                        ui.set_min_width(220.0);
                        level(ui, &menu.children, 1, mac, bounded, &mut clicked, &mut nav);
                    });
                buttons.push(title);
            }
            switch_on_hover(ui.ctx(), &buttons, hover);
            let tops: Vec<egui::Id> = buttons.iter().map(egui::Popup::default_response_id).collect();
            nav.keys(ui.ctx(), &tops, &mut clicked);
        });
    });
    nav.store(ui.ctx());
    if ui.input(|i| i.pointer.primary_released() || !i.pointer.primary_down()) {
        ui.ctx().data_mut(|d| d.remove::<bool>(press_gesture_id()));
    }
    clicked.map(|key| (key, ui.input(|i| i.modifiers.alt)))
}

/// The open top-level menu, if any (tests, the status line).
pub fn open_menu(ctx: &egui::Context) -> Option<usize> {
    Nav::current(ctx).top
}

fn press_gesture_id() -> egui::Id {
    egui::Id::new("proto-menu-press-gesture")
}

/// The press opens a closed menu (and starts a press-drag gesture) or closes an open one; the
/// release never toggles, so the menu doesn't blink.
fn title_press(ctx: &egui::Context, title: &egui::Response) -> Option<egui::SetOpenCommand> {
    let pressed = ctx.input(|i| i.pointer.primary_pressed()) && (title.is_pointer_button_down_on() || title.clicked());
    if !pressed {
        return None;
    }
    let open = egui::Popup::is_id_open(ctx, egui::Popup::default_response_id(title));
    if !open {
        ctx.data_mut(|d| d.insert_temp(press_gesture_id(), true));
    }
    Some(egui::SetOpenCommand::Bool(!open))
}

/// Did the press-drag gesture that opened the menus end on `item`?
fn released_on(ui: &egui::Ui, item: &egui::Response) -> bool {
    item.enabled()
        && item.contains_pointer()
        && ui.input(|i| i.pointer.primary_released())
        && ui.ctx().data(|d| d.get_temp::<bool>(press_gesture_id())).unwrap_or(false)
}

/// Can the pointer reach this title? In v0.3.0 only when the title's own layer is top-most there,
/// so a submenu drawn over the title hides it. v0.2.0 checked the rect alone: that is #92.
fn pointer_reaches_title(ctx: &egui::Context, response: &egui::Response, p: egui::Pos2, hover: Hover) -> bool {
    response.interact_rect.contains(p) && (hover == Hover::Legacy || ctx.layer_id_at(p) == Some(response.layer_id))
}

/// While one menu is open, moving the pointer onto another title opens that one instead, like a
/// native menu bar. egui doesn't do this on its own.
fn switch_on_hover(ctx: &egui::Context, buttons: &[egui::Response], hover: Hover) {
    let ids: Vec<egui::Id> = buttons.iter().map(egui::Popup::default_response_id).collect();
    let Some(open) = ids.iter().position(|id| egui::Popup::is_id_open(ctx, *id)) else { return };
    let Some(p) = ctx.pointer_hover_pos() else { return };
    // Only a moving pointer switches: one resting on a title must not undo ← / →.
    if ctx.input(|i| i.pointer.delta() == egui::Vec2::ZERO) {
        return;
    }
    if let Some(i) = buttons.iter().position(|b| pointer_reaches_title(ctx, b, p, hover))
        && i != open
    {
        egui::Popup::open_id(ctx, ids[i]);
        ctx.request_repaint();
    }
}

fn level(ui: &mut egui::Ui, nodes: &[Node], depth: usize, mac: bool, bounded: bool, clicked: &mut Option<String>, nav: &mut Nav) {
    menu_nav::level(ui, depth, nav, bounded, |ui, nav| rows(ui, nodes, depth, mac, bounded, clicked, nav));
}

fn rows(ui: &mut egui::Ui, nodes: &[Node], depth: usize, mac: bool, bounded: bool, clicked: &mut Option<String>, nav: &mut Nav) {
    // Rows never wrap: the menu widens to its longest label plus shortcut.
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
    // PhotoCraft's menu look: a blue highlight row with white text, and roomier rows.
    let v = &mut ui.style_mut().visuals;
    let accent = egui::Color32::from_rgb(56, 117, 215);
    v.widgets.hovered.weak_bg_fill = accent;
    v.widgets.hovered.bg_fill = accent;
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    v.widgets.hovered.corner_radius = egui::CornerRadius::same(3);
    ui.spacing_mut().button_padding = egui::vec2(10.0, 4.0);
    for node in nodes {
        match node {
            Node::Separator => {
                ui.separator();
            }
            Node::Item(it) => {
                let text = match it.checked {
                    Some(true) => format!("✔ {}", it.label),
                    Some(false) => format!("   {}", it.label),
                    None => it.label.clone(),
                };
                let mut b = egui::Button::new(text);
                if let Some(c) = it.shortcut.as_deref().and_then(Chord::parse) {
                    b = b.shortcut_text(c.display(mac));
                }
                let key = it.key();
                let hit = nav.row(ui, depth - 1, it.enabled, Some(&key), |ui, _| {
                    let r = ui.add_enabled(it.enabled, b);
                    let hit = r.clicked() || released_on(ui, &r);
                    (r, hit)
                });
                if hit {
                    *clicked = Some(key);
                    ui.close();
                }
            }
            Node::Submenu { label, children, .. } => {
                let enabled = !children.is_empty();
                ui.add_enabled_ui(enabled, |ui| {
                    nav.row(ui, depth - 1, enabled, None, |ui, nav| {
                        let r = ui.menu_button(label.as_str(), |ui| level(ui, children, depth + 1, mac, bounded, clicked, nav));
                        if r.inner.is_some() {
                            // Where the submenu hangs from, to keep it below the bar (#319).
                            nav.set_anchor(depth, r.response.rect);
                        }
                        (r.response, ())
                    });
                });
            }
            // Platform items exist only in the native macOS layout.
            Node::Standard { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{FakeHost, MenuHost};
    use egui_kittest::{Harness, kittest::Queryable};

    struct State {
        bar: MenuBar,
        hover: Hover,
        chosen: Vec<Chosen>,
    }

    /// PhotoCraft's menu with a document open, in a window the size of the #92 report's display.
    fn harness(hover: Hover) -> Harness<'static, State> {
        let state = State { bar: FakeHost::photocraft().menu_bar(), hover, chosen: Vec::new() };
        let mut h = Harness::builder().with_size(egui::vec2(1280.0, 703.0)).build_ui_state(
            |ui, s: &mut State| {
                if let Some(c) = menu_bar(ui, &s.bar, s.hover) {
                    s.chosen.push(c);
                }
            },
            state,
        );
        h.ctx.all_styles_mut(|s| s.scroll_animation = egui::style::ScrollAnimation::none());
        h.run_steps(3);
        h
    }

    fn title(h: &Harness<'static, State>, name: &str) -> egui::Rect {
        h.query_all_by_label(name).map(|n| n.rect()).min_by(|a, b| a.top().total_cmp(&b.top())).expect("menu title")
    }

    fn popups(h: &Harness<'static, State>) -> Vec<egui::Rect> {
        h.ctx
            .memory(|m| m.areas().visible_layer_ids())
            .into_iter()
            .filter(|l| l.order == egui::Order::Foreground)
            .filter_map(|layer| h.ctx.memory(|m| m.area_rect(layer.id)))
            .collect()
    }

    /// Open Image › Adjustments, the tall submenu from #319.
    fn open_adjustments(h: &mut Harness<'static, State>) {
        h.get_by_label("Image").click();
        h.run_steps(4);
        let row = h.query_all_by_label_contains("Adjustments").map(|n| n.rect()).max_by(|a, b| a.top().total_cmp(&b.top())).expect("row");
        h.hover_at(row.center() + egui::vec2(-20.0, 0.0));
        h.run_steps(6);
    }

    #[test]
    fn clicking_an_item_returns_its_key() {
        let mut h = harness(Hover::Fixed);
        h.get_by_label("File").click();
        h.run_steps(4);
        h.get_by_label_contains("Open…").click();
        h.run_steps(3);
        assert_eq!(h.state().chosen.first().map(|c| c.0.as_str()), Some("file.open"));
    }

    #[test]
    fn hovering_another_title_switches_menus() {
        let mut h = harness(Hover::Fixed);
        h.get_by_label("File").click();
        h.run_steps(4);
        assert_eq!(open_menu(&h.ctx), Some(0));
        let edit = title(&h, "Edit");
        h.hover_at(edit.center() - egui::vec2(3.0, 0.0));
        h.hover_at(edit.center());
        h.run_steps(4);
        assert_eq!(open_menu(&h.ctx), Some(1));
    }

    /// Open each submenu of the tallest menus in turn (rows low in a menu open upward) until one
    /// covers the menu bar. Returns that submenu's rect and the open top menu, or `None`.
    fn find_covering_submenu(h: &mut Harness<'static, State>) -> Option<(egui::Rect, usize)> {
        for top in ["Filter", "Layer", "Image", "Edit"] {
            h.get_by_label(top).click();
            h.run_steps(4);
            let subs: Vec<egui::Rect> = Nav::current(&h.ctx).rows.first().cloned().unwrap_or_default().iter()
                .filter(|r| r.enabled && r.command.is_none())
                .map(|r| r.rect)
                .collect();
            for row in subs {
                h.hover_at(row.center() + egui::vec2(-20.0, 0.0));
                h.run_steps(6);
                let bar_bottom = Nav::current(&h.ctx).bar_bottom.unwrap();
                if let Some(sub) = popups(h).into_iter().find(|r| r.top() < bar_bottom - 0.5) {
                    return Some((sub, open_menu(&h.ctx).unwrap()));
                }
            }
            h.key_press(egui::Key::Escape);
            h.run_steps(3);
        }
        None
    }

    /// #92 and #319: with v0.2.0's behaviour a tall submenu slides up over the menu titles, and
    /// moving the pointer inside it, across a title underneath, opens a different menu.
    #[test]
    fn legacy_hover_reproduces_photocraft_92() {
        let mut h = harness(Hover::Legacy);
        let (sub, open) = find_covering_submenu(&mut h).expect("a submenu covers the bar (#319)");
        let names: Vec<String> = h.state().bar.menus.iter().map(|m| m.title.clone()).collect();
        let (i, covered) = names
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != open)
            .map(|(i, t)| (i, title(&h, t)))
            .find(|(_, r)| sub.intersects(r.shrink(2.0)))
            .expect("a covered title");
        let p = sub.intersect(covered).center();
        h.hover_at(p - egui::vec2(4.0, 0.0));
        h.hover_at(p);
        h.run_steps(4);
        assert_eq!(open_menu(&h.ctx), Some(i), "moving inside the submenu opened {} (#92)", names[i]);
    }

    #[test]
    fn fixed_hover_never_covers_the_bar() {
        let mut h = harness(Hover::Fixed);
        assert_eq!(find_covering_submenu(&mut h), None);
    }

    #[test]
    fn fixed_hover_keeps_every_menu_below_the_bar() {
        let mut h = harness(Hover::Fixed);
        open_adjustments(&mut h);
        let bar_bottom = Nav::current(&h.ctx).bar_bottom.unwrap();
        let open = popups(&h);
        assert!(open.len() >= 2, "Image and its Adjustments submenu are open");
        for r in open {
            assert!(r.top() >= bar_bottom - 0.5, "popup {r:?} covers the bar (bottom {bar_bottom})");
        }
        assert_eq!(open_menu(&h.ctx), Some(2), "Image stays open");
    }
}
