use crate::storage::*;
use log::*;
use screeps::{
    find, prelude::*, ConstructionSite, ObjectId, Part, ResourceType, ReturnCode, Room, RoomObject,
    RoomObjectProperties, Source, StructureController, StructureExtension, StructureObject,
    StructureType,
};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

pub struct Creep<'a> {
    inner_creep: &'a screeps::Creep,
}
impl<'a> Creep<'a> {
    pub fn new(creep: &'a screeps::Creep) -> Self {
        Self { inner_creep: creep }
    }
    pub fn name(&self) -> String {
        self.inner_creep.name()
    }
    pub fn spawning(&self) -> bool {
        self.inner_creep.spawning()
    }
    pub fn store(&self) -> screeps::Store {
        self.inner_creep.store()
    }
    pub fn say(&self, msg: &str, public: bool) -> ReturnCode {
        self.inner_creep.say(msg, public)
    }
    pub fn pos(&self) -> screeps::Position {
        self.inner_creep.pos()
    }
    pub fn build(&self, target: &ConstructionSite) -> ReturnCode {
        self.inner_creep.build(target)
    }
    pub fn repair(&self, target: &RoomObject) -> ReturnCode {
        self.inner_creep.repair(target)
    }
    pub fn move_to<T>(&self, target: T) -> ReturnCode
    where
        T: HasPosition,
    {
        self.inner_creep.move_to(target)
    }
    pub fn harvest<T>(&self, target: &T) -> ReturnCode
    where
        T: ?Sized + Harvestable,
    {
        self.inner_creep.harvest(target)
    }
    pub fn upgrade_controller(&self, target: &StructureController) -> ReturnCode {
        self.inner_creep.upgrade_controller(target)
    }
    pub fn transfer<T>(&self, target: &T, ty: ResourceType, amount: Option<u32>) -> ReturnCode
    where
        T: Transferable,
    {
        self.inner_creep.transfer(target, ty, amount)
    }
    pub fn room(&self) -> Option<Room> {
        self.inner_creep.room()
    }
    pub fn pick_random_energy_source(&self) -> Option<ObjectId<screeps::Source>> {
        // let's pick a random energy source
        let room = self
            .inner_creep
            .room()
            .expect("couldn't resolve creep room");
        let sources = room.find(find::SOURCES_ACTIVE);
        let rnd_number = rnd_source_idx(sources.len());

        if let Some(source) = sources.get(rnd_number) {
            return Some(source.id());
        }
        None
    }
    pub fn get_value_to_transfer(&self, target_store: &screeps::Store) -> u32 {
        let mut value_to_transfer = self
            .inner_creep
            .store()
            .get_used_capacity(Some(ResourceType::Energy));
        let target_free_store: u32 = target_store
            .get_free_capacity(Some(ResourceType::Energy))
            .try_into()
            .expect("could not convert i32 to u32");

        if target_free_store < value_to_transfer {
            value_to_transfer = target_free_store;
        }
        value_to_transfer
    }
    pub fn find_unfilled_extension(&self) -> Option<StructureExtension> {
        for s in self
            .inner_creep
            .room()
            .unwrap()
            .find(find::MY_STRUCTURES)
            .iter()
        {
            match s {
                StructureObject::StructureExtension(ext) => {
                    if ext.store().get_free_capacity(Some(ResourceType::Energy)) > 0 {
                        return Some(ext.clone());
                    }
                }
                _ => {}
            }
        }
        None
    }
    pub fn run_creep(&self, creep_targets: &mut HashMap<String, CreepTarget>) {
        let name = self.name();
        if self.spawning() {
            return;
        }
        let room = self.room().unwrap();

        let target = creep_targets.get(&name);
        match target {
            Some(creep_target) => {
                match &creep_target {
                    CreepTarget::UpgradeController(controller_id) => {
                        if self.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                            match controller_id.resolve() {
                                Some(controller) => {
                                    let r = self.upgrade_controller(&controller);
                                    if r == ReturnCode::NotInRange {
                                        self.move_to(&controller);
                                    } else if r != ReturnCode::Ok {
                                        warn!("(upgrade controller)couldn't upgrade: {:?}", r);
                                        creep_targets.remove(&name);
                                    }
                                }
                                None => warn!("couldn't file controller with id {}", controller_id),
                            }
                        } else {
                            creep_targets.remove(&name);
                            // HARVEST random energy source
                            if let Some(source_id) = self.pick_random_energy_source() {
                                creep_targets.insert(name, CreepTarget::Harvest(source_id));
                            } else {
                                warn!("could not find source with id");
                            }
                        }
                    }
                    CreepTarget::Harvest(source_id) => {
                        if self.store().get_free_capacity(Some(ResourceType::Energy)) > 0 {
                            match source_id.resolve() {
                                Some(source) => {
                                    if self.pos().is_near_to(source.pos()) {
                                        let r = self.harvest(&source);
                                        if r != ReturnCode::Ok {
                                            warn!("couldn't harvest: {:?}", r);
                                        }
                                    } else {
                                        self.move_to(&source);
                                    }
                                }
                                None => warn!("couldn't file controller with id {}", source_id),
                            }
                        } else {
                            creep_targets.remove(&name);
                        }
                    }
                    CreepTarget::DepositSpawn(spawn_id) => {
                        if self.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                            match spawn_id.resolve() {
                                Some(spawn) => {
                                    if self.pos().is_near_to(spawn.pos()) {
                                        let value_to_transfer =
                                            self.get_value_to_transfer(&spawn.store());
                                        let r = self.transfer(
                                            &spawn,
                                            ResourceType::Energy,
                                            Some(value_to_transfer),
                                        );
                                        if r == ReturnCode::Full {
                                            info!(
                                                "spawn {} is FULL, will do something else",
                                                spawn_id
                                            );
                                            creep_targets.remove(&name);
                                        } else if r != ReturnCode::Ok {
                                            warn!(
                                            "could not make a transfer from creep {}, to spawn {} code: {:?}",
                                            self.name(),
                                            spawn_id,
                                            r
                                        );
                                            creep_targets.remove(&name);
                                        }
                                    } else {
                                        let r = self.move_to(&spawn);
                                        match r {
                                            ReturnCode::Ok => {}
                                            ReturnCode::Tired => {
                                                self.say("got tired", false);
                                            }
                                            code => {
                                                warn!("could not move to spawn code: {:?}", code);
                                            }
                                        }
                                    }
                                }
                                None => {
                                    warn!("could not resolve spawn with id {}", spawn_id);
                                }
                            }
                        } else {
                            creep_targets.remove(&name);
                        }
                    }
                    CreepTarget::DepositExtension(ext) => {
                        if self.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                            if self.pos().is_near_to(ext.pos()) {
                                let value_to_transfer = self.get_value_to_transfer(&ext.store());
                                let r = self.transfer(
                                    ext,
                                    ResourceType::Energy,
                                    Some(value_to_transfer),
                                );
                                match r {
                                    ReturnCode::Ok => {
                                        creep_targets.remove(&name);
                                        if self
                                            .store()
                                            .get_used_capacity(Some(ResourceType::Energy))
                                            > 0
                                        {
                                            if let Some(ext) = self.find_unfilled_extension() {
                                                creep_targets.insert(
                                                    self.name(),
                                                    CreepTarget::DepositExtension(ext),
                                                );
                                            }
                                        }
                                    }
                                    ReturnCode::Full => {
                                        self.say("FULL", false);
                                        creep_targets.remove(&name);
                                    }
                                    code => {
                                        warn!("could not transfer to extension code: {:?}", code);
                                    }
                                }
                            } else {
                                let r = self.move_to(&ext);
                                match r {
                                    ReturnCode::Ok => {}
                                    ReturnCode::Tired => {
                                        self.say("got tired", false);
                                    }
                                    code => {
                                        warn!("could not move to extension code: {:?}", code);
                                    }
                                }
                            }
                        } else {
                            creep_targets.remove(&name);
                        }
                    }
                    CreepTarget::UpgradeConstructionSite(site) => {
                        if self.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                            let r = self.build(&site);
                            if r == ReturnCode::NotInRange {
                                self.move_to(&site);
                            } else if r != ReturnCode::Ok {
                                warn!("(upgrade site) couldn't upgrade: {:?}", r);
                                self.say("CANNOT UPG", false);
                                creep_targets.remove(&name);
                            }
                        } else {
                            creep_targets.remove(&name);
                            // HARVEST random energy source
                            if let Some(source_id) = self.pick_random_energy_source() {
                                creep_targets.insert(name, CreepTarget::Harvest(source_id));
                            } else {
                                warn!("could not find source with id");
                            }
                        }
                    }
                    CreepTarget::RepairRoad(road_id) => {
                        if self.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                            let target = road_id.resolve().unwrap();
                            if target.hits() == target.hits_max() {
                                creep_targets.remove(&name);
                            }
                            let r = self.repair(&target);
                            if r == ReturnCode::NotInRange {
                                self.move_to(&target);
                            } else if r != ReturnCode::Ok {
                                warn!("could not repair road code: {:?}", r);
                                creep_targets.remove(&name);
                            }
                        } else {
                            creep_targets.remove(&name);
                            // HARVEST random energy source
                            if let Some(source_id) = self.pick_random_energy_source() {
                                creep_targets.insert(name, CreepTarget::Harvest(source_id));
                            } else {
                                warn!("could not find source with id");
                            }
                        }
                    }
                    CreepTarget::RepairWall(wall_id) => {
                        if self.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                            let target = wall_id.resolve().unwrap();
                            if target.hits() == target.hits_max() {
                                creep_targets.remove(&name);
                            }
                            let r = self.repair(&target);
                            if r == ReturnCode::NotInRange {
                                self.move_to(&target);
                            } else if r != ReturnCode::Ok {
                                warn!("could not repair wall code: {:?}", r);
                                creep_targets.remove(&name);
                            }
                        } else {
                            creep_targets.remove(&name);
                            // HARVEST random energy source
                            if let Some(source_id) = self.pick_random_energy_source() {
                                creep_targets.insert(name, CreepTarget::Harvest(source_id));
                            } else {
                                warn!("could not find source with id");
                            }
                        }
                    }
                    CreepTarget::Harvester(source_id, object) => {
                        let harvester = Harvester { creep: self };
                        if self.store().get_free_capacity(Some(ResourceType::Energy)) > 0 {
                            if source_id.is_none() {
                                // FIND A SOURCE TO HARVEST
                                if let Some(source_id) = self.pick_random_energy_source() {
                                    creep_targets.insert(
                                        name,
                                        CreepTarget::Harvester(Some(source_id), None),
                                    );
                                    return;
                                } else {
                                    warn!("could not find source with id");
                                }
                            } else {
                                // HARVEST THE SOURCE
                                if !harvester.harvest(source_id.unwrap()) {
                                    warn!("could not resolve source_id");
                                }
                            }
                        } else {
                            // DEPOSIT
                            if object.is_none() {
                                // Find a spawn or an extension
                                if let Some(deposit) = harvester.find_deposit() {
                                    creep_targets
                                        .insert(name, CreepTarget::Harvester(None, Some(deposit)));
                                    return;
                                } else {
                                    warn!("could not find a deposit");
                                    return;
                                }
                            } else {
                                // Actually deposit
                                match object.clone().unwrap() {
                                    StructureObject::StructureExtension(ext) => {
                                        if !harvester.deposit(ext) {
                                            creep_targets
                                                .insert(name, CreepTarget::Harvester(None, None));
                                        }
                                    }
                                    StructureObject::StructureSpawn(spawn) => {
                                        if !harvester.deposit(spawn) {
                                            creep_targets
                                                .insert(name, CreepTarget::Harvester(None, None));
                                        }
                                    }
                                    _ => {
                                        warn!("was expecting a spawn or an extension, received something else");
                                    }
                                }
                            }
                        }
                    }
                };
            }
            None => {
                // no target, let's find one depending on if we have energy
                let room = self.room().expect("couldn't resolve creep room");
                if self.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                    // Upgrade controller, build shit or deposit in a spawn or extension
                    let max = 10;
                    let rnd_number = rnd_source_idx(max);
                    if rnd_number < 1 {
                        // Upgrade Controller
                        for structure in room.find(find::STRUCTURES).iter() {
                            if let StructureObject::StructureController(controller) = structure {
                                creep_targets
                                    .insert(name, CreepTarget::UpgradeController(controller.id()));
                                break;
                            }
                        }
                        return;
                    } else if rnd_number < 5 {
                        // Deposit to spawn
                        for s in room.find(find::MY_SPAWNS).iter() {
                            if s.store().get_free_capacity(Some(ResourceType::Energy)) == 0 {
                                break;
                            }
                            creep_targets.insert(self.name(), CreepTarget::DepositSpawn(s.id()));
                            return;
                        }
                        if self.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                            if let Some(ext) = self.find_unfilled_extension() {
                                creep_targets
                                    .insert(self.name(), CreepTarget::DepositExtension(ext));
                            }
                        }
                    } else {
                        // REPAIR ROADS
                        let objects =
                            self.pos()
                                .find_closest_by_path(find::STRUCTURES)
                                .filter(|r| {
                                    r.structure_type() == StructureType::Road
                                        && r.as_attackable().unwrap().hits()
                                            > r.as_attackable().unwrap().hits_max() / 3
                                });
                        if objects.is_none() {
                            return;
                        }
                        for object in objects.iter() {
                            match object {
                                StructureObject::StructureRoad(road) => {
                                    creep_targets
                                        .insert(name.clone(), CreepTarget::RepairRoad(road.id()));
                                    return;
                                }
                                _ => {}
                            }
                        }
                        // REPAIR WALL
                        let objects =
                            self.pos()
                                .find_closest_by_path(find::STRUCTURES)
                                .filter(|r| {
                                    r.structure_type() == StructureType::Wall
                                        && r.as_attackable().unwrap().hits()
                                            > r.as_attackable().unwrap().hits_max() / 3
                                });
                        if objects.is_none() {
                            return;
                        }
                        for object in objects.iter() {
                            match object {
                                StructureObject::StructureWall(wall) => {
                                    creep_targets
                                        .insert(name.clone(), CreepTarget::RepairWall(wall.id()));
                                    return;
                                }
                                _ => {}
                            }
                        }
                        // Upgrade EXTENSION
                        for site in room.find(find::CONSTRUCTION_SITES).iter() {
                            if site.structure_type() == StructureType::Extension {
                                creep_targets.insert(
                                    name,
                                    CreepTarget::UpgradeConstructionSite(site.clone()),
                                );
                                return;
                            }
                        }
                        // Upgrade RANDOM CONSTRUCTION SITE
                        let site = self.pos().find_closest_by_path(find::CONSTRUCTION_SITES);
                        match site {
                            Some(site) => {
                                creep_targets
                                    .insert(name, CreepTarget::UpgradeConstructionSite(site));
                                return;
                            }
                            _ => {}
                        }
                    }
                } else {
                    // HARVEST random energy source
                    if let Some(source_id) = self.pick_random_energy_source() {
                        creep_targets.insert(name, CreepTarget::Harvest(source_id));
                    } else {
                        warn!("could not find source with id");
                    }
                }
            }
        }
    }
}
/// max is exclusive, i.e for max = 10, [0,10[
fn rnd_source_idx(max: usize) -> usize {
    js_sys::Math::floor(js_sys::Math::random() * max as f64) as usize
}

