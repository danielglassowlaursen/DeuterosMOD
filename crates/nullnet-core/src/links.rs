//! Encrypted links between a crew's citadels. Port of the MTX
//! (Godot/Code/Platform/Screens/MTX.cs, `UpdateMTX`).
//!
//! A citadel with an encrypted link installed can be pointed at another of
//! the crew's linked citadels, with a set of items to send there and a set
//! to balance between the two. Each day the link handles one of those
//! items, taking them in turn: sending moves all that fits under the
//! target's cap; balancing evens the two stores out.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::command::CommandError;
use crate::data::GameData;
use crate::ids::{HostId, PlayerId};
use crate::items::ItemType;
use crate::site::Citadel;
use crate::store::Store;
use crate::world::{Controller, World};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkConfig {
    pub target: Option<HostId>,
    pub send: BTreeSet<ItemType>,
    /// Items in both sets are balanced.
    pub balance: BTreeSet<ItemType>,
    /// The item the link handled last.
    pub last: Option<ItemType>,
}

/// The crew's citadel at a host: the hideout's on the home host.
fn citadel<'a>(
    data: &GameData,
    world: &'a World,
    owner: PlayerId,
    host: HostId,
) -> Option<&'a Citadel> {
    if host == data.hideout.host {
        world.players.get(&owner).map(|p| &p.hideout.citadel)
    } else {
        world
            .hosts
            .get(usize::from(host.0))
            .filter(|h| h.controller == Some(Controller::Crew(owner)))
            .map(|h| &h.site.citadel)
    }
}

fn citadel_mut<'a>(
    data: &GameData,
    world: &'a mut World,
    owner: PlayerId,
    host: HostId,
) -> Option<&'a mut Citadel> {
    if host == data.hideout.host {
        world
            .players
            .get_mut(&owner)
            .map(|p| &mut p.hideout.citadel)
    } else {
        world
            .hosts
            .get_mut(usize::from(host.0))
            .filter(|h| h.controller == Some(Controller::Crew(owner)))
            .map(|h| &mut h.site.citadel)
    }
}

fn linked(citadel: Option<&Citadel>) -> bool {
    citadel.is_some_and(|c| c.complete() && c.encrypted_link)
}

/// Installs an encrypted link from a citadel's own store. The original had
/// no way for the player to do this; it only came with captured stations.
pub(crate) fn install(
    data: &GameData,
    world: &mut World,
    owner: PlayerId,
    host: HostId,
) -> Result<(), CommandError> {
    let citadel = citadel_mut(data, world, owner, host).ok_or(CommandError::NotYourHost(host))?;
    if !citadel.complete() {
        return Err(CommandError::NoCitadel);
    }
    if citadel.encrypted_link {
        return Err(CommandError::AlreadyComplete);
    }
    if !citadel.store.take(ItemType::EncryptedLink, 1) {
        return Err(CommandError::MissingResources(ItemType::EncryptedLink));
    }
    citadel.encrypted_link = true;
    Ok(())
}

/// Points a linked citadel at another and sets what to send and balance.
pub(crate) fn configure(
    data: &GameData,
    world: &mut World,
    owner: PlayerId,
    host: HostId,
    config: LinkConfig,
) -> Result<(), CommandError> {
    if !linked(citadel(data, world, owner, host)) {
        return Err(CommandError::NoLink(host));
    }
    if let Some(target) = config.target
        && (target == host || !linked(citadel(data, world, owner, target)))
    {
        return Err(CommandError::NoLink(target));
    }
    let citadel = citadel_mut(data, world, owner, host).expect("checked above");
    citadel.link = LinkConfig {
        last: None,
        ..config
    };
    Ok(())
}

/// One day of every crew's encrypted links, in host order with each
/// crew's hideout first.
pub(crate) fn run_day(data: &GameData, world: &mut World) {
    let mut links = Vec::new();
    for &owner in world.players.keys() {
        links.push((owner, data.hideout.host));
    }
    for (index, host) in world.hosts.iter().enumerate() {
        if let Some(Controller::Crew(owner)) = host.controller {
            links.push((owner, HostId(index as u16)));
        }
    }
    for (owner, host) in links {
        transfer(data, world, owner, host);
    }
}

