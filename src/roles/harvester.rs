use crate::creep::*;
use log::*;
use screeps::{
    find, look, prelude::*, Look, Position, ResourceType, ReturnCode, Room, RoomPosition, Source,
    StructureContainer, StructureObject, StructureType,
};

use super::role::{CanHarvest, Deposit, Movable};

pub struct Harvester<'a> {
    pub creep: &'a screeps::Creep,
}

impl<'a> CanHarvest for Harvester<'a> {
    fn harvest(&self, source: &Source) -> bool {
        let r = self.creep.harvest(source);
        match r {
            ReturnCode::Ok => true,
            ReturnCode::NotEnough => {
                info!("couldn't harvest: {:?}", r);
                false
            }
            _ => {
                warn!("couldn't harvest: {:?}", r);
                false
            }
        }
    }
}

impl<'a> Movable for Harvester<'a> {
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

impl<'a> Harvester<'a> {
    pub fn pick_closest_spot(&self) -> Option<(Source, Position)> {
        let room = self.creep.room().unwrap();
        let sources = room.find(find::SOURCES);
        let mut source_container = Vec::<(Source, Position)>::new();
        for s in sources.iter() {
            let deposit = self.find_closest_container_from_source(s.pos());
            if let Some(d) = deposit {
                let creeps = room.look_for_at(look::CREEPS, &d.pos());
                let objs = creeps
                    .iter()
                    .filter(|creep| creep.pos() != self.creep.pos())
                    .collect::<Vec<&screeps::Creep>>();
                if objs.len() == 0 {
                    source_container.push((s.clone(), d.pos()));
                }
            } else {
                build_container_around_source(room.clone(), s.pos());
                warn!("did not find container near this source {:?}", s.pos());
            }
        }
        if source_container.len() > 0 {
            let val = source_container.get(0).unwrap();
            Some((val.0.clone(), val.1))
        } else {
            None
        }
    }

    pub fn run(self) {
        if let Some((source, c_pos)) = self.pick_closest_spot() {
            if self.creep.pos().is_equal_to(c_pos) {
                //ignoring return code for harvest because it already logs
                //inside
                let _ = self.harvest(&source);
            } else {
                self.move_to(c_pos);
            }
        } else {
            info!("could not find an active source");
        }
    }
    fn find_closest_container_from_source(
        &self,
        source_pos: Position,
    ) -> Option<StructureContainer> {
        let room = self.creep.room().unwrap();
        let structures = room.find(find::MY_STRUCTURES);
        let container_obj = structures
            .iter()
            .filter(|o| o.structure_type() == StructureType::Container)
            .reduce(|closer, next| {
                if closer.pos().get_range_to(source_pos) > next.pos().get_range_to(source_pos) {
                    next
                } else {
                    closer
                }
            });
        if let Some(obj) = container_obj {
            let container = obj_to_container(obj).unwrap();
            Some(container)
        } else {
            None
        }
    }
}

fn build_container_around_source(room: Room, source_pos: Position) {
    let area = room.look_at_area(
        source_pos.y().u8() - 1,
        source_pos.x().u8() - 1,
        source_pos.y().u8() + 1,
        source_pos.x().u8() + 1,
        true,
    );
    // let objs = js_sys::Array::from(&area).map(|x: Vec<StructureObject>| x.map().collect());
}
