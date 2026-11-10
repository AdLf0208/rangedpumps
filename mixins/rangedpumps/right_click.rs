// A right click on the pump reports its state and tank.
fn observe_right_click_block(
    game: &mut crate::game::Game<'_>,
    pos: [i32; 3],
    face: [i32; 3],
    canceled: &mut bool,
) {
    crate::game::rangedpumps_state::on_right_click_block(game, pos, face, canceled);
}
