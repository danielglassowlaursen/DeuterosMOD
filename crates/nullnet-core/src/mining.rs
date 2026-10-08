//! Daily extraction by taps. Port of `Planet.DayTick`
//! (Godot/Code/Objects/Planet.cs).

use crate::data::GameData;
use crate::rng::Rng;
use crate::site::Site;
use crate::world::World;

/// Mines every hideout and every host with taps. Hideouts mine only on even
/// days, as Earth did in the original.
pub(crate) fn run_day(data: &GameData, world: &mut World) {
    let even_day = world.day.is_multiple_of(2);
    for player in world.players.values_mut() {
        if even_day {
            mine_site(data, &mut world.rng, &mut player.hideout);
        }
    }
    for host in &mut world.hosts {
        mine_site(data, &mut world.rng, &mut host.site);
    }
}

/// One day of extraction on a site with taps and a working backdoor.
///
/// For each resource: an empty vein with no survey running starts one of
/// 0-7 days times the resource's survey multiplier; a finished survey finds
/// a vein of up to 32,767 units; a vein with units left gives taps x rate a
/// day. Unlike the original, a tap never takes more than the vein holds.
pub(crate) fn mine_site(data: &GameData, rng: &mut Rng, site: &mut Site) {
    if site.taps == 0 || !site.backdoor_complete() || site.backdoor_damaged {
        return;
    }
    let Site {
        taps,
        veins,
        store,
        citadel,
        ..
    } = site;
    for vein in veins {
        let multiplier = data.survey_multiplier[&vein.resource];
        if vein.amount == 0 {
            if vein.survey_days == 0 {
                vein.survey_days = rng.below(8) * multiplier;
            } else {
                vein.survey_days -= 1;
                if vein.survey_days == 0 {
                    // The 15-bit mask is the original's: rich surveys wrap around.
                    vein.amount = (rng.below(32_768) * multiplier) & 0x7FFF;
                }
            }
        } else {
            let extracted = (*taps * data.tap_rate[&vein.resource]).min(vein.amount);
            vein.amount -= extracted;
            let target = if citadel.encrypted_link {
                &mut citadel.store
            } else {
                &mut *store
            };
            target.add(vein.resource, extracted);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::ItemType;
    use crate::site::Vein;

    fn working_site(veins: Vec<Vein>, taps: u32) -> Site {
        Site {
            backdoor_parts: Site::BACKDOOR_PARTS,
            taps,
            veins,
            ..Site::default()
        }
    }

    fn vein(resource: ItemType, amount: u32, survey_days: u32) -> Vein {
        Vein {
            resource,
            amount,
            survey_days,
        }
    }

    #[test]
    fn taps_extract_rate_times_taps_and_never_more_than_the_vein() {
        let data = GameData::classic();
        let mut rng = Rng::new(1, 1);
        // Compute is extracted at 2 a day per tap.
        let mut site = working_site(vec![vein(ItemType::Compute, 13, 0)], 3);

        mine_site(&data, &mut rng, &mut site);
        assert_eq!(site.store.get(ItemType::Compute), 6);
        mine_site(&data, &mut rng, &mut site);
        mine_site(&data, &mut rng, &mut site);
        assert_eq!(site.store.get(ItemType::Compute), 13);
        assert_eq!(site.veins[0].amount, 0);
    }

    #[test]
    fn a_dry_vein_is_surveyed_and_refilled() {
        let data = GameData::classic();
        let mut rng = Rng::new(7, 3);
        let mut site = working_site(vec![Vein::new(ItemType::Keys)], 1);

        // A fresh vein is one day from its first survey result.
        mine_site(&data, &mut rng, &mut site);
        let first = site.veins[0].amount;
        assert!(first > 0 && first <= 0x7FFF);

        site.veins[0] = vein(ItemType::Keys, 0, 0);
        mine_site(&data, &mut rng, &mut site);
        let survey = site.veins[0].survey_days;
        assert_eq!(survey % 4, 0, "keys survey in steps of their multiplier, 4");
        assert!(survey < 8 * 4);
    }

    #[test]
    fn nothing_is_extracted_without_a_working_backdoor() {
        let data = GameData::classic();
        let mut rng = Rng::new(1, 1);
        let full = vein(ItemType::Compute, 100, 0);
        for mut site in [
            Site {
                backdoor_parts: 1,
                ..working_site(vec![full.clone()], 1)
            },
            Site {
                backdoor_damaged: true,
                ..working_site(vec![full.clone()], 1)
            },
            working_site(vec![full.clone()], 0),
        ] {
            mine_site(&data, &mut rng, &mut site);
            assert_eq!(site.veins[0], full);
            assert_eq!(site.store.get(ItemType::Compute), 0);
        }
    }

    #[test]
    fn an_encrypted_link_delivers_to_the_citadel() {
        let data = GameData::classic();
        let mut rng = Rng::new(1, 1);
        let mut site = working_site(vec![vein(ItemType::Code, 100, 0)], 1);
        site.citadel.encrypted_link = true;

        mine_site(&data, &mut rng, &mut site);
        assert_eq!(site.store.get(ItemType::Code), 0);
        assert_eq!(site.citadel.store.get(ItemType::Code), 2);
    }
}
