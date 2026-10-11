use acacia_client::proto::types::{
    ContainerSlotType as T, ItemStackRequestActionsItemContent as Content, ItemStackRequestActionsItemTypeId as TypeId, WindowType,
};

use super::*;
use crate::items::ui;
use crate::workstation::beacon::beacon_plan;
use crate::workstation::BeaconEffect;
use crate::ActionError;

#[test]
fn a_pyramid_counts_its_full_layers() {
    use crate::workstation::beacon::pyramid_levels;
    // Two full layers under a beacon at y 10, the third missing a corner.
    let base = |[x, y, z]: [i32; 3]| match 10 - y {
        n @ 1..=2 => x.abs() <= n && z.abs() <= n,
        3 => x.abs() <= 3 && z.abs() <= 3 && [x, z] != [3, 3],
        _ => false,
    };
    assert_eq!((pyramid_levels(base, [0, 10, 0]), pyramid_levels(base, [1, 10, 0]), pyramid_levels(|_| true, [0, 0, 0])), (2, 0, 4));
}

#[test]
fn beacon_payment_then_destroy() {
    let mut state = state(&[]);
    open(&mut state, WindowType::Beacon);
    assert!(matches!(beacon_plan(&state, BeaconEffect::Speed, None), Err(ActionError::NotPossible(_))), "no payment placed");
    state.inventory.ui[usize::from(ui::BEACON_PAYMENT)] = stack("minecraft:iron_ingot", 1, 40);

    let (head, ops) = beacon_plan(&state, BeaconEffect::Speed, Some(BeaconEffect::Regeneration)).unwrap();
    let packet = Plan::headed(&state, Screen::of(&state, BlockKind::Other), head, &ops).unwrap().request(REQUEST);
    let decoded: ItemStackRequestPacket = raw(&packet).decode().unwrap();
    assert_eq!(decoded, packet);
    let request = &decoded.requests[0];
    assert_eq!(kinds(request), [(TypeId::BeaconPayment, 10), (TypeId::Destroy, 4)]);
    let Content::BeaconPayment(payment) = &request.actions[0].content else { panic!() };
    assert_eq!((payment.primary_effect, payment.secondary_effect), (1, 10));
    let Content::Destroy(destroy) = &request.actions[1].content else { panic!() };
    assert_eq!((destroy.count, destroy.source.clone()), (1, info(T::BeaconPayment, ui::BEACON_PAYMENT, 40)));

    let (head, _) = beacon_plan(&state, BeaconEffect::Haste, Some(BeaconEffect::Haste)).unwrap();
    assert_eq!(head.len(), 1, "the primary power again is level II");
    assert!(matches!(beacon_plan(&state, BeaconEffect::Speed, Some(BeaconEffect::Haste)), Err(ActionError::NotPossible(_))));
}
