// The pump block declares a block entity (`native_block_entity`, API 15).
fn observe_block_entity(
    block: &str,
    entity: &mut Option<(String, horizon_world::nbt::Compound)>,
) {
    if block == "rangedpumps:pump" {
        *entity = Some(("rangedpumps:pump".to_string(), Default::default()));
    }
}
