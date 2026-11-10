// The pump block declares one 32-bucket tank (`native_block_entity_fluids`,
// API 54), read and written by the fluid helpers in the block entity's saved
// `fluids`.
fn observe_block_entity_fluids(
    block: &str,
    fluids: &mut Option<horizon_client::containers::ModFluidTanks>,
) {
    crate::game::rangedpumps_state::on_block_entity_fluids(block, fluids);
}
