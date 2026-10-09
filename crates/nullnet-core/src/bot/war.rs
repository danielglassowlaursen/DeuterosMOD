//! The bot at war: daemons and a C2 controller, garrisons in its citadels,
//! and a warship that meets swarms, frees Legacy hosts and, when a rival
//! runs away with the game, raids it.

use crate::command::Command;
use crate::ids::HostId;
use crate::items::ItemType;
use crate::legacy::{self, Fleet};
use crate::raid::{self, RaidGoal};
use crate::transport::{self, Berth, Vessel, VesselId, VesselKind};
use crate::world::Controller;

use super::Bot;

/// Daemons the bot keeps in each of its citadels, once the warship is
/// manned: a speed bump while it is away.
pub(super) const GARRISON_WANTED: u32 = 20;
/// Daemons the warship wants aboard before any go to garrisons.
const WARSHIP_FIRST: u32 = 150;
/// Daemons for the warship before the crew lets the war come.
const WARSHIP_READY: u32 = 100;
/// Daemons in all that let the war come without a warship.
const GARRISONS_READY: u32 = 250;
/// Daemons the bot builds up to, counting its stores, garrisons and vessels.
pub(super) const DAEMONS_WANTED: u32 = 600;
/// Fewest daemons the warship sets out with.
const SORTIE: u32 = 40;
/// Fewest daemons a garrison run carries.
const GARRISON_RUN: u32 = 20;
/// Complete citadels a rival must lead by before the bot raids it.
const RAID_LEAD: usize = 2;

impl<'a> Bot<'a> {
    /// Whether the crew can build daemons.
    pub(super) fn armed(&self) -> bool {
        self.researched(ItemType::Daemon)
    }

    /// Whether the crew is ready to have the Legacy Net come for it: a
    /// warship with daemons aboard or waiting for it, or daemons enough in
    /// its garrisons and stores.
    pub(super) fn armed_for_war(&self) -> bool {
        let warship = self
            .vessels(VesselKind::Worm)
            .filter(|(_, v)| v.c2)
            .map(|(_, v)| v.daemons)
            .max();
        match warship {
            Some(aboard) => aboard + self.above.get(ItemType::Daemon) >= WARSHIP_READY,
            None => self.total_daemons() >= GARRISONS_READY,
        }
    }

    /// The battle power of a side: daemons times its operator's level plus four.
    pub(super) fn power(daemons: u32, level: u8) -> u32 {
        daemons * (u32::from(level) + 4)
    }

    pub(super) fn garrison_of(&self, host: HostId) -> u32 {
        self.world
            .host(host)
            .site
            .citadel
            .store
            .get(ItemType::Daemon)
    }

    /// Daemons the crew has anywhere: stored, garrisoned or aboard.
    pub(super) fn total_daemons(&self) -> u32 {
        let aboard: u32 = self
            .world
            .vessels
            .values()
            .filter(|v| v.owner == self.crew)
            .map(|v| v.daemons)
            .sum();
        let garrisons: u32 = self
            .held_citadels()
            .iter()
            .map(|&h| self.garrison_of(h))
            .sum();
        self.above.get(ItemType::Daemon) + aboard + garrisons
    }

    /// Items the home citadel builds for the war.
    pub(super) fn war_wants(&self, wants: &mut Vec<ItemType>) {
        if self.researched(ItemType::C2Controller)
            && self.above.get(ItemType::C2Controller) == 0
            && !self.vessels(VesselKind::Worm).any(|(_, v)| v.c2)
        {
            wants.push(ItemType::C2Controller);
        }
        if self.armed() && self.total_daemons() < DAEMONS_WANTED {
            wants.push(ItemType::Daemon);
        }
    }

    pub(super) fn holds(&self, host: HostId) -> bool {
        self.world.host(host).controller == Some(Controller::Crew(self.crew))
    }