pub enum BodyType {
    HARVESTER,
    HAULER,
}

impl BodyType {
    pub fn body(&self) -> Vec<Part> {
        match self {
            BodyType::HARVESTER => {
                vec![
                    Part::Carry,
                    Part::Work,
                    Part::Work,
                    Part::Work,
                    Part::Work,
                    Part::Work,
                    Part::Move,
                ]
            }
            BodyType::HAULER => {
                vec![
                    Part::Carry,
                    Part::Carry,
                    Part::Carry,
                    Part::Move,
                    Part::Move,
                    Part::Move,
                    Part::Move,
                    Part::Move,
                    Part::Move,
                ]
            }
        }
    }
}

struct Harvester<'a> {
    pub creep: &'a Creep<'a>,
}
impl<'a> Harvester<'a> {
    pub fn harvest(&self, source_id: ObjectId<Source>) -> bool {
        return match source_id.resolve() {
            Some(source) => {
                if self.creep.pos().is_near_to(source.pos()) {
                    let r = self.creep.harvest(&source);
                    if r != ReturnCode::Ok {
                        warn!("couldn't harvest: {:?}", r);
                    }
                    true
                } else {
                    self.creep.move_to(&source);
                    true
                }
            }
            None => false,
        };
    }

    pub fn find_deposit(&self) -> Option<StructureObject> {
        let room = self.creep.room().unwrap();
        for s in room.find(find::MY_SPAWNS).iter() {
            if s.store().get_free_capacity(Some(ResourceType::Energy)) == 0 {
                break;
            }
            return Some(StructureObject::StructureSpawn(s.clone()));
        }
        if let Some(ext) = self.creep.find_unfilled_extension() {
            Some(StructureObject::StructureExtension(ext))
        } else {
            None
        }
    }

    /// Can return false when it's not done with deposit everything
    /// or because it failed for some reason which should be logged
    pub fn deposit<T>(&self, target: T) -> bool
    where
        T: Transferable + HasStore + HasId,
    {
        if self.creep.pos().is_near_to(target.pos()) {
            let value_to_transfer = self.creep.get_value_to_transfer(&target.store());
            let r = self
                .creep
                .transfer(&target, ResourceType::Energy, Some(value_to_transfer));
            match r {
                ReturnCode::Ok => true,
                ReturnCode::Full => {
                    info!("deposit is full");
                    false
                }
                code => {
                    warn!("could not deposit energy, {:?}", code);
                    false
                }
            }
        } else {
            let r = self.creep.move_to(&target);
            match r {
                ReturnCode::Ok => true,
                ReturnCode::Tired => {
                    self.creep.say("got tired", false);
                    false
                }
                code => {
                    warn!("could not move to spawn code: {:?}", code);
                    false
                }
            }
        }
    }
}
