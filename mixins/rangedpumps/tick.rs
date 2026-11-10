// One server tick of a pump: advance the scan and drain a source block.
fn observe_block_entity_tick(
    game: &mut crate::game::Game<'_>,
    pos: [i32; 3],
    entity: &mut horizon_client::containers::ModBlockEntity,
) {
    crate::game::rangedpumps_state::on_block_entity_tick(game, pos, entity);
}
