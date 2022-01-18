use super::role::{CanDeposit, Deposit, DepositCode, Movable, Role};
use crate::creep::find_tower;
use crate::storage::CreepTarget;
use log::*;
use screeps::{
    find, game, prelude::*, Creep, ObjectId, Resource, ResourceType, ReturnCode,
    RoomObjectProperties, SharedCreepProperties, StructureExtension, StructureObject,
    StructureType,
};
use std::collections::HashMap;

//TODO: deposit with the following precedence if we are being attacked and a minimum number of
//creeps -> towers,spawn, extensions, storage. This is probably better to create a new struct,
//DangeredHauler or something

// TODO: pick first from containers, as long as it has at least the amount that the hauler can carry,
// otherwise pick from the drop. We should also factor the amount that that container has already
// filled in.
//
// TODO: I think we need to implement creep_targets a bit more on the haulers now otherwise, they
// are doing all the same thing at the same time when it's needed only one to do it
//
pub struct Hauler<'a> {
    pub creep: &'a screeps::Creep,
}
impl<'a> Movable for Hauler<'a> {
    fn move_to<T>(&self, target: T)
    where
        T: HasPosition,
    {
        let r = self.creep.move_to(target);
        match r {
            ReturnCode::Ok => {}
            ReturnCode::Tired => {
                self.creep.say("TIRED", false);
            }
            _ => {
                warn!("couldn't move: {:?}", r);
            }
        }
    }
}
impl<'a> Hauler<'a> {
    pub fn pickup(&self, target: &Resource) -> ReturnCode {
        self.creep.pickup(target)
    }

    pub fn say(&self, msg: &str, public: bool) -> ReturnCode {
        self.creep.say(msg, public)
    }

    pub fn run(&self, has_hostiles: bool, creep_targets: &mut HashMap<String, CreepTarget>) {
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
            .get_used_capacity(Some(ResourceType::Energy))
            > 0
        {
            // Creep has store with energy

            let deposit = self.find_closest_depositable(false);
            if let Some(val) = deposit {
                if val.is_storage() {
                    if let Some(c) = self.find_creep() {
                        creep_targets.insert(self.creep.name(), CreepTarget::TransferToCreep(c));
                        return;
                    }
                }
                if self.creep.pos().is_near_to(val.pos()) {
                    self.deposit(val);
                    return;
                } else {
                    self.move_to(val.pos());
                }
            } else {
                info!("could not find deposit");
            }
        } else {
            // Creep has empty store
            //
            // Let's empty those containers
            let deposit = self.find_closest_container();
            if let Some(val) = deposit {
                if self.creep.pos().is_near_to(val.pos()) {
                    let target = *val.withdrawable();
                    let r = self
                        .creep
                        .withdraw(target, ResourceType::Energy, Some(val.amount()));
                    if r != ReturnCode::Ok {
                        warn!("couldn't withdraw: {:?}", r);
                    }
                    return;
                } else {
                    self.move_to(val.pos());
                    return;
                }
            }

            // Containers are kind of empty, let's PICKUP energy from the floor
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
                    self.move_to(r);
                    return;
                }
            }

            // No drops either. Let's see if we have energy on the storage. If we have we can fill towers if they are empty.

            // store

