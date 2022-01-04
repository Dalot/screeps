use crate::storage::*;
use log::*;
use screeps::{
    find, game, prelude::*, rooms, ConstructionSite, ObjectId, Part, ResourceType, ReturnCode,
    Room, RoomObject, RoomObjectProperties, Source, StructureController, StructureExtension,
    StructureObject, StructureTower,
};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

// TODO: make roles
// Miners will harvest and drop in a cointainer maybe
// Haulers will transport energy from containers to storage/spawn/extension
// Builders will upgrade and repair things

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
    pub fn pick_closest_energy_source(&self) -> Option<ObjectId<screeps::Source>> {
        let source = self.pos().find_closest_by_path(find::SOURCES_ACTIVE);

        if let Some(val) = source {
            return Some(val.id());
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
    /// Returns an option because it may not find an extension
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
                    warn!("how the hell could this not be a structure extension");
                    None
                }
            }
        } else {
            info!("could not find the closest extension");
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
            Some(creep_target) => match &creep_target {
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
                        match site.structure_type() {
                            screeps::StructureType::Controller => {
                                warn!("you have sent controller and it's not supported");
                            }
                            _ => {
                                let mut r = self.build(&site);
                                match r {
                                    ReturnCode::Ok => {
                                        self.say("BUILD", false);
                                    }
                                    ReturnCode::InvalidTarget => {
                                        // this could be a creep in the same square
                                        // could be that the site is finished upgrading
                                        // could be that there is a logic error that you don't own
                                        // the site
                                        info!("cannot upgrade site");
                                        creep_targets.remove(&name);
                                    }
                                    ReturnCode::NotInRange => {
                                        r = self.move_to(&site);
                                        if r == ReturnCode::Tired {
                                            self.say("tired", false);
                                        } else if r != ReturnCode::Ok {
                                            warn!("could not move to site code: {:?}", r);
                                        }
                                    }
                                    code => {
                                        warn!("(upgrade site) couldn't upgrade: {:?}", code);
                                        self.say("CANNOT UPG", false);
                                        creep_targets.remove(&name);
                                    }
                                }
                            }
                        }
                    } else {
                        creep_targets.remove(&name);
                    }
                }
                CreepTarget::Repair(object_id) => {
                    if self.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                        match object_id.resolve() {
                            Some(obj) => {
                                if obj.hits() == obj.hits_max() {
                                    creep_targets.remove(&name);
                                }
                                let r = self.repair(&obj);
                                self.say("REPAIR", false);
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
                    }
                }
                CreepTarget::Deposit() => {
                    let harvester = Harvester { creep: self };
                    let creeps: Vec<screeps::Creep> = game::creeps().values().collect();
                    if creeps.len() > 6 {
                        if let Some(t) = find_tower(room) {
                            if DepositCode::Done == harvester.deposit(t) {
                                creep_targets.remove(&name);
                            }
                            return;
                        }
                    }
                    let deposit = harvester.find_deposit();
                    if let Some(val) = deposit {
                        match val {
                            StructureObject::StructureExtension(ext) => {
                                if DepositCode::Done == harvester.deposit(ext) {
                                    creep_targets.remove(&name);
                                }
                            }
                            StructureObject::StructureSpawn(s) => {
                                if DepositCode::Done == harvester.deposit(s) {
                                    creep_targets.remove(&name);
                                }
                            }
                            _ => {
                                warn!("how the hell could this be different from a spawn or extension")
                            }
                        }
                    } else {
                        info!("(CreepTarget::Deposit) could not find deposit");
                    }
                }
                CreepTarget::UpgradeController(controller_id) => {
                    if self.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                        match controller_id.resolve() {
                            Some(controller) => {
                                let mut r = self.upgrade_controller(&controller);
                                if r == ReturnCode::NotInRange {
                                    r = self.move_to(&controller);
                                    if r != ReturnCode::Ok {
                                        warn!("could not move to controller code: {:?}", r);
                                    }
                                } else if r != ReturnCode::Ok {
                                    warn!("(upgrade controller) couldn't upgrade: {:?}", r);
                                    self.say("CANNOT UPG", false);
                                    creep_targets.remove(&name);
                                }
                            }
                            None => warn!("could not resolve id for controller"),
                        }
                    } else {
                        creep_targets.remove(&name);
                    }
                }
            },
            None => {
                // no target, let's find one depending on if we have energy
                if self.store().get_free_capacity(Some(ResourceType::Energy)) > 0 {
                    // HARVEST random energy source
                    if let Some(source_id) = self.pick_closest_energy_source() {
                        creep_targets.insert(name, CreepTarget::Harvest(source_id));
                    } else {
                        info!("could not find an active source");
                    }
                } else {
                    // Upgrade controller, build shit or deposit in a spawn or extension
                    let max = 10;
                    let rnd_number = rnd_source_idx(max);
                    if rnd_number < 3 {
                        creep_targets.insert(name, CreepTarget::Deposit());
                    } else {
                        let max2 = 2;
                        let repair_or_build = rnd_source_idx(max2);
                        // we need this change because otherwise we will repair walls infinetly
                        if repair_or_build < 1 {
                            // REPAIR
                            let object = room
                                .find(find::STRUCTURES)
                                .into_iter()
                                .filter(|o| o.as_attackable().is_some())
                                .filter(|o| {
                                    o.structure_type() != screeps::StructureType::Controller
                                })
                                .filter(|o| {
                                    o.as_attackable().unwrap().hits()
                                        < o.as_attackable().unwrap().hits_max() / 3
                                })
                                .reduce(|fewer_hp_obj, next_obj| {
                                    info!("(reduce) {:?}", next_obj.structure_type());
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
                                    info!("({}) will repair {:?}", name, obj.structure_type());
                                    creep_targets
                                        .insert(name, CreepTarget::Repair(obj.as_structure().id()));
                                    return;
                                }
                                None => {
                                    info!("could not find anything to repair");
                                }
                            }
                        } else {
                            // Upgrade RANDOM CONSTRUCTION SITE but Controller
                            let site = self.pos().find_closest_by_path(find::CONSTRUCTION_SITES);
                            match site {
                                Some(val) => match val.structure_type() {
                                    screeps::StructureType::Controller => {
                                        if let Some(controller) = room.controller() {
                                            creep_targets.insert(
                                                name,
                                                CreepTarget::UpgradeController(controller.id()),
                                            );
                                        } else {
                                            warn!("could not find controller");
                                            return;
                                        }
                                    }
                                    _ => {
                                        creep_targets.insert(
                                            name,
                                            CreepTarget::UpgradeConstructionSite(val),
                                        );
                                        return;
                                    }
                                },
                                None => {
                                    warn!("could not find counstruction site");
                                }
                            }
                        }
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
        let spawns = room.find(find::MY_SPAWNS);

        let spawn = spawns
            .iter()
            .filter(|s| s.store().get_free_capacity(Some(ResourceType::Energy)) > 0)
            .last();

        let creep_pos = self.creep.pos();

        if let Some(ext) = self.creep.find_unfilled_extension() {
            if let Some(s) = spawn {
                if ext.pos().get_range_to(creep_pos) < s.pos().get_range_to(creep_pos) {
                    Some(StructureObject::StructureExtension(ext))
                } else {
                    Some(StructureObject::StructureSpawn(s.clone()))
                }
            } else {
                Some(StructureObject::StructureExtension(ext))
            }
        } else {
            if let Some(s) = spawn {
                Some(StructureObject::StructureSpawn(s.clone()))
            } else {
                None
            }
        }
    }

    /// Can return false when it's not done with deposit everything
    /// or because it failed for some reason which should be logged
    pub fn deposit<T>(&self, target: T) -> DepositCode
    where
        T: Transferable + HasStore,
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
                info!("deposit code: {:?}", r);
                match r {
                    ReturnCode::Ok => {
                        info!("deposited {}", value_to_transfer);
                        DepositCode::NotDone
                    }
                    ReturnCode::Full => {
                        info!("deposit is full");
                        DepositCode::Full
                    }
                    _ => {
                        warn!("could not deposit energy, {:?}", r);
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
                    _ => {
                        warn!("(deposit) could not move to target code: {:?}", r);
                        DepositCode::Error
                    }
                }
            }
        } else {
            DepositCode::Done
        }
    }
}

pub fn find_tower(room: Room) -> Option<StructureTower> {
    let structures = room.find(find::MY_STRUCTURES);
    let tower_obj = structures
        .into_iter()
        .filter(|s| s.structure_type() == screeps::StructureType::Tower)
        .filter(|t| {
            t.as_has_store()
                .unwrap()
                .store()
                .get_free_capacity(Some(ResourceType::Energy))
                > 0
        })
        .reduce(|res, next_t| {
            info!("(reduce) {:?}", res.structure_type());
            if next_t
                .as_has_store()
                .unwrap()
                .store()
                .get_free_capacity(Some(ResourceType::Energy))
                < res
                    .as_has_store()
                    .unwrap()
                    .store()
                    .get_free_capacity(Some(ResourceType::Energy))
            {
                next_t
            } else {
                res
            }
        })
        .take();

    match tower_obj {
        Some(t_obj) => match t_obj {
            StructureObject::StructureTower(t) => Some(t),
            _ => {
                warn!("expected only a tower here");
                None
            }
        },
        None => {
            info!("towers seem ok");
            None
        }
    }
}