    /// The swarm heading for or besieging one of the crew's hosts, if any.
    pub(super) fn threat(&self) -> Option<&'a Fleet> {
        self.world
            .legacy
            .fleets
            .iter()
            .find(|f| f.target.is_some_and(|t| self.holds(t)))
    }

    /// The citadel most in need of daemons, a threatened one first, with
    /// how many it lacks.
    pub(super) fn garrison_target(&self) -> Option<(HostId, u32)> {
        let threatened = self.threat().and_then(|f| f.target);
        self.held_citadels()
            .into_iter()
            .filter(|&h| !legacy::under_siege(self.world, h))
            .map(|h| (h, self.garrison_of(h)))
            .filter(|&(_, g)| g < GARRISON_WANTED)
            .min_by_key(|&(h, g)| {
                (
                    Some(h) != threatened,
                    g,
                    transport::latency(self.data, self.home, h),
                )
            })
            .map(|(h, g)| (h, GARRISON_WANTED - g))
    }

    /// Loads daemons for a garrison run and sets off; whether it did.
    pub(super) fn garrison_run(&mut self, id: VesselId, worm: &Vessel) -> bool {
        if !self.armed() {
            return false;
        }
        // The warship comes first; garrisons get what is left.
        let warship_short = self
            .vessels(VesselKind::Worm)
            .any(|(_, v)| v.c2 && v.daemons < WARSHIP_FIRST);
        if warship_short {
            return false;
        }
        let Some((host, lacking)) = self.garrison_target() else {
            return false;
        };
        let load = lacking
            .min(self.above.get(ItemType::Daemon))
            .min(Vessel::DAEMON_CAPACITY - worm.daemons);
        if worm.daemons + load < GARRISON_RUN {
            return false;
        }
        let fuel = worm.fuel + self.refuelled(id);
        if fuel < 2 * transport::latency(self.data, self.home, host) + 4 {
            return false;
        }
        if load > 0 {
            self.above.take(ItemType::Daemon, load);
            self.order(Command::LoadDaemons {
                vessel: id,
                count: load,
            });
        }
        self.dispatch(id, host, Berth::Connected);
        true
    }

    // ------------------------------------------------------------ warship

    pub(super) fn warship_at_home(&mut self, id: VesselId, worm: &Vessel) {
        if !self.worm_ready(id, worm, 120) {
            return;
        }
        let load = (Vessel::DAEMON_CAPACITY - worm.daemons).min(self.above.get(ItemType::Daemon));
        if load > 0 {
            self.above.take(ItemType::Daemon, load);
            self.order(Command::LoadDaemons {
                vessel: id,
                count: load,
            });
        }
        let daemons = worm.daemons + load;
        if daemons < SORTIE {
            return;
        }
        let level = worm.pilot.as_ref().map_or(0, |p| p.level());
        let power = Self::power(daemons, level);
        // Attacking the Legacy Net declares war; not before the crew is
        // ready for it.
        let ready = self.player.war.is_some() || self.armed_for_war();
        let Some(target) = self
            .threat_target()
            .or_else(|| ready.then(|| self.liberation_target(power)).flatten())
            .or_else(|| self.raid_target(power))
        else {
            return;
        };
        let fuel = worm.fuel + self.refuelled(id);
        if fuel >= 2 * transport::latency(self.data, self.home, target) + 4 {
            self.dispatch(id, target, Berth::Lurking);
        }
    }

    /// The host a swarm is heading for, if the warship can be there before
    /// the siege is over.
    fn threat_target(&self) -> Option<HostId> {
        let fleet = self.threat()?;
        let target = fleet.target?;
        if fleet.siege_until.is_some() {
            return Some(target);
        }
        let arrives = fleet.arrives?;
        let there = self.world.day + transport::latency(self.data, self.home, target) + 1;
        (there < arrives + legacy::SIEGE_DAYS).then_some(target)
    }

    /// Whether `power` is enough to take on a garrison of `daemons`
    /// under an operator of `level`.
    fn can_take(power: u32, daemons: u32, level: u8) -> bool {
        power >= Self::power(daemons, level) * 3 / 2
    }

    /// The Legacy host in the home network the warship can free, with the
    /// smallest garrison first.
    fn liberation_target(&self, power: u32) -> Option<HostId> {
        let network = self.data.host(self.home).network;
        self.data
            .hosts
            .iter()
            .enumerate()
            .filter(|(h, def)| {
                def.network == network
                    && self.world.hosts[*h].controller == Some(Controller::Legacy)
            })
            .map(|(h, _)| {
                let id = HostId(h as u16);
                (id, self.garrison_of(id).min(legacy::DAEMON_CAP))
            })
            .filter(|&(_, g)| Self::can_take(power, g, 0))
            .min_by_key(|&(h, g)| (g, transport::latency(self.data, self.home, h)))
            .map(|(h, _)| h)
    }

    /// A citadel of a rival well ahead of the crew that the warship can
    /// take, once crews may raid each other.
    fn raid_target(&self, power: u32) -> Option<HostId> {
        if self.world.turn < raid::PROTECTION_TURNS {
            return None;
        }
        let mine = self.held_citadels().len();
        let network = self.data.host(self.home).network;
        let citadels = |crew| {
            self.world
                .hosts
                .iter()
                .filter(|s| {
                    s.controller == Some(Controller::Crew(crew)) && s.site.citadel.complete()
                })
                .count()
        };
        self.data
            .hosts
            .iter()
            .enumerate()
            .filter_map(|(h, def)| {
                let state = &self.world.hosts[h];
                let Some(Controller::Crew(rival)) = state.controller else {
                    return None;
                };
                if rival == self.crew || def.network != network || !state.site.citadel.complete() {
                    return None;
                }
                if citadels(rival) < mine + RAID_LEAD {
                    return None;
                }
                let garrison = raid::garrison(&state.site.citadel);
                (power >= Self::power(garrison.daemons, garrison.level) * 2)
                    .then_some((HostId(h as u16), garrison.daemons))
            })
            .min_by_key(|&(h, g)| (g, transport::latency(self.data, self.home, h)))
            .map(|(h, _)| h)
    }

    pub(super) fn warship_outside(&mut self, id: VesselId, worm: &Vessel) {
        let host = worm.host;
        let level = worm.pilot.as_ref().map_or(0, |p| p.level());
        let power = Self::power(worm.daemons, level);
        if host != self.home && worm.pilot.is_some() && worm.daemons > 0 {
            let state = self.world.host(host);
            if self.holds(host) && legacy::under_siege(self.world, host) {
                self.order(Command::Attack { vessel: id });
                return;
            }
            if state.controller == Some(Controller::Legacy)
                && (self.player.war.is_some() || self.armed_for_war())
                && Self::can_take(power, self.garrison_of(host).min(legacy::DAEMON_CAP), 0)
            {
                self.order(Command::Attack { vessel: id });
                return;
            }
            if self.raid_target(power) == Some(host) {
                self.order(Command::Raid {
                    vessel: id,
                    goal: RaidGoal::TakeOver,
                });
                return;
            }
            // Wait for a swarm on its way here.
            if self
                .threat()
                .is_some_and(|f| f.target == Some(host) && f.arrives.is_some())
            {
                return;
            }
        }
        if worm.fuel > 0 {
            self.dispatch(id, self.home, Berth::Connected);
        }
    }
}
