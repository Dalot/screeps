use crate::role::Role;
use screeps::{
    Attackable, ConstructionSite, ObjectId, Position, Resource, Source, Structure,
    StructureController, StructureTower,
};
use serde::{Deserialize, Serialize};
// this is one way to persist data between ticks within Rust's memory, as opposed to
use std::cell::RefCell;
use std::collections::HashMap;
// keeping state in memory on game objects - but will be lost on global resets!
thread_local! {
    pub static CREEPS_TARGET: RefCell<HashMap<String, CreepTarget>> = RefCell::new(HashMap::new());
    pub static TOWERS_TARGET: RefCell<HashMap<Position, TowerTarget>> = RefCell::new(HashMap::new());
    pub static CREEPS_ROLE: RefCell<HashMap<String, Role>> = RefCell::new(HashMap::new());
    static CREEPS_MEMORY: RefCell<HashMap<String, CreepMemory>> = RefCell::new(HashMap::new());
}

// this enum will represent a creep's lock on a specific target object, storing a js reference to the object id so that we can grab a fresh reference to the object each successive tick, since screeps game objects become 'stale' and shouldn't be used beyond the tick they were fetched
#[derive(Clone)]
pub enum CreepTarget {
    UpgradeController(ObjectId<StructureController>),
    UpgradeConstructionSite(ConstructionSite),
    Harvest(ObjectId<Source>),
    Deposit(),
    Pickup(Resource),
    // Harvester(Option<ObjectId<Source>>, Option<StructureObject>),
    Repair(ObjectId<Structure>),
}
// this enum will represent a creep's lock on a specific target object, storing a js reference to the object id so that we can grab a fresh reference to the object each successive tick, since screeps game objects become 'stale' and shouldn't be used beyond the tick they were fetched
pub enum TowerTarget {
    Attack(Box<dyn Attackable>),
    //Heal(?),
    Repair(ObjectId<Structure>),
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Root {
    pub creeps: HashMap<String, CreepMemory>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct CreepMemory {
    _move: Option<Move>,
    pub role: Option<Role>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Move {
    dest: DestJson,
    time: u64,
    path: String,
    room: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct DestJson {
    x: u64,
    y: u64,
    room: String,
}
