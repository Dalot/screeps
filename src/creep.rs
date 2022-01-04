use crate::storage::*;
use log::*;
use screeps::{
    find, prelude::*, ConstructionSite, ObjectId, Part, ResourceType, ReturnCode, Room, RoomObject,
    RoomObjectProperties, Source, StructureController, StructureExtension, StructureObject,
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

    /// Will find the nearest unfilled extension
    pub fn find_unfilled_extension(&self) -> Option<StructureExtension> {
        let creep_pos = self.pos();
        let structures = self.inner_creep.room().unwrap().find(find::MY_STRUCTURES);
        let closest_ext_obj = structures
            .iter()
            .filter(|s| screeps::StructureType::Extension == s.structure_type())
            .filter(|s| {
                s.as_has_store()
                    .expect("expected an extension with a store")
                    .store()
                    .get_free_capacity(Some(ResourceType::Energy))
                    > 0
            })
            .reduce(|closer, next| {
                if closer.pos().get_range_to(creep_pos) > next.pos().get_range_to(creep_pos) {
                    next
                } else {
                    closer
                }
            });
        if let Some(ext) = closest_ext_obj {
            match ext {
                StructureObject::StructureExtension(val) => Some(val.clone()),
                _ => {
                    warn!("something went wrong on the filter above");
                    None
                }
            }
        } else {
            warn!("could not find the closest extension");
            None
        }
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
                    CreepTarget::Repair(object_id) => {
                        if self.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                            match object_id.resolve() {
                                Some(obj) => {
                                    let r = self.repair(&obj);
                                    if r == ReturnCode::NotInRange {
                                        self.move_to(&obj);
                                    } else if r != ReturnCode::Ok {
                                        warn!("couldn't repair: {:?}", r);
                                        self.say("CANNOT REP", false);
                                        creep_targets.remove(&name);
                                    }
                                }
                                None => {
                                    warn!("could not resolve object in repair");
                                    creep_targets.remove(&name);
                                }
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
                    CreepTarget::Deposit() => {
                        let spawns = room.find(find::MY_SPAWNS);

                        let spawn = spawns
                            .iter()
                            .filter(|s| s.store().get_free_capacity(Some(ResourceType::Energy)) > 0)
                            .last();

                        let harvester = Harvester { creep: self };
                        let creep_pos = self.pos();

                        if let Some(ext) = self.find_unfilled_extension() {
                            if let Some(s) = spawn {
                                if ext.pos().get_range_to(creep_pos)
                                    < s.pos().get_range_to(creep_pos)
                                {
                                    if DepositCode::Done == harvester.deposit(ext) {
                                        creep_targets.remove(&name);
                                    }
                                } else {
                                    if DepositCode::Done == harvester.deposit(s.clone()) {
                                        creep_targets.remove(&name);
                                    }
                                }
                            } else {
                                if DepositCode::Done == harvester.deposit(ext) {
                                    creep_targets.remove(&name);
                                }
                            }
                            return;
                        } else {
                            if let Some(s) = spawn {
                                if DepositCode::Done == harvester.deposit(s.clone()) {
                                    creep_targets.remove(&name);
                                }
                            }
                        }

                        warn!("could not find spawn or extension to deposit to");
                    }
                    _ => {
                        creep_targets.remove(&name);
                    }
                };
            }
            None => {
                // no target, let's find one depending on if we have energy
                if self.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                    // Upgrade controller, build shit or deposit in a spawn or extension
                    let max = 10;
                    let rnd_number = rnd_source_idx(max);
                    if rnd_number < 0 {
                        // Upgrade Controller
                        for structure in room.find(find::STRUCTURES).iter() {
                            if let StructureObject::StructureController(controller) = structure {
                                creep_targets
                                    .insert(name, CreepTarget::UpgradeController(controller.id()));
                                break;
                            }
                        }
                        return;
                    } else if rnd_number < 10 {
                        creep_targets.insert(name, CreepTarget::Deposit());
                    } else {
                        let max = 2;
                        let rnd_number = rnd_source_idx(max);
                        if rnd_number < 1 {
                            // REPAIR
                            let object = room
                                .find(find::STRUCTURES)
                                .into_iter()
                                .filter(|o| {
                                    if let Some(attackable) = o.as_attackable() {
                                        attackable.hits() < attackable.hits_max() / 3
                                    } else {
                                        false
                                    }
                                })
                                .reduce(|fewer_hp_obj, next_obj| {
                                    // here we are sure we only have only attackables
                                    if let Some(next_attackable) = next_obj.as_attackable() {
                                        if let Some(fewer_hp_atttackble) =
                                            fewer_hp_obj.as_attackable()
                                        {
                                            if next_attackable.hits() < fewer_hp_atttackble.hits() {
                                                next_obj
                                            } else {
                                                fewer_hp_obj
                                            }
                                        } else {
                                            fewer_hp_obj
                                        }
                                    } else {
                                        warn!("could not get one of the attackables");
                                        fewer_hp_obj
                                    }
                                })
                                .take();
                            match object {
                                Some(obj) => {
                                    creep_targets.insert(
                                        name.clone(),
                                        CreepTarget::Repair(obj.as_structure().id()),
                                    );
                                    return;
                                }
                                None => {}
                            }
                        } else {
                            // Upgrade RANDOM CONSTRUCTION SITE but Controller
                            let site = self.pos().find_closest_by_path(find::CONSTRUCTION_SITES);
                            match site {
                                Some(val) => match val.structure_type() {
                                    screeps::StructureType::Controller => {}
                                    _ => {
                                        creep_targets.insert(
                                            name,
                                            CreepTarget::UpgradeConstructionSite(val),
                                        );
                                        return;
                                    }
                                },
                                _ => {}
                            }
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
pub trait CanHarvest {
    fn harvest(&self, source_id: ObjectId<Source>) -> bool;
}
pub trait CanDeposit {
    fn find_deposit(&self) -> Option<StructureObject>;
    fn deposit<T>(&self, target: T) -> bool;
}
#[derive(PartialEq)]
pub enum DepositCode {
    Done = 0,
    NotNear = 1,
    Full = 2,
    Error = 3,
    NotDone = 4,
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
    pub fn deposit<T>(&self, target: T) -> DepositCode
    where
        T: Transferable + HasStore + HasId,
    {
        if self
            .creep
            .store()
            .get_used_capacity(Some(ResourceType::Energy))
            > 0
        {
            if self.creep.pos().is_near_to(target.pos()) {
                let value_to_transfer = self.creep.get_value_to_transfer(&target.store());
                let r = self
                    .creep
                    .transfer(&target, ResourceType::Energy, Some(value_to_transfer));
                match r {
                    ReturnCode::Ok => {
                        info!("deposited {}", value_to_transfer);
                        DepositCode::NotDone
                    }
                    ReturnCode::Full => {
                        info!("deposit is full");
                        DepositCode::Full
                    }
                    code => {
                        warn!("could not deposit energy, {:?}", code);
                        DepositCode::Error
                    }
                }
            } else {
                let r = self.creep.move_to(&target);
                match r {
                    ReturnCode::Ok => DepositCode::NotNear,

                    ReturnCode::Tired => {
                        self.creep.say("got tired", false);
                        DepositCode::NotNear
                    }
                    code => {
                        warn!("could not move to spawn code: {:?}", code);
                        DepositCode::Error
                    }
                }
            }
        } else {
            info!("deposited everything");
            DepositCode::Done
        }
    }
}
