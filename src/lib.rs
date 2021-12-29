use std::cell::RefCell;
use std::collections::HashMap;

use log::*;
use screeps::{
    find, game, prelude::*, Creep, ObjectId, Part, RawMemory, ResourceType, ReturnCode,
    RoomObjectProperties, Source, StructureController, StructureObject, StructureSpawn,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

mod logging;

// add wasm_bindgen to any function you would like to expose for call from js
#[wasm_bindgen]
pub fn setup() {
    logging::setup_logging(logging::Info);
}

// this is one way to persist data between ticks within Rust's memory, as opposed to
// keeping state in memory on game objects - but will be lost on global resets!
thread_local! {
    static CREEP_TARGETS: RefCell<HashMap<String, CreepTarget>> = RefCell::new(HashMap::new());
}

// this enum will represent a creep's lock on a specific target object, storing a js reference to the object id so that we can grab a fresh reference to the object each successive tick, since screeps game objects become 'stale' and shouldn't be used beyond the tick they were fetched
#[derive(Clone)]
enum CreepTarget {
    Upgrade(ObjectId<StructureController>),
    Harvest(ObjectId<Source>),
    Deposit(ObjectId<StructureSpawn>),
}

// to use a reserved name as a function name, use `js_name`:
#[wasm_bindgen(js_name = loop)]
pub fn game_loop() {
    debug!("loop starting! CPU: {}", game::cpu::get_used());
    // mutably borrow the creep_targets refcell, which is holding our creep target locks
    // in the wasm heap
    CREEP_TARGETS.with(|creep_targets_refcell| {
        let mut creep_targets = creep_targets_refcell.borrow_mut();
        debug!("running creeps");
        // same type conversion (and type assumption) as the spawn loop
        for creep in game::creeps().values() {
            run_creep(&creep, &mut creep_targets);
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

        let body = [Part::Move, Part::Move, Part::Carry, Part::Work];
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
        clean_up();
    }
    info!("done! cpu: {}", game::cpu::get_used())
}

#[derive(Debug, Serialize, Deserialize)]
struct Root {
    creeps: HashMap<String, CreepMemory>,
}
#[derive(Debug, Serialize, Deserialize)]
struct CreepMemory {
    _move: Option<Move>,
    is_manual: Option<bool>,
    is_upgrader: Option<bool>,
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
fn clean_up() {
    let root_json_string: String = RawMemory::get().into();
    match serde_json::from_str(root_json_string.as_str()) {
        Ok::<Root, _>(mut root_json) => {
            info!("ROOT: {:?}", root_json);
            let mut to_remove = Vec::<String>::new();
            for (name, _) in root_json.creeps.iter() {
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
                let removed_creep_js = root_json.creeps.remove(name);
                if let None = removed_creep_js {
                    info!(
                        "tried to remove inexistent creep in memory object, name: {}",
                        name
                    );
                }
            }
            match serde_json::to_string(&root_json) {
                Ok::<String, _>(root_json) => {
                    RawMemory::set(&js_sys::JsString::from(root_json));
                }
                Err(e) => info!("could not serialize root_json: {}", e),
            }
        }
        Err(e) => info!("could not deserialize root_json: {}", e),
    }
}

fn is_upgrader_creep(creep: &Creep) -> bool {
    match js_sys::Reflect::get(&creep.memory(), &JsValue::from_str("is_upgrader")) {
        Ok(val) => match val.as_bool() {
            Some(v) => v,

            None => {
                debug!("could not find any boolean value inside");
                false
            }
        },
        Err(e) => {
            warn!("could not deserialize creep memory, err: {:?}", e);
            false
        }
    }
}
fn is_manual_creep(creep: &Creep) -> bool {
    match js_sys::Reflect::get(&creep.memory(), &JsValue::from_str("is_manual")) {
        Ok(val) => match val.as_bool() {
            Some(v) => v,

            None => {
                debug!("could not find any boolean value inside");
                false
            }
        },
        Err(e) => {
            warn!("could not deserialize creep memory, err: {:?}", e);
            false
        }
    }
}

fn run_creep(creep: &Creep, creep_targets: &mut HashMap<String, CreepTarget>) {
    if creep.spawning() {
        return;
    }
    if is_manual_creep(creep) {
        return;
    }
    if is_upgrader_creep(creep) {
        if creep.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
            let site = creep.pos().find_closest_by_path(find::CONSTRUCTION_SITES);
            match site {
                Some(site) => {
                    let r = creep.build(&site);
                    if r == ReturnCode::NotInRange {
                        creep.move_to(&site);
                    } else if r != ReturnCode::Ok {
                        warn!("couldn't upgrade: {:?}", r);
                    } else {
                        warn!("could not upgrade construction site: {:?}", r);
                    }
                }
                None => warn!("could not find any construction sites"),
            }
        } else {
            let source = creep.pos().find_closest_by_path(find::SOURCES_ACTIVE);
            match source {
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
                None => warn!("could not find any active source"),
            }
        }
        return;
    }
    let name = creep.name();
    debug!("running creep {}", name);

    let target = creep_targets.remove(&name);
    match target {
        Some(creep_target) => {
            let keep_target = match &creep_target {
                CreepTarget::Upgrade(controller_id) => {
                    if creep.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                        match controller_id.resolve() {
                            Some(controller) => {
                                let r = creep.upgrade_controller(&controller);
                                if r == ReturnCode::NotInRange {
                                    creep.move_to(&controller);
                                    true
                                } else if r != ReturnCode::Ok {
                                    warn!("couldn't upgrade: {:?}", r);
                                    false
                                } else {
                                    true
                                }
                            }
                            None => false,
                        }
                    } else {
                        false
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
                                        false
                                    } else {
                                        true
                                    }
                                } else {
                                    creep.move_to(&source);
                                    true
                                }
                            }
                            None => false,
                        }
                    } else {
                        false
                    }
                }
                CreepTarget::Deposit(spawn_id) => {
                    if creep.store().get_free_capacity(Some(ResourceType::Energy)) > 0 {
                        match spawn_id.resolve() {
                            Some(spawn) => {
                                if creep.pos().is_near_to(spawn.pos()) {
                                    let r = creep.transfer(
                                        &spawn,
                                        ResourceType::Energy,
                                        Some(
                                            creep
                                                .store()
                                                .get_used_capacity(Some(ResourceType::Energy)),
                                        ),
                                    );
                                    if r != ReturnCode::Ok {
                                        warn!(
                                            "could not make a transfer from creep {}, to spawn {}",
                                            creep.name(),
                                            spawn.name()
                                        );
                                    }
                                    false
                                } else {
                                    let r = creep.move_to(&spawn);
                                    if r != ReturnCode::Ok {
                                        warn!("could not move to spawn with id {}", spawn_id);
                                        false
                                    } else {
                                        true
                                    }
                                }
                            }
                            None => {
                                warn!("could not resolve spawn with id {}", spawn_id);
                                false
                            }
                        }
                    } else {
                        false
                    }
                }
            };

            if keep_target {
                creep_targets.insert(name, creep_target);
            }
        }
        None => {
            // no target, let's find one depending on if we have energy
            let room = creep.room().expect("couldn't resolve creep room");
            if creep.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                // Half change that will harvest, half chance that will upgrade controller
                let rnd_number = rnd_source_idx(2);
                if rnd_number == 0 {
                    for structure in room.find(find::STRUCTURES).iter() {
                        if let StructureObject::StructureController(controller) = structure {
                            creep_targets.insert(name, CreepTarget::Upgrade(controller.id()));
                            break;
                        }
                    }
                } else {
                    for s in room.find(find::MY_SPAWNS).iter() {
                        creep_targets.insert(creep.name(), CreepTarget::Deposit(s.id()));
                    }
                }
            } else {
                // let's pick a random energy source
                let sources = room.find(find::SOURCES_ACTIVE);
                let rnd_number = rnd_source_idx(sources.len());

                if let Some(source) = sources.get(rnd_number) {
                    creep_targets.insert(name, CreepTarget::Harvest(source.id()));
                }
            }
        }
    }
}

/// max is exclusive, i.e for max = 10, [0,10[
fn rnd_source_idx(max: usize) -> usize {
    js_sys::Math::floor(js_sys::Math::random() * max as f64) as usize
}
