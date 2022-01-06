use std::collections::HashMap;

use crate::storage::*;
use log::*;
use screeps::{
    Attackable, Creep as ScreepsCreep, MaybeHasNativeId, Position, ReturnCode, Room, RoomPosition,
    Structure, StructureTower,
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

        let target = towers_target.get(&self.pos());
        match target {
            Some(tower_target) => match &tower_target {
                TowerTarget::Repair(structure_id) => {}
                TowerTarget::Attack(target) => {
                    let r = self.attack(&(**target));
                    if r != ReturnCode::Ok {
                        warn!("couldn't attack: {:?}", r);
                    }
                }
            },
            None => {
                if hostiles.len() > 0 {
                    for h in hostiles.iter() {
                        towers_target.insert(self.pos(), TowerTarget::Attack(Box::new(h.clone())));
                    }
                } else {
                    // ADD HERE FUNCTIONALITY TO REPAIR
                }
            }
        }
    }
}
