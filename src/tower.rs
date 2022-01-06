use std::collections::HashMap;

use crate::storage::*;
use log::*;
use screeps::{
    find, Attackable, Creep as ScreepsCreep, HasTypedId, MaybeHasNativeId, Position, ResourceType,
    ReturnCode, Room, RoomPosition, Store, Structure, StructureProperties, StructureTower,
    StructureType,
};
pub struct Tower<'a> {
    inner_tower: &'a StructureTower,
}
impl<'a> Tower<'a> {
    pub fn new(tower: &'a StructureTower) -> Self {
        Self { inner_tower: tower }
    }
    pub fn repair(&self, target: &Structure) -> ReturnCode {
        self.inner_tower.repair(target)
    }
    pub fn room(&self) -> Option<Room> {
        self.inner_tower.room()
    }
    pub fn store(&self) -> Store {
        self.inner_tower.store()
    }
    pub fn pos(&self) -> Position {
        self.inner_tower.pos().into()
    }
    pub fn attack<T>(&self, target: &T) -> ReturnCode
    where
        T: ?Sized + Attackable,
    {
        self.inner_tower.attack(target)
    }
    pub fn run(
        &self,
        towers_target: &mut HashMap<Position, TowerTarget>,
        hostiles: Vec<ScreepsCreep>,
    ) {
        let room = self.room().unwrap();
        let tower_pos = self.pos();

        let target = towers_target.get(&self.pos());
        match target {
            Some(tower_target) => match &tower_target {
                TowerTarget::Repair(structure_id) => match structure_id.resolve() {
                    Some(obj) => {
                        if obj.hits() == obj.hits_max() {
                            towers_target.remove(&tower_pos);
                        }
                        let r = self.repair(&obj);
                        if r != ReturnCode::Ok {
                            warn!("couldn't repair: {:?}", r);
                            towers_target.remove(&tower_pos);
                        }
                    }
                    None => {
                        warn!("could not resolve object in repair");
                        towers_target.remove(&tower_pos);
                    }
                },
                TowerTarget::Attack(target) => {
                    let r = self.attack(&(**target));
                    if r != ReturnCode::Ok {
                        warn!("couldn't attack: {:?}", r);
                        towers_target.remove(&self.pos());
                    }
                }
                TowerTarget::Heal(_) => {}
            },
            None => {
                if hostiles.len() > 0 {
                    for h in hostiles.iter() {
                        towers_target.insert(self.pos(), TowerTarget::Attack(Box::new(h.clone())));
                    }
                } else {
                    if self.store().get_free_capacity(Some(ResourceType::Energy))
                        > self.store().get_capacity(Some(ResourceType::Energy)) as i32 / 2
                    {
                        //used too much energy already, need to save in case of an attack
                        return;
                    }
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
                    match object {
                        Some(obj) => {
                            towers_target
                                .insert(tower_pos, TowerTarget::Repair(obj.as_structure().id()));
                            return;
                        }
                        None => {
                            info!("could not find anything to repair");
                        }
                    }
                }
            }
        }
    }
}
