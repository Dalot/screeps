use log::*;
use screeps::{
    find, look, prelude::*, Look, Position, ResourceType, ReturnCode, RoomPosition, Source,
    StructureContainer, StructureType,
};

use super::role::{CanHarvest, Deposit, Movable};

pub struct Builder<'a> {
    pub creep: &'a screeps::Creep,
}

impl<'a> Movable for Builder<'a> {
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

// TODO: needs targets
impl<'a> Builder<'a> {
    pub fn run(&self) {
        if self
            .creep
            .store()
            .get_used_capacity(Some(ResourceType::Energy))
            > 0
        {
            // Upgrade RANDOM CONSTRUCTION SITE but Controller
            let site = self
                .creep
                .pos()
                .find_closest_by_path(find::CONSTRUCTION_SITES);
            match site {
                Some(val) => {
                    if self.creep.pos().is_near_to(val.pos()) {
                        self.creep.build(&val);
                        return;
                    } else {
                        self.move_to(val.pos())
                    }
                }
                None => {}
            }
            let room = self.creep.room().unwrap();
            let object = room
                .find(find::STRUCTURES)
                .into_iter()
                .filter(|o| o.as_attackable().is_some())
                .filter(|o| o.structure_type() != StructureType::Controller)
                .filter(|o| {
                    o.as_attackable().unwrap().hits() < o.as_attackable().unwrap().hits_max() / 3
                })
                .reduce(|fewer_hp_obj, next_obj| {
                    // here we are sure we only have only attackables
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
                    let target = obj.as_structure();
                    let r = self.creep.repair(target);
                    if r == ReturnCode::NotInRange {
                        self.move_to(target)
                    } else if r != ReturnCode::Ok {
                        warn!("couldn't repair: {:?}", r);
                    }
                }
                None => {
                    info!("could not find anything to repair");
                }
            }
        } else {
            self.creep.say("E_OUT", false);
        }
    }
}
