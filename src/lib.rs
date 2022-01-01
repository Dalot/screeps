use std::cell::RefCell;
use std::collections::HashMap;

use creep::*;
use log::*;
use screeps::{
    find, game, prelude::*, ConstructionSite, ObjectId, OwnedStructureObject, Part, RawMemory,
    ResourceType, ReturnCode, Room, RoomObject, RoomObjectProperties, Source, Structure,
    StructureController, StructureExtension, StructureObject, StructureRoad, StructureSpawn,
    StructureType,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

mod creep;
mod logging;

// add wasm_bindgen to any function you would like to expose for call from js
#[wasm_bindgen]
pub fn setup() {
    logging::setup_logging(logging::Info);
}

// this is one way to persist data between ticks within Rust's memory, as opposed to
// keeping state in memory on game objects - but will be lost on global resets!
thread_local! {
    static CREEPS_TARGET: RefCell<HashMap<String, CreepTarget>> = RefCell::new(HashMap::new());
    static CREEPS_MEMORY: RefCell<HashMap<String, CreepMemory>> = RefCell::new(HashMap::new());
}

// this enum will represent a creep's lock on a specific target object, storing a js reference to the object id so that we can grab a fresh reference to the object each successive tick, since screeps game objects become 'stale' and shouldn't be used beyond the tick they were fetched
#[derive(Clone)]
enum CreepTarget {
    UpgradeController(ObjectId<StructureController>),
    UpgradeConstructionSite(ConstructionSite),
    RepairRoad(ObjectId<StructureRoad>),
    Harvest(ObjectId<Source>),
    DepositExtension(StructureExtension),
    DepositSpawn(ObjectId<StructureSpawn>),
}

#[derive(Default)]
struct Totals {
    upgrade_controller_total: usize,
    harvest_total: usize,
    deposit_total: usize,
    upgrade_sites_total: usize,
}

// to use a reserved name as a function name, use `js_name`:
#[wasm_bindgen(js_name = loop)]
pub fn game_loop() {
    debug!("loop starting! CPU: {}", game::cpu::get_used());
    let mut db = Database::init().expect("could not init database");
    let mut harvest_sources = HashMap::<ObjectId<Source>, (usize, String)>::new();
    // mutably borrow the creep_targets refcell, which is holding our creep target locks
    // in the wasm heap
    CREEPS_TARGET.with(|creep_targets_refcell| {
        let mut creep_targets = creep_targets_refcell.borrow_mut();
        debug!("running creeps");
        for creep in game::creeps().values() {
            run_creep(&creep, &mut creep_targets);
        }
        for (creep_name, creep_target) in creep_targets.iter() {
            if let CreepTarget::Harvest(source_id) = creep_target {
                let total = harvest_sources
                    .entry(source_id.clone())
                    .or_insert((1, creep_name.clone()));
                (*total).0 += 1;
            }
        }
    });

    CREEPS_TARGET.with(|creep_targets_refcell| {
        let mut creep_targets = creep_targets_refcell.borrow_mut();
        for (object_id, tuple) in harvest_sources.iter() {
            if tuple.0 > 7 {
                info!(
                    "source ({}) is too crowded, will try to clean the area",
                    *object_id
                );
                let creep_name = String::from(tuple.1.clone());
                let creep = game::creeps()
                    .get(creep_name.clone())
                    .expect("could not find creep");
                let room = creep.room().expect("couldn't resolve creep room");
                let sources = room.find(find::SOURCES_ACTIVE);
                for source in sources.iter() {
                    if source.id() != *object_id {
                        creep_targets.remove(&tuple.1);
                        creep_targets.insert(creep_name.clone(), CreepTarget::Harvest(*object_id));
                    }
                }
            }
        }
    });
    debug!("running spawns");
    // Game::spawns returns a `js_sys::Object`, which is a light reference to an
    // object of any kind which is held on the javascript heap.
    //
    // Object::values returns a `js_sys::Array`, which contains the member spawn objects
    // representing all the spawns you control.
    //
    // They are returned as wasm_bindgen::JsValue references, which we can safely
    // assume are StructureSpawn objects as returned from js without checking first
    let mut additional = 0;
    for spawn in game::spawns().values() {
        debug!("running spawn {}", String::from(spawn.name()));

        let body = [
            Part::Move,
            Part::Move,
            Part::Carry,
            Part::Carry,
            Part::Work,
            Part::Work,
        ];
        if spawn.room().unwrap().energy_available() >= body.iter().map(|p| p.cost()).sum() {
            // create a unique name, spawn.
            let name_base = game::time();
            let name = format!("{}-{}", name_base, additional);
            // note that this bot has a fatal flaw; spawning a creep
            // creates Memory.creeps[creep_name] which will build up forever;
            // these memory entries should be prevented (todo doc link on how) or cleaned up
            let res = spawn.spawn_creep(&body, &name);
            // todo once fixed in branch this should be ReturnCode::Ok instead of this i8 grumble grumble
            if res != ReturnCode::Ok {
                warn!("couldn't spawn: {:?}", res);
            } else {
                additional += 1;
            }
        }
    }

    let time = screeps::game::time();

    if time % 32 == 3 {
        info!("running memory cleanup");
        // clean_up();
        db.clean_up();
    }
    info!("done! cpu: {}", game::cpu::get_used())
}

#[derive(Debug, Serialize, Deserialize)]
struct Root {
    creeps: HashMap<String, CreepMemory>,
    harvesters: Option<usize>,
    upgraders: Option<usize>,
    manuals: Option<usize>,
}
#[derive(Debug, Serialize, Deserialize)]
struct CreepMemory {
    _move: Option<Move>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Move {
    dest: DestJson,
    time: u64,
    path: String,
    room: String,
}
#[derive(Debug, Serialize, Deserialize)]
struct DestJson {
    x: u64,
    y: u64,
    room: String,
}

struct Database {
    data: Root,
}

impl Database {
    fn init() -> Option<Self> {
        let root_json_string: String = RawMemory::get().into();
        match serde_json::from_str(root_json_string.as_str()) {
            Ok::<Root, _>(root_json) => {
                info!("database init");
                Some(Self { data: root_json })
            }
            Err(e) => {
                info!("could not deserialize root_json: {}", e);
                None
            }
        }
    }

    fn clean_up(&mut self) {
        let mut to_remove = Vec::<String>::new();
        for (name, _) in self.data.creeps.iter() {
            let mut remove = true;
            for living_creep in game::creeps().values().into_iter() {
                if name == &living_creep.name() {
                    remove = false;
                    break;
                }
            }
            if remove {
                to_remove.push(name.clone());
            }
        }
        if to_remove.len() > 0 {
            info!("gonna remove {:?}", to_remove);
        }

        for name in to_remove.iter() {
            let removed_creep_js = self.data.creeps.remove(name);
            if let None = removed_creep_js {
                info!(
                    "tried to remove inexistent creep in memory object, name: {}",
                    name
                );
            }
        }

        self.update_memory();
    }

    fn update_memory(&self) {
        match serde_json::to_string(&self.data) {
            Ok::<String, _>(root_json) => {
                RawMemory::set(&js_sys::JsString::from(root_json));
            }
            Err(e) => {
                info!("could not serialize root_json: {}", e);
                info!("mutation did not persist to screeps memory");
            }
        }
    }

    fn get_creep_memory(&self, name: &str) -> Option<&CreepMemory> {
        self.data.creeps.get(name)
    }

    fn get_mut_creep_memory(&mut self, name: &str) -> Option<&mut CreepMemory> {
        self.data.creeps.get_mut(name)
    }
}

fn run_creep(creep: &screeps::Creep, creep_targets: &mut HashMap<String, CreepTarget>) {
    let creep = Creep::new(creep);
    let name = creep.name();
    if creep.spawning() {
        return;
    }

    let target = creep_targets.get(&name);
    match target {
        Some(creep_target) => {
            match &creep_target {
                CreepTarget::UpgradeController(controller_id) => {
                    if creep.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                        match controller_id.resolve() {
                            Some(controller) => {
                                let r = creep.upgrade_controller(&controller);
                                if r == ReturnCode::NotInRange {
                                    creep.move_to(&controller);
                                } else if r != ReturnCode::Ok {
                                    warn!("(upgrade controller)couldn't upgrade: {:?}", r);
                                    creep_targets.remove(&name);
                                }
                            }
                            None => warn!("couldn't file controller with id {}", controller_id),
                        }
                    } else {
                        creep_targets.remove(&name);
                        // HARVEST random energy source
                        if let Some(source_id) = creep.pick_random_energy_source() {
                            creep_targets.insert(name, CreepTarget::Harvest(source_id));
                        } else {
                            warn!("could not find source with id");
                        }
                    }
                }
                CreepTarget::Harvest(source_id) => {
                    if creep.store().get_free_capacity(Some(ResourceType::Energy)) > 0 {
                        match source_id.resolve() {
                            Some(source) => {
                                if creep.pos().is_near_to(source.pos()) {
                                    let r = creep.harvest(&source);
                                    if r != ReturnCode::Ok {
                                        warn!("couldn't harvest: {:?}", r);
                                    }
                                } else {
                                    creep.move_to(&source);
                                }
                            }
                            None => warn!("couldn't file controller with id {}", source_id),
                        }
                    } else {
                        creep_targets.remove(&name);
                    }
                }
                CreepTarget::DepositSpawn(spawn_id) => {
                    if creep.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                        match spawn_id.resolve() {
                            Some(spawn) => {
                                if creep.pos().is_near_to(spawn.pos()) {
                                    let value_to_transfer =
                                        creep.get_value_to_transfer(&spawn.store());
                                    let r = creep.transfer(
                                        &spawn,
                                        ResourceType::Energy,
                                        Some(value_to_transfer),
                                    );
                                    if r == ReturnCode::Full {
                                        info!("spawn {} is FULL, will do something else", spawn_id);
                                        creep_targets.remove(&name);
                                    } else if r != ReturnCode::Ok {
                                        warn!(
                                            "could not make a transfer from creep {}, to spawn {} code: {:?}",
                                            creep.name(),
                                            spawn_id,
                                            r
                                        );
                                        creep_targets.remove(&name);
                                    }
                                } else {
                                    let r = creep.move_to(&spawn);
                                    if r != ReturnCode::Ok {
                                        warn!(
                                            "could not move to spawn with id {} code: {:?}",
                                            spawn_id, r
                                        );
                                    }
                                }
                            }
                            None => {
                                warn!("could not resolve spawn with id {}", spawn_id);
                            }
                        }
                    } else {
                        creep_targets.remove(&name);
                    }
                }
                CreepTarget::DepositExtension(ext) => {
                    if creep.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                        if creep.pos().is_near_to(ext.pos()) {
                            let value_to_transfer = creep.get_value_to_transfer(&ext.store());
                            let r =
                                creep.transfer(ext, ResourceType::Energy, Some(value_to_transfer));
                            if r == ReturnCode::Ok {
                                creep_targets.remove(&name);
                                if creep.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                                    if let Some(ext) = creep.find_unfilled_extension() {
                                        creep_targets.insert(
                                            creep.name(),
                                            CreepTarget::DepositExtension(ext),
                                        );
                                    }
                                }
                            } else {
                                warn!(
                                            "could not make a transfer from creep {}, to extension {} code: {:?}",
                                            creep.name(),
                                            ext.id(),
                                            r
                                        );
                                creep_targets.remove(&name);
                            }
                        } else {
                            let r = creep.move_to(&ext);
                            if r != ReturnCode::Ok {
                                warn!(
                                    "could not move to spawn with id {}, code: {:?}",
                                    ext.id(),
                                    r
                                );
                            }
                        }
                        // warn!("could not resolve spawn with id {}", ext_id);
                    } else {
                        creep_targets.remove(&name);
                    }
                }
                CreepTarget::UpgradeConstructionSite(site) => {
                    if creep.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                        let r = creep.build(&site);
                        if r == ReturnCode::NotInRange {
                            creep.move_to(&site);
                        } else if r != ReturnCode::Ok {
                            warn!("(upgrade site) couldn't upgrade: {:?}", r);
                            creep_targets.remove(&name);
                        }
                    } else {
                        creep_targets.remove(&name);
                        // HARVEST random energy source
                        if let Some(source_id) = creep.pick_random_energy_source() {
                            creep_targets.insert(name, CreepTarget::Harvest(source_id));
                        } else {
                            warn!("could not find source with id");
                        }
                    }
                }
                CreepTarget::RepairRoad(road_id) => {
                    if creep.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                        let target = road_id.resolve().unwrap();
                        if target.hits() == target.hits_max() {
                            creep_targets.remove(&name);
                        }
                        let r = creep.repair(&target);
                        if r == ReturnCode::NotInRange {
                            creep.move_to(&target);
                        } else if r != ReturnCode::Ok {
                            warn!("could not repair road code: {:?}", r);
                            creep_targets.remove(&name);
                        }
                    } else {
                        creep_targets.remove(&name);
                        // HARVEST random energy source
                        if let Some(source_id) = creep.pick_random_energy_source() {
                            creep_targets.insert(name, CreepTarget::Harvest(source_id));
                        } else {
                            warn!("could not find source with id");
                        }
                    }
                }
            };
        }
        None => {
            // no target, let's find one depending on if we have energy
            let room = creep.room().expect("couldn't resolve creep room");
            if creep.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                // Upgrade controller, build shit or deposit in a spawn or extension
                let max_actions = 6;
                let rnd_number = rnd_source_idx(max_actions);
                if rnd_number < 1 {
                    // Upgrade Controller
                    for structure in room.find(find::STRUCTURES).iter() {
                        if let StructureObject::StructureController(controller) = structure {
                            creep_targets
                                .insert(name, CreepTarget::UpgradeController(controller.id()));
                            break;
                        }
                    }
                    return;
                } else if rnd_number < 4 {
                    // Deposit to spawn
                    for s in room.find(find::MY_SPAWNS).iter() {
                        if s.store().get_free_capacity(Some(ResourceType::Energy)) == 0 {
                            break;
                        }
                        creep_targets.insert(creep.name(), CreepTarget::DepositSpawn(s.id()));
                        return;
                    }
                    if creep.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                        if let Some(ext) = creep.find_unfilled_extension() {
                            creep_targets.insert(creep.name(), CreepTarget::DepositExtension(ext));
                        }
                    }
                } else {
                    // Upgrade EXTENSION
                    for site in room.find(find::CONSTRUCTION_SITES).iter() {
                        if site.structure_type() == StructureType::Extension {
                            creep_targets
                                .insert(name, CreepTarget::UpgradeConstructionSite(site.clone()));
                            return;
                        }
                    }
                    // Upgrade RANDOM CONSTRUCTION SITE
                    let site = creep.pos().find_closest_by_path(find::CONSTRUCTION_SITES);
                    match site {
                        Some(site) => {
                            creep_targets.insert(name, CreepTarget::UpgradeConstructionSite(site));
                            return;
                        }
                        _ => {}
                    }
                    // REPAIR ROADS
                    let objects = creep
                        .pos()
                        .find_closest_by_path(find::STRUCTURES)
                        .filter(|r| {
                            r.structure_type() == StructureType::Road
                                && r.as_attackable().unwrap().hits()
                                    > r.as_attackable().unwrap().hits_max() / 3
                        });
                    if objects.is_none() {
                        return;
                    }
                    for object in objects.iter() {
                        match object {
                            StructureObject::StructureRoad(road) => {
                                creep_targets
                                    .insert(name.clone(), CreepTarget::RepairRoad(road.id()));
                            }
                            _ => {}
                        }
                    }
                }
            } else {
                // HARVEST random energy source
                if let Some(source_id) = creep.pick_random_energy_source() {
                    creep_targets.insert(name, CreepTarget::Harvest(source_id));
                } else {
                    warn!("could not find source with id");
                }
            }
        }
    }
}

/// max is exclusive, i.e for max = 10, [0,10[
fn rnd_source_idx(max: usize) -> usize {
    js_sys::Math::floor(js_sys::Math::random() * max as f64) as usize
}
