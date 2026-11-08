//! The ribbon's one row: its controls answer the pointer, with the groups' names only on hover.

use egui::Event;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pearl::app::App;

#[test]
fn a_control_answers_the_pointer_over_its_group() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 800.0))
        .with_accessibility_check(false)
        .build_ui_state(|ui, app: &mut App| app.show(ui), App::new(None).unwrap());
    h.run_steps(4);
    h.state_mut().set_text("palavra");
    h.run_steps(2);
    let ctx = h.ctx.clone();
    h.state_mut().select(&ctx, 0, 7);
    h.run_steps(2);
    let bold = h.get_by_label_contains("Ctrl+B").rect();
    let at = egui::pos2(bold.center().x as f32, bold.center().y as f32);
    h.event(Event::PointerMoved(at));
    h.run_steps(90);
    // Over a control, its own tip shows, and not the group's.
    assert_eq!(h.query_all_by_label_contains("Negrito").count(), 2, "the button and its tip");
    assert_eq!(h.query_all_by_label("Fonte").count(), 0);
    h.event(Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
    h.step();
    h.event(Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);
    assert_eq!(h.state().text(), "*palavra*");
}
