// Declare `/rangedpumps info|range`.
fn observe_register(commands: &mut Vec<NativeCommand>) {
    crate::game::rangedpumps_state::on_register_commands(commands);
}
