use std::collections::HashMap;

use creep::*;
use log::*;
use screeps::{
    find, game, prelude::*, ObjectId, Part, RawMemory, ReturnCode, RoomObjectProperties, Source,
};
use storage::*;
use wasm_bindgen::prelude::*;

mod creep;
mod logging;
mod role;
mod storage;

// add wasm_bindgen to any function you would like to expose for call from js
#[wasm_bindgen]
pub fn setup() {
    logging::setup_logging(logging::Info);
}

// to use a reserved name as a function name, use `js_name`:
#[wasm_bindgen(js_name = loop)]
pub fn game_loop() {
    debug!("loop starting! CPU: {}", game::cpu::get_used());
    let mut harvest_sources = HashMap::<ObjectId<Source>, (usize, String)>::new();
    CREEPS_TARGET.with(|creep_targets_refcell| {
        let mut creep_targets = creep_targets_refcell.borrow_mut();
        debug!("running creeps");
        for creep in game::creeps().values() {
            let creep = Creep::new(&creep);
            creep.run_creep(&mut creep_targets);
        }
        // populate harvest_sources so we can next avoid to have many creeps trying to harvest
        for (creep_name, creep_target) in creep_targets.iter() {
            if let CreepTarget::Harvest(source_id) = creep_target {
                let total = harvest_sources
                    .entry(source_id.clone())
                    .or_insert((1, creep_name.clone()));
                (*total).0 += 1;
            }
        }
    });
    CREEPS_ROLE.with(|creep_role_refcell| {
        let mut creep_roles = creep_role_refcell.borrow_mut();
        for (creep_name, role) in creep_roles.iter() {}
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
                        info!(
                            "current source ({}) and next source ({})",
                            *object_id,
                            source.id()
                        );
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

        // Part::Move => 50,
        // Part::Work => 100,
        // Part::Carry => 50,
        // Part::Attack => 80,
        // Part::RangedAttack => 150,
        // Part::Tough => 10,
        // Part::Heal => 250,
        // Part::Claim => 600,
        // this 10 part is costing us -> 650
        let body = [
            Part::Carry,
            Part::Carry,
            Part::Work,
            Part::Work,
            Part::Work,
            Part::Move,
            Part::Move,
            Part::Move,
            Part::Move,
            Part::Move,
        ];
        if spawn.room().unwrap().energy_available() >= body.iter().map(|p| p.cost()).sum() {
            let name_base = game::time();
            let name = format!("{}-{}", name_base, additional);
            let res = spawn.spawn_creep(&body, &name);
            if res != ReturnCode::Ok {
                warn!("couldn't spawn: {:?}", res);
            } else {
                additional += 1;
            }
        }
    }

    let time = screeps::game::time();

    if time % 32 == 3 {
        let mut db = Database::init().expect("could not init database");
        info!("running memory cleanup");
        db.clean_up();
    }
    info!("done! cpu: {}", game::cpu::get_used())
}

struct Database {
    data: Root,
}

impl Database {
    fn init() -> Option<Self> {
        let root_json_string: String = RawMemory::get().into();
        match serde_json::from_str(root_json_string.as_str()) {
            Ok::<Root, _>(root_json) => Some(Self { data: root_json }),
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

/// max is exclusive, i.e for max = 10, [0,10[
fn rnd_source_idx(max: usize) -> usize {
    js_sys::Math::floor(js_sys::Math::random() * max as f64) as usize
}
