// After the level's tick: push each pump's tank into its neighbouring fluid
// handlers (`native_level_tick_post`, API 23).
fn observe_level_tick_post(game: &mut crate::game::Game<'_>, dimension: &str) {
    crate::game::rangedpumps_state::on_level_tick_post(game, dimension);
}