            // TODO: transfer from storages to extensions/spawn?
            // TODO: transfer to towers ?
            // Let's pick energy from a storage then
            let room = self.creep.room().unwrap();
            let storage = room.storage();
            if let Some(s) = storage {
                if s.store().get_used_capacity(Some(ResourceType::Energy))
                    >= self
                        .creep
                        .store()
                        .get_free_capacity(Some(ResourceType::Energy)) as u32
                        / 2
                {
                    // Ok we have a storage with energy, let's pick it up.
                    let value_to_withdraw = self.get_value_to_withdraw(&s.store());
                    if self.creep.pos().is_near_to(s.pos()) {
                        let r =
                            self.creep
                                .withdraw(&s, ResourceType::Energy, Some(value_to_withdraw));
                        if r != ReturnCode::Ok {
                            warn!("couldn't withdraw: {:?}", r);
                        }
                        return;
                    } else {
                        self.move_to(s.pos());
                        return;
                    }
                }
            }
        }
    }

    pub fn run_targets(&self, creep_targets: &mut HashMap<String, CreepTarget>) {
        let name = self.creep.name();
        let target = creep_targets.get(&name);
        let keep_target = match target {
            Some(creep_target) => match &creep_target {
                CreepTarget::TransferToCreep(creep) => {
                    if self.creep.pos().is_near_to(creep.pos()) {
                        let value_to_transfer = self.get_value_to_transfer(&creep.store());
                        let r = self.creep.transfer(
                            creep,
                            ResourceType::Energy,
                            Some(value_to_transfer),
                        );
                        match r {
                            ReturnCode::Ok => false,
                            ReturnCode::Full => false,
                            _ => {
                                warn!("could not deposit energy, {:?}", r);
                                false
                            }
                        }
                    } else {
                        self.move_to(creep.pos());
                        true
                    }
                }
            },
            None => false,
        };
        if !keep_target {
            creep_targets.remove(&name);
        }
    }

    /// Will find the nearest unfilled extension
    /// Returns an option because it may not find an extension
    pub fn find_unfilled_extension(&self) -> Option<StructureExtension> {
        let creep_pos = self.creep.pos();
        let structures = self.creep.room().unwrap().find(find::MY_STRUCTURES);
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
            None
        }
    }
    pub fn get_value_to_transfer(&self, target_store: &screeps::Store) -> u32 {
        let mut value_to_transfer = self
            .creep
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
    pub fn get_value_to_withdraw(&self, target_store: &screeps::Store) -> u32 {
        let mut value_to_transfer: u32 = self
            .creep
            .store()
            .get_free_capacity(Some(ResourceType::Energy))
            .try_into()
            .expect("could not convert i32 to u32");

        let target_used_store: u32 = target_store.get_used_capacity(Some(ResourceType::Energy));

        if target_used_store < value_to_transfer {
            value_to_transfer = target_used_store;
        }
        value_to_transfer
    }
    pub fn find_creep(&self) -> Option<Creep> {
        let room = self.creep.room().unwrap();
        let creeps = room.find(find::MY_CREEPS);
        creeps
            .iter()
            .filter(|c| {
                if let Some(r) = Role::find_role(c) {
                    match r {
                        Role::Builder => {
                            return true;
                        }
                        _ => {
                            return false;
                        }
                    }
                }
                false
            })
            .filter(|c| {
                c.store().get_used_capacity(Some(ResourceType::Energy))
                    != c.store().get_capacity(Some(ResourceType::Energy))
            })
            .reduce(|closer, next| {
                if closer.pos().get_range_to(self.creep.pos())
                    < next.pos().get_range_to(self.creep.pos())
                {
                    closer
                } else {
                    next
                }
            })
            .cloned()
    }
}

