use super::role::{CanDeposit, Deposit, DepositCode, Movable};
use crate::creep::*;
use log::*;
use screeps::{
    find, game, prelude::*, rooms, ConstructionSite, MoveToOptions, ObjectId, Part, PolyStyle,
    Position, RawObjectId, Resource, ResourceType, ReturnCode, Room, RoomObject,
    RoomObjectProperties, RoomPosition, SharedCreepProperties, Source, Structure,
    StructureContainer, StructureController, StructureExtension, StructureObject, StructureTower,
    StructureType,
};
use std::collections::HashMap;

pub struct Hauler<'a> {
    pub creep: &'a Creep<'a>,
}
impl<'a> Movable for Hauler<'a> {
    fn move_to<T>(&self, target: T) -> ReturnCode
    where
        T: HasPosition,
    {
        self.creep.move_to(target)
    }
}
impl<'a> Hauler<'a> {
    pub fn pickup(&self, target: &Resource) -> ReturnCode {
        self.creep.pickup(target)
    }

    pub fn say(&self, msg: &str, public: bool) -> ReturnCode {
        self.creep.say(msg, public)
    }

    pub fn run(self) {
        //HAULER NEEEDS TO:
        // PICK UP ENERGY ON THE FLOOR AND TAKE IT TO THE CLOSEST DEPOSIT
        // PICK THE ENERGY FROM THE CONTAINERS AND TAKE IT TO A SPAWN->EXTENSION->TOWER->STORAGE
        // let creeps: Vec<screeps::Creep> = game::creeps().values().collect();
        // if creeps.len() > 10 || (has_hostiles && creeps.len() > 5) {
        //     if let Some(t) = find_tower(room) {
        //         if DepositCode::Done == self.deposit(t) {
        //             creep_targets.remove(&name);
        //         }
        //         return;
        //     }
        // }
        if self
            .creep
            .store()
            .get_free_capacity(Some(ResourceType::Energy))
            > 0
        {
            // PICKUP
            let drop = self
                .creep
                .pos()
                .find_closest_by_path(find::DROPPED_RESOURCES);
            if let Some(r) = drop {
                if self.creep.pos().is_near_to(r.pos()) {
                    let r = self.creep.pickup(&r);
                    match r {
                        ReturnCode::Ok => {
                            return;
                        }
                        _ => {
                            warn!("could not pickup: {:?}", r);
                            return;
                        }
                    }
                } else {
                    let r = self.move_to(r);
                    match r {
                        ReturnCode::Ok => {
                            return;
                        }
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
            } else {
                let deposit = self.find_closest_depositable(false);
                if let Some(val) = deposit {
                    if self.creep.pos().is_near_to(val.pos()) {
                        self.deposit(val);
                        return;
                    } else {
                        let r = self.move_to(val.pos());
                        match r {
                            ReturnCode::Ok => {
                                return;
                            }
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
                } else {
                    info!("(CreepTarget::Deposit) could not find deposit");
                }
            }
        } else {
            // let deposit = self.find_closest_bank::<Option<Deposit<dyn Transferable>>>();
            let deposit = self.find_closest_depositable(true);
            if let Some(val) = deposit {
                if self.creep.pos().is_near_to(val.pos()) {
                    self.deposit(val);
                    return;
                } else {
                    let r = self.move_to(val.pos());
                    match r {
                        ReturnCode::Ok => {
                            return;
                        }
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
            } else {
                info!("(CreepTarget::Deposit) could not find deposit");
            }
        }
    }
}

impl<'a> CanDeposit for Hauler<'a> {
    fn find_closest_depositable(&self, including_containers: bool) -> Option<Deposit> {
        let room = self.creep.room().unwrap();
        let spawns = room.find(find::MY_SPAWNS);
        let objects = Vec::<StructureObject>::new();
        let creep_pos = self.creep.pos();

        let spawn = spawns
            .iter()
            .filter(|s| s.store().get_free_capacity(Some(ResourceType::Energy)) > 0)
            .last();
        let structures = room.find(find::MY_STRUCTURES);
        let storage = room.storage();
        if including_containers {
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
            if let Some(obj) = container_obj {
                let container = obj_to_container(obj).unwrap();
                objects.push(StructureObject::StructureContainer(container));
            }
        }
        let object = room
            .find(find::STRUCTURES)
            .into_iter()
            .filter(|o| o.as_attackable().is_some())
            .filter(|o| o.structure_type() != StructureType::Controller)
            .filter(|o| {
                o.as_attackable().unwrap().hits() < o.as_attackable().unwrap().hits_max() / 3
            })
            .reduce(|fewer_hp_obj, next_obj| {
                // here we are sure we only have attackables so we are free to use
                // unwrap. Here we pick the most closer object
                if let Some(next_attackable) = next_obj.as_attackable() {
                    if let Some(fewer_hp_atttackble) = fewer_hp_obj.as_attackable() {
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
        if let Some(t) = find_tower(room) {
            objects.push(StructureObject::StructureTower(t));
        }
        if let Some(s) = spawn {
            objects.push(StructureObject::StructureSpawn(s.clone()));
        }
        if let Some(ext) = self.creep.find_unfilled_extension() {
            objects.push(StructureObject::StructureExtension(ext));
        }
        if let Some(s) = storage {
            objects.push(StructureObject::StructureStorage(s));
        }

        let obj = objects.iter().reduce(|closer, next| {
            if closer.pos().get_range_to(creep_pos) > next.pos().get_range_to(creep_pos) {
                next
            } else {
                closer
            }
        });
        if let Some(o) = obj {
            let transferable = o
                .clone()
                .as_transferable()
                .expect("expected the obj to be a transferable");
            let target_store = o
                .as_has_store()
                .expect("expected the obj to be a HasStore")
                .store();
            let value_to_transfer = self.creep.get_value_to_transfer(&target_store);
            Some(Deposit {
                target: Box::new(transferable),
                position: o.pos(),
                amount: value_to_transfer,
            })
        } else {
            None
        }
    }

    /// Can return false when it's not done with deposit everything
    /// or because it failed for some reason which should be logged
    fn deposit(&self, deposit: Deposit) -> DepositCode {
        if self
            .creep
            .store()
            .get_used_capacity(Some(ResourceType::Energy))
            > 0
        {
            if self.creep.pos().is_near_to(deposit.pos()) {
                let target = **deposit.target();
                let r = self
                    .creep
                    .transfer(target, ResourceType::Energy, Some(deposit.amount()));
                info!("deposit code: {:?}", r);
                match r {
                    ReturnCode::Ok => {
                        info!("deposited {}", deposit.amount());
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
                let r = self.creep.move_to(deposit.pos());
                match r {
                    ReturnCode::Ok => DepositCode::NotNear,

                    ReturnCode::Tired => {
                        self.creep.say("TIRED", false);
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

    fn find_closest_bank(&self) -> Option<Deposit> {
        let room = self.creep.room().unwrap();
        let spawns = room.find(find::MY_SPAWNS);
        let objects = Vec::<StructureObject>::new();
        let creep_pos = self.creep.pos();

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
        let object = room
            .find(find::STRUCTURES)
            .into_iter()
            .filter(|o| o.as_attackable().is_some())
            .filter(|o| o.structure_type() != StructureType::Controller)
            .filter(|o| {
                o.as_attackable().unwrap().hits() < o.as_attackable().unwrap().hits_max() / 3
            })
            .reduce(|fewer_hp_obj, next_obj| {
                // here we are sure we only have attackables so we are free to use
                // unwrap. Here we pick the most closer object
                if let Some(next_attackable) = next_obj.as_attackable() {
                    if let Some(fewer_hp_atttackble) = fewer_hp_obj.as_attackable() {
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
        if let Some(t) = find_tower(room) {
            objects.push(StructureObject::StructureTower(t));
        }
        if let Some(s) = spawn {
            objects.push(StructureObject::StructureSpawn(s.clone()));
        }
        if let Some(ext) = self.creep.find_unfilled_extension() {
            objects.push(StructureObject::StructureExtension(ext));
        }
        if let Some(s) = storage {
            objects.push(StructureObject::StructureStorage(s));
        }

        let mut container: StructureContainer;
        if let Some(obj) = container_obj {
            container = obj_to_container(obj).unwrap();
            objects.push(StructureObject::StructureContainer(container));
        }

        let obj = objects.iter().reduce(|closer, next| {
            if closer.pos().get_range_to(creep_pos) > next.pos().get_range_to(creep_pos) {
                next
            } else {
                closer
            }
        });
        if let Some(o) = obj {
            let transferable = o
                .clone()
                .as_transferable()
                .expect("expected the obj to be a transferable");
            let target_store = o
                .as_has_store()
                .expect("expected the obj to be a HasStore")
                .store();
            let value_to_transfer = self.creep.get_value_to_transfer(&target_store);
            Some(Deposit {
                target: Box::new(transferable),
                position: o.pos(),
                amount: value_to_transfer,
            })
        } else {
            None
        }
    }
}
