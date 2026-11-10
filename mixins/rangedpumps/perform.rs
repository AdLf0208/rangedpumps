// Run the `/rangedpumps` commands a player types.
fn observe_perform(
    game: &mut crate::game::Game<'_>,
    player: &NativeServerPlayer,
    command: &str,
    ran: &mut bool,
) {
    crate::game::rangedpumps_state::on_perform_command(game, player, command, ran);
}