impl<'a> CanDeposit for Hauler<'a> {
    /// It will find and return the first depositable on the following precedence:
    /// Spawn > extension > tower > storage
    fn find_closest_depositable(&self, danger: bool) -> Option<Deposit> {
        let room = self.creep.room().unwrap();
        let spawns = room.find(find::MY_SPAWNS);

        let spawn = spawns
            .iter()
            .filter(|s| s.store().get_free_capacity(Some(ResourceType::Energy)) > 0)
            .last();
        if let Some(s) = spawn {
            let target_store = s.store();
            let value_to_transfer = self.get_value_to_transfer(&target_store);
            Some(Deposit::new(
                StructureObject::StructureSpawn(s.clone()),
                value_to_transfer,
            ))
        } else {
            if let Some(ext) = self.find_unfilled_extension() {
                let target_store = ext.store();
                let value_to_transfer = self.get_value_to_transfer(&target_store);
                Some(Deposit::new(
                    StructureObject::StructureExtension(ext),
                    value_to_transfer,
                ))
            } else {
                if let Some(t) = find_tower(room.clone()) {
                    let target_store = t.store();
                    let value_to_transfer = self.get_value_to_transfer(&target_store);
                    Some(Deposit::new(
                        StructureObject::StructureTower(t),
                        value_to_transfer,
                    ))
                } else {
                    let storage = room.storage();
                    if let Some(s) = storage {
                        let target_store = s.store();
                        let value_to_transfer = self.get_value_to_transfer(&target_store);
                        Some(Deposit::new(
                            StructureObject::StructureStorage(s),
                            value_to_transfer,
                        ))
                    } else {
                        None
                    }
                }
            }
        }
        // let structures = room.find(find::MY_STRUCTURES);
        // if including_containers {
        //     let container_obj = structures
        //         .iter()
        //         .filter(|s| s.structure_type() == StructureType::Container)
        //         .filter(|s| {
        //             s.as_has_store()
        //                 .unwrap()
        //                 .store()
        //                 .get_free_capacity(Some(ResourceType::Energy))
        //                 > 0
        //         })
        //         .reduce(|closer, next| {
        //             if closer.pos().get_range_to(creep_pos) > next.pos().get_range_to(creep_pos) {
        //                 next
        //             } else {
        //                 closer
        //             }
        //         });
        //     if let Some(obj) = container_obj {
        //         let container = obj_to_container(obj).unwrap();
        //         objects.push(StructureObject::StructureContainer(container));
        //     }
        // }

        // let obj = objects.into_iter().reduce(|closer, next| {
        //     if closer.pos().get_range_to(creep_pos) > next.pos().get_range_to(creep_pos) {
        //         next
        //     } else {
        //         closer
        //     }
        // });
        // if let Some(o) = obj {
        //     let transferable = o
        //         .as_transferable()
        //         .expect("expected the obj to be a transferable");
        //     let target_store = o
        //         .as_has_store()
        //         .expect("expected the obj to be a HasStore")
        //         .store();
        //     let value_to_transfer = self.get_value_to_transfer(&target_store);
        //     Some(Deposit::new(o, value_to_transfer))
        // } else {
        //     None
        // }
    }

    /// Finds the closest container that has sufficient stored energy to at least fill the creep's
    /// store
    fn find_closest_container(&self) -> Option<Deposit> {
        let room = self.creep.room().unwrap();
        let creep_pos = self.creep.pos();
        let structures = room.find(find::STRUCTURES);
        let container_obj = structures
            .iter()
            .filter(|s| s.structure_type() == StructureType::Container)
            .filter(|s| {
                s.as_has_store()
                    .unwrap()
                    .store()
                    .get_used_capacity(Some(ResourceType::Energy))
                    >= self.creep.store().get_capacity(Some(ResourceType::Energy))
            })
            .reduce(|closer, next| {
                if closer.pos().get_range_to(creep_pos) > next.pos().get_range_to(creep_pos) {
                    next
                } else {
                    closer
                }
            });
        if let Some(obj) = container_obj {
            let store = obj.as_has_store().unwrap().store();
            let creep_free_cap: u32 = self
                .creep
                .store()
                .get_free_capacity(Some(ResourceType::Energy))
                .try_into()
                .expect("could not convert i32 to u32");

            let target_used_store: u32 = store.get_used_capacity(Some(ResourceType::Energy));
            if target_used_store < creep_free_cap {
                None
            } else {
                let value_to_transfer = std::cmp::min(creep_free_cap, target_used_store);
                Some(Deposit::new(obj.clone(), value_to_transfer))
            }
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
                let target = *deposit.transferable();
                let r = self
                    .creep
                    .transfer(target, ResourceType::Energy, Some(deposit.amount()));
                info!("deposit code: {:?}", r);
                match r {
                    ReturnCode::Ok => DepositCode::NotDone,
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
}
