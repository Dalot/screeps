use std::cell::RefCell;
use std::collections::HashMap;

use log::*;
use screeps::{
    find, game, prelude::*, ConstructionSite, ObjectId, OwnedStructureObject, Part, RawMemory,
    ResourceType, ReturnCode, Room, RoomObjectProperties, Source, Structure, StructureController,
    StructureExtension, StructureObject, StructureSpawn, StructureType,
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
    static CREEPS_TARGET: RefCell<HashMap<String, CreepTarget>> = RefCell::new(HashMap::new());
    static CREEPS_MEMORY: RefCell<HashMap<String, CreepMemory>> = RefCell::new(HashMap::new());
}

// this enum will represent a creep's lock on a specific target object, storing a js reference to the object id so that we can grab a fresh reference to the object each successive tick, since screeps game objects become 'stale' and shouldn't be used beyond the tick they were fetched
#[derive(Clone)]
enum CreepTarget {
    UpgradeController(ObjectId<StructureController>),
    Upgrade(ObjectId<StructureController>), //TODO: delete this after migration
    UpgradeConstructionSite(ConstructionSite),
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

        let body = [Part::Move, Part::Move, Part::Carry, Part::Work, Part::Work];
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

struct Creep<'a> {
    inner_creep: &'a screeps::Creep,
}
impl<'a> Creep<'a> {
    fn name(&self) -> String {
        self.inner_creep.name()
    }
    fn spawning(&self) -> bool {
        self.inner_creep.spawning()
    }
    fn store(&self) -> screeps::Store {
        self.inner_creep.store()
    }
    fn pos(&self) -> screeps::Position {
        self.inner_creep.pos()
    }
    fn build(&self, target: &ConstructionSite) -> ReturnCode {
        self.inner_creep.build(target)
    }
    fn move_to<T>(&self, target: T) -> ReturnCode
    where
        T: HasPosition,
    {
        self.inner_creep.move_to(target)
    }
    pub fn harvest<T>(&self, target: &T) -> ReturnCode
    where
        T: ?Sized + Harvestable,
    {
        self.inner_creep.harvest(target)
    }
    pub fn upgrade_controller(&self, target: &StructureController) -> ReturnCode {
        self.inner_creep.upgrade_controller(target)
    }
    fn transfer<T>(&self, target: &T, ty: ResourceType, amount: Option<u32>) -> ReturnCode
    where
        T: Transferable,
    {
        self.inner_creep.transfer(target, ty, amount)
    }
    fn room(&self) -> Option<Room> {
        self.inner_creep.room()
    }
    fn is_upgrader_creep(&self) -> bool {
        match js_sys::Reflect::get(
            &self.inner_creep.memory(),
            &JsValue::from_str("is_upgrader"),
        ) {
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
    fn is_manual_creep(&self) -> bool {
        match js_sys::Reflect::get(&self.inner_creep.memory(), &JsValue::from_str("is_manual")) {
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
    fn pick_random_energy_source(&self) -> Option<ObjectId<screeps::Source>> {
        // let's pick a random energy source
        let room = self
            .inner_creep
            .room()
            .expect("couldn't resolve creep room");
        let sources = room.find(find::SOURCES_ACTIVE);
        let rnd_number = rnd_source_idx(sources.len());

        if let Some(source) = sources.get(rnd_number) {
            return Some(source.id());
        }
        None
    }
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
    let creep = Creep { inner_creep: creep };
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
                                    let mut value_to_transfer =
                                        creep.store().get_used_capacity(Some(ResourceType::Energy));
                                    let target_free_store: u32 = spawn
                                        .store()
                                        .get_free_capacity(Some(ResourceType::Energy))
                                        .try_into()
                                        .expect("could not convert i32 to u32");

                                    if target_free_store < value_to_transfer {
                                        value_to_transfer = target_free_store;
                                    }
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
                            let mut value_to_transfer =
                                creep.store().get_used_capacity(Some(ResourceType::Energy));
                            let target_free_store: u32 = ext
                                .store()
                                .get_free_capacity(Some(ResourceType::Energy))
                                .try_into()
                                .expect("could not convert i32 to u32");

                            if target_free_store < value_to_transfer {
                                value_to_transfer = target_free_store;
                            }
                            let r =
                                creep.transfer(ext, ResourceType::Energy, Some(value_to_transfer));
                            if r != ReturnCode::Ok {
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
                        } else {
                            creep.build(&site);
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
                CreepTarget::Upgrade(_) => {
                    creep_targets.remove(&name);
                }
            };
        }
        None => {
            // no target, let's find one depending on if we have energy
            let room = creep.room().expect("couldn't resolve creep room");
            if creep.store().get_used_capacity(Some(ResourceType::Energy)) > 0 {
                let max_actions = 3;
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
                } else if rnd_number < 2 {
                    for s in room.find(find::MY_STRUCTURES).iter() {
                        match s {
                            StructureObject::StructureExtension(val) => {
                                // TODO: creep may have more than 50 energy
                                if val.store().get_free_capacity(Some(ResourceType::Energy)) >= 50 {
                                    creep_targets.insert(
                                        creep.name(),
                                        CreepTarget::DepositExtension(val.clone()),
                                    );
                                    return;
                                }
                            }
                            _ => {}
                        }
                    }
                    // Deposit to spawn
                    for s in room.find(find::MY_SPAWNS).iter() {
                        creep_targets.insert(creep.name(), CreepTarget::DepositSpawn(s.id()));
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
                        }
                        None => warn!("could not find any construction sites"),
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
