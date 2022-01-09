use crate::{roles::harvester::Harvester, roles::role::Role, storage::*};
use log::*;
use screeps::{
    find, game, prelude::*, rooms, ConstructionSite, MoveToOptions, ObjectId, Part, PolyStyle,
    Resource, ResourceType, ReturnCode, Room, RoomObject, RoomObjectProperties, RoomPosition,
    Source, StructureContainer, StructureController, StructureExtension, StructureObject,
    StructureTower, StructureType,
};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

// TODO: make roles
// Miners will harvest and drop in a cointainer maybe
// Haulers will transport energy from containers to storage/spawn/extension
// Builders will upgrade and repair things

#[derive(PartialEq)]
pub enum DepositCode {
    Done = 0,
    NotNear = 1,
    Full = 2,
    Error = 3,
    NotDone = 4,
}
pub struct Creep<'a> {
    inner_creep: &'a screeps::Creep,
    role: Role,
}
impl<'a> Creep<'a> {
    pub fn new(creep: &'a screeps::Creep) -> Self {
        Self {
            inner_creep: creep,
            role: Role::General,
        }
    }
    pub fn set_role(&mut self, role: Option<Role>) {
        if let Some(r) = role {
            self.role = r;
        } else {
            self.role = Role::General;
        }
    }
    pub fn role(&self) -> &Role {
        &self.role
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
    pub fn pickup(&self, target: &Resource) -> ReturnCode {
        self.inner_creep.pickup(target)
    }
    pub fn move_to<T>(&self, target: T) -> ReturnCode
    where
        T: HasPosition,
    {
        let mut options = MoveToOptions::new();
        let mut poly_style = PolyStyle::default();
        poly_style = poly_style
            .fill("transparent")
            .opacity(0.1)
            .stroke("#fff")
            .stroke_width(0.15)
            .line_style(screeps::LineDrawStyle::Dashed);
        options = options.visualize_path_style(poly_style);
        self.inner_creep.move_to_with_options(target, Some(options))
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
        T: Transferable + ?Sized,
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
            .filter(|s| StructureType::Extension == s.structure_type())
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

    fn deposit<T>(&self, target: T) -> DepositCode
    where
        T: Transferable + HasStore,
    {
        if self.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
            if self.pos().is_near_to(target.pos()) {
                let value_to_transfer = self.get_value_to_transfer(&target.store());
                let r = self.transfer(&target, ResourceType::Energy, Some(value_to_transfer));
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
                let r = self.move_to(&target);
                match r {
                    ReturnCode::Ok => DepositCode::NotNear,

                    ReturnCode::Tired => {
                        self.say("TIRED", false);
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
    fn find_deposit(&self) -> Option<StructureObject> {
        let room = self.room().unwrap();
        let spawns = room.find(find::MY_SPAWNS);
        let positions = Vec::<RoomPosition>::new();
        let creep_pos = self.pos();

        let spawn = spawns
            .iter()
            .filter(|s| s.store().get_free_capacity(Some(ResourceType::Energy)) > 0)
            .last();
        let structures = room.find(find::MY_STRUCTURES);
        let storage = room.storage();
        let container_obj = structures
            .iter()
            .filter(|s| s.structure_type() == StructureType::Container)
            .filter(|s| {
                s.as_has_store()
                    .unwrap()
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
        if let Some(ext) = self.find_unfilled_extension() {
            ext.id();
        }

        let mut container: StructureContainer;
        if let Some(obj) = container_obj {
            container = obj_to_container(obj).unwrap();
        }

        // Find which it's closer, the spawn or extension.
        // If there aren't none of them available, store it in the storage
        // TODO: Add container
        if let Some(ext) = self.find_unfilled_extension() {
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
                if let Some(s) = storage {
                    Some(StructureObject::StructureStorage(s))
                } else {
                    None
                }
            }
        }
    }
    pub fn run(&self, creep_targets: &mut HashMap<String, CreepTarget>, has_hostiles: bool) {
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
                                    //ignoring return code for harvest because it already logs
                                    //inside
                                    let _ = self.harvest(&source);
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
                            StructureType::Controller => {
                                warn!("you have sent controller and it's not supported");
                            }
                            _ => {
                                let mut r = self.build(&site);
                                match r {
                                    ReturnCode::Ok => {}
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
                                        } else if r != ReturnCode::Ok {
                                            warn!("could not move to site code: {:?}", r);
                                        }
                                    }
                                    code => {
                                        warn!("(upgrade site) couldn't upgrade: {:?}", code);
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
                                if r == ReturnCode::NotInRange {
                                    self.move_to(&obj);
                                } else if r != ReturnCode::Ok {
                                    warn!("couldn't repair: {:?}", r);
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
                    let creeps: Vec<screeps::Creep> = game::creeps().values().collect();
                    if creeps.len() > 10 || (has_hostiles && creeps.len() > 5) {
                        if let Some(t) = find_tower(room) {
                            if DepositCode::Done == self.deposit(t) {
                                creep_targets.remove(&name);
                            }
                            return;
                        }
                    }
                    let deposit = self.find_deposit();
                    if let Some(val) = deposit {
                        match val {
                            StructureObject::StructureExtension(ext) => {
                                if DepositCode::Done == self.deposit(ext) {
                                    creep_targets.remove(&name);
                                }
                            }
                            StructureObject::StructureSpawn(s) => {
                                if DepositCode::Done == self.deposit(s) {
                                    creep_targets.remove(&name);
                                }
                            }
                            StructureObject::StructureStorage(s) => {
                                if DepositCode::Done == self.deposit(s) {
                                    creep_targets.remove(&name);
                                }
                            }
                            _ => {
                                warn!("how the hell could this be different from a spawn, extension or storage")
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
                                    match r {
                                        ReturnCode::Ok => {}
                                        ReturnCode::Tired => {
                                            self.say("TIRED", false);
                                        }
                                        _ => {
                                            warn!("could not move to controller code: {:?}", r);
                                        }
                                    }
                                } else if r != ReturnCode::Ok {
                                    warn!("(upgrade controller) couldn't upgrade: {:?}", r);
                                    creep_targets.remove(&name);
                                }
                            }
                            None => warn!("could not resolve id for controller"),
                        }
                    } else {
                        creep_targets.remove(&name);
                    }
                }
                CreepTarget::Pickup(r) => {
                    if self.pos().is_near_to(r.pos()) {
                        let r = self.inner_creep.pickup(&r);
                        match r {
                            ReturnCode::Ok => {
                                creep_targets.insert(name, CreepTarget::Deposit());
                                return;
                            }
                            _ => {
                                warn!("could not pickup: {:?}", r);
                                creep_targets.remove(&name);
                                return;
                            }
                        }
                    } else {
                        let r = self.move_to(r);
                        match r {
                            ReturnCode::Ok => {}
                            ReturnCode::Tired => {
                                self.say("TIRED", false);
                                return;
                            }
                            _ => {
                                warn!("could not move to drop: {:?}", r);
                                return;
                            }
                        }
                    }
                }
            },
            None => {
                match self.role() {
                    Role::Harvester => {
                        let harvester = Harvester { creep: self };
                        harvester.run();
                        return;
                    }
                    Role::Hauler => todo!(),
                    _ => {}
                }

                // no target, let's find one depending on if we have energy
                if self.store().get_free_capacity(Some(ResourceType::Energy)) > 0 {
                    let drop = self.pos().find_closest_by_path(find::DROPPED_RESOURCES);
                    if let Some(d) = drop {
                        if ReturnCode::Ok == self.move_to(d.clone()) {
                            creep_targets.insert(name, CreepTarget::Pickup(d));
                        }
                        return;
                    }

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
                    if rnd_number < 8 {
                        creep_targets.insert(name, CreepTarget::Deposit());
                    } else {
                        // TODO:CLEAN
                        let max2 = 3;
                        let repair_or_build = rnd_source_idx(max2);
                        // we need this chance because otherwise we will repair walls infinetly
                        if repair_or_build < 1 {
                            // REPAIR
                            let object = room
                                .find(find::STRUCTURES)
                                .into_iter()
                                .filter(|o| o.as_attackable().is_some())
                                .filter(|o| o.structure_type() != StructureType::Controller)
                                .filter(|o| {
                                    o.as_attackable().unwrap().hits()
                                        < o.as_attackable().unwrap().hits_max() / 3
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
                            let max2 = 2;
                            let build_controller_or_other = rnd_source_idx(max2);
                            // we need this chance because otherwise we will repair everything but the
                            // controller since it's very improbably that a creep will be near a
                            // controller
                            // TODO: CLEAN
                            if build_controller_or_other < 1 {
                                // Upgrade RANDOM CONSTRUCTION SITE but Controller
                                let site =
                                    self.pos().find_closest_by_path(find::CONSTRUCTION_SITES);
                                match site {
                                    Some(val) => match val.structure_type() {
                                        StructureType::Controller => {
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
                            } else {
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

pub fn find_tower(room: Room) -> Option<StructureTower> {
    let structures = room.find(find::MY_STRUCTURES);
    let tower_obj = structures
        .into_iter()
        .filter(|s| s.structure_type() == StructureType::Tower)
        .filter(|t| {
            t.as_has_store()
                .unwrap()
                .store()
                .get_free_capacity(Some(ResourceType::Energy))
                > 150
        })
        .reduce(|res, next_t| {
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

pub fn obj_to_container(obj: &StructureObject) -> Option<StructureContainer> {
    match obj {
        StructureObject::StructureContainer(c) => Some(c.clone()),
        _ => {
            warn!("expected a container");
            None
        }
    }
}
