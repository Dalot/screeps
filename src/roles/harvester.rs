use crate::creep::*;
use log::*;
use screeps::{
    find, prelude::*, rooms, ConstructionSite, MoveToOptions, ObjectId, Part, PolyStyle,
    ResourceType, ReturnCode, Room, RoomObject, RoomObjectProperties, RoomPosition, Source,
    StructureContainer, StructureController, StructureExtension, StructureObject, StructureTower,
    StructureType,
};

use super::role::CanHarvest;

pub struct Harvester<'a> {
    pub creep: &'a Creep<'a>,
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

impl<'a> Harvester<'a> {
    pub fn pick_closest_energy_source(&self) -> Option<screeps::Source> {
        let source = self.creep.pos().find_closest_by_path(find::SOURCES_ACTIVE);

        if let Some(val) = source {
            return Some(val);
        }
        None
    }

    pub fn run(self) {
        if let Some(source) = self.pick_closest_energy_source() {
            if self.creep.pos().is_near_to(source.pos()) {
                //ignoring return code for harvest because it already logs
                //inside
                let _ = self.harvest(&source);
            } else {
                self.creep.move_to(&source);
            }
        } else {
            info!("could not find an active source");
        }
    }
}