fn transfer(data: &GameData, world: &mut World, owner: PlayerId, host: HostId) {
    let Some(source) = citadel(data, world, owner, host).filter(|c| linked(Some(c))) else {
        return;
    };
    let config = &source.link;
    let Some(target) = config.target else {
        return;
    };
    let items: BTreeSet<ItemType> = config.send.union(&config.balance).copied().collect();
    // The next listed item after the last one handled, wrapping around.
    let Some(&item) = config
        .last
        .and_then(|last| {
            items
                .range((std::ops::Bound::Excluded(last), std::ops::Bound::Unbounded))
                .next()
        })
        .or_else(|| items.iter().next())
    else {
        return;
    };
    let balance = config.balance.contains(&item);
    let here = source.store.get(item);
    let Some(there) = citadel(data, world, owner, target)
        .filter(|c| linked(Some(c)))
        .map(|c| c.store.get(item))
    else {
        return;
    };

    // Unlike the original's (a + b / 2), balancing never creates units.
    let moved = if balance {
        let total = here + there;
        let target_share = total / 2;
        target_share as i64 - there as i64
    } else {
        i64::from(here.min(Store::CAP - there.min(Store::CAP)))
    };

    let source = citadel_mut(data, world, owner, host).expect("checked above");
    source.link.last = Some(item);
    match moved {
        0 => {}
        n if n > 0 => {
            source.store.take(item, n as u32);
            citadel_mut(data, world, owner, target)
                .expect("checked above")
                .store
                .add(item, n as u32);
        }
        n => {
            source.store.add(item, (-n) as u32);
            citadel_mut(data, world, owner, target)
                .expect("checked above")
                .store
                .take(item, (-n) as u32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::turn::{Orders, TurnReport, resolve_turn};

    const CREW: PlayerId = PlayerId(0);

    fn orders(data: &GameData, world: &mut World, commands: Vec<Command>) -> TurnReport {
        resolve_turn(data, world, &Orders::from([(CREW, commands)]), 0)
    }

    /// A crew with linked citadels in its hideout and on Mars.
    fn setup() -> (GameData, World, HostId, HostId) {
        let data = GameData::classic();
        let mut world = World::new_game(&data, 4, &[(CREW, "Crew")]);
        let home = data.hideout.host;
        let transit = HostId(data.hosts.iter().position(|h| h.name == "Transit").unwrap() as u16);
        let hideout = &mut world.players.get_mut(&CREW).unwrap().hideout.citadel;
        hideout.modules = 8;
        hideout.store.add(ItemType::EncryptedLink, 1);
        let host = &mut world.hosts[usize::from(transit.0)];
        host.controller = Some(Controller::Crew(CREW));
        host.site.citadel.modules = 8;
        host.site.citadel.store.add(ItemType::EncryptedLink, 1);
        let report = orders(
            &data,
            &mut world,
            vec![
                Command::InstallLink { host: home },
                Command::InstallLink { host: transit },
            ],
        );
        assert!(report.rejected.is_empty(), "{:?}", report.rejected);
        (data, world, home, transit)
    }

    fn count(data: &GameData, world: &World, host: HostId, item: ItemType) -> u32 {
        citadel(data, world, CREW, host).unwrap().store.get(item)
    }

    fn link(home: HostId, transit: HostId, send: Vec<ItemType>, balance: Vec<ItemType>) -> Command {
        Command::ConfigureLink {
            host: home,
            target: Some(transit),
            send,
            balance,
        }
    }

    #[test]
    fn a_link_sends_one_item_a_day_in_turn() {
        let (data, mut world, home, transit) = setup();
        let citadel = &mut world.players.get_mut(&CREW).unwrap().hideout.citadel;
        citadel.store.add(ItemType::Compute, 700);
        citadel.store.add(ItemType::Code, 30);
        let report = orders(
            &data,
            &mut world,
            vec![link(
                home,
                transit,
                vec![ItemType::Compute, ItemType::Code],
                vec![],
            )],
        );
        assert!(report.rejected.is_empty(), "{:?}", report.rejected);

        resolve_turn(&data, &mut world, &Orders::new(), 1);
        assert_eq!(count(&data, &world, transit, ItemType::Compute), 700);
        assert_eq!(count(&data, &world, transit, ItemType::Code), 0);
        resolve_turn(&data, &mut world, &Orders::new(), 1);
        assert_eq!(count(&data, &world, transit, ItemType::Code), 30);
        assert_eq!(count(&data, &world, home, ItemType::Compute), 0);
    }

    #[test]
    fn balancing_evens_the_stores_without_creating_units() {
        let (data, mut world, home, transit) = setup();
        world
            .players
            .get_mut(&CREW)
            .unwrap()
            .hideout
            .citadel
            .store
            .add(ItemType::Crypto, 1001);
        world.hosts[usize::from(transit.0)]
            .site
            .citadel
            .store
            .add(ItemType::Crypto, 0);
        orders(
            &data,
            &mut world,
            vec![link(home, transit, vec![], vec![ItemType::Crypto])],
        );
        resolve_turn(&data, &mut world, &Orders::new(), 1);
        assert_eq!(count(&data, &world, home, ItemType::Crypto), 501);
        assert_eq!(count(&data, &world, transit, ItemType::Crypto), 500);
    }

    #[test]
    fn links_need_linked_citadels_on_both_ends() {
        let (data, mut world, home, transit) = setup();
        let colossus = HostId(
            data.hosts
                .iter()
                .position(|h| h.name == "Colossus")
                .unwrap() as u16,
        );
        world.hosts[usize::from(transit.0)]
            .site
            .citadel
            .encrypted_link = false;
        let report = orders(
            &data,
            &mut world,
            vec![
                link(home, transit, vec![ItemType::Compute], vec![]),
                link(home, home, vec![ItemType::Compute], vec![]),
                link(colossus, home, vec![], vec![]),
                Command::InstallLink { host: home },
            ],
        );
        let errors: Vec<_> = report.rejected.iter().map(|r| r.error.clone()).collect();
        assert_eq!(
            errors,
            [
                CommandError::NoLink(transit),
                CommandError::NoLink(home),
                CommandError::NoLink(colossus),
                CommandError::AlreadyComplete,
            ]
        );
    }
}
