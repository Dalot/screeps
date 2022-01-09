use std::collections::HashMap;

use creep::*;
use log::*;
use role::Role;
use screeps::{
    find, game, look, prelude::*, ObjectId, Part, RawMemory, ReturnCode, RoomObjectProperties,
    Source, StructureObject,
};
use storage::*;
use tower::*;
use wasm_bindgen::prelude::*;

mod creep;
mod logging;
mod role;
mod roles;
mod storage;
mod tower;

// add wasm_bindgen to any function you would like to expose for call from js
#[wasm_bindgen]
pub fn setup() {
    logging::setup_logging(logging::Info);
}

// to use a reserved name as a function name, use `js_name`:
#[wasm_bindgen(js_name = loop)]
pub fn game_loop() {
    let time = screeps::game::time();

    if time % 32 == 3 {
        let mut db = Database::init().expect("could not init database");
        db.assign_roles();
        info!("running memory cleanup");
        db.clean_up();
    }

    let mut creeps_role_map = HashMap::<String, Role>::new();
    // If a creep does not have a role, find the appropriate role and add it to the local storage
    CREEPS_ROLE.with(|creeps_role_refcell| {
        let mut creeps_role = creeps_role_refcell.borrow_mut();
        for creep in game::creeps().values() {
            let creep_name = creep.name();
            if let Some(r) = creeps_role.get(&creep.name()) {
                creeps_role_map.insert(creep_name, r.clone());
            } else {
                let role = Role::find_role(&creep);
                if let Some(r) = role {
                    creeps_role.insert(creep_name, r);
                } else {
                    creeps_role.insert(creep_name, Role::General);
                }
            }
        }
    });

    CREEPS_TARGET.with(|creeps_target_refcell| {
        let mut creeps_target = creeps_target_refcell.borrow_mut();
        for creep in game::creeps().values() {
            let mut creep = Creep::new(&creep);
            let r = creeps_role_map.get(&creep.name()).cloned();
            creep.set_role(r);
            let hostiles = creep.room().unwrap().find(find::HOSTILE_CREEPS);
            creep.run(&mut creeps_target, hostiles.len() > 0);
        }
    });

    TOWERS_TARGET.with(|towers_target_refcell| {
        let mut towers_target = towers_target_refcell.borrow_mut();
        for room in game::rooms().values() {
            let hostiles = room.find(find::HOSTILE_CREEPS);
            let structures = room.find(find::MY_STRUCTURES);
            let towers: Vec<&StructureObject> = structures
                .iter()
                .filter(|s| s.structure_type() == screeps::StructureType::Tower)
                .collect();
            for tower in towers {
                match tower {
                    StructureObject::StructureTower(screeps_t) => {
                        let t = Tower::new(screeps_t);
                        t.run(&mut towers_target, hostiles.clone());
                    }
                    _ => {
                        warn!("expected a tower here");
                    }
                }
            }
        }
    });

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
        // 15 part is costing us -> 1000
        let body = [
            Part::Carry,
            Part::Carry,
            Part::Carry,
            Part::Work,
            Part::Work,
            Part::Work,
            Part::Work,
            Part::Work,
            Part::Move,
            Part::Move,
            Part::Move,
            Part::Move,
            Part::Move,
            Part::Move,
            Part::Move,
        ];
        let name_base = game::time();
        let name = format!("{}-{}", name_base, additional);
        if let Some(role_needed) = Role::find_role_to_spawn(&creeps_role_map) {
            let b = role_needed.get_body();
            if spawn.room().unwrap().energy_available() >= b.iter().map(|p| p.cost()).sum() {
                let res = spawn.spawn_creep(&body, &name);
                if res != ReturnCode::Ok {
                    warn!("couldn't spawn: {:?}", res);
                } else {
                    additional += 1;
                }
            }
            continue;
        } else {
            // TODO: Have a phase on the game that will influence the body part
            if spawn.room().unwrap().energy_available() >= body.iter().map(|p| p.cost()).sum() {
                let res = spawn.spawn_creep(&body, &name);
                if res != ReturnCode::Ok {
                    warn!("couldn't spawn: {:?}", res);
                } else {
                    additional += 1;
                }
            }
        }
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

    fn assign_roles(&mut self) {
        for (name, creep) in self.data.creeps.iter_mut() {
            if let None = creep.role {
                if name == "34656950-0" {
                    creep.role = Some(Role::Harvester);
                }
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
