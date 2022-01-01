use log::*;
use screeps::{
    find, game, prelude::*, ConstructionSite, ObjectId, OwnedStructureObject, Part, RawMemory,
    ResourceType, ReturnCode, Room, RoomObject, RoomObjectProperties, Source, Structure,
    StructureController, StructureExtension, StructureObject, StructureRoad, StructureSpawn,
    StructureType,
};
use wasm_bindgen::prelude::*;
pub struct Creep<'a> {
    inner_creep: &'a screeps::Creep,
}
impl<'a> Creep<'a> {
    pub fn new(creep: &'a screeps::Creep) -> Self {
        Self { inner_creep: creep }
    }
    pub fn name(&self) -> String {
        self.inner_creep.name()
    }
    pub fn spawning(&self) -> bool {
        self.inner_creep.spawning()
    }
    pub fn store(&self) -> screeps::Store {
        self.inner_creep.store()
    }
    pub fn pos(&self) -> screeps::Position {
        self.inner_creep.pos()
    }
    pub fn build(&self, target: &ConstructionSite) -> ReturnCode {
        self.inner_creep.build(target)
    }
    pub fn repair(&self, target: &RoomObject) -> ReturnCode {
        self.inner_creep.repair(target)
    }
    pub fn move_to<T>(&self, target: T) -> ReturnCode
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
    pub fn transfer<T>(&self, target: &T, ty: ResourceType, amount: Option<u32>) -> ReturnCode
    where
        T: Transferable,
    {
        self.inner_creep.transfer(target, ty, amount)
    }
    pub fn room(&self) -> Option<Room> {
        self.inner_creep.room()
    }
    pub fn is_upgrader_creep(&self) -> bool {
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
    pub fn is_manual_creep(&self) -> bool {
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
    pub fn pick_random_energy_source(&self) -> Option<ObjectId<screeps::Source>> {
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
    pub fn get_value_to_transfer(&self, target_store: &screeps::Store) -> u32 {
        let mut value_to_transfer = self
            .inner_creep
            .store()
            .get_used_capacity(Some(ResourceType::Energy));
        let target_free_store: u32 = target_store
            .get_free_capacity(Some(ResourceType::Energy))
            .try_into()
            .expect("could not convert i32 to u32");

        if target_free_store < value_to_transfer {
            value_to_transfer = target_free_store;
        }
        value_to_transfer
    }
    pub fn find_unfilled_extension(&self) -> Option<StructureExtension> {
        for s in self
            .inner_creep
            .room()
            .unwrap()
            .find(find::MY_STRUCTURES)
            .iter()
        {
            match s {
                StructureObject::StructureExtension(ext) => {
                    if ext.store().get_free_capacity(Some(ResourceType::Energy)) > 0 {
                        return Some(ext.clone());
                    }
                }
                _ => {}
            }
        }
        None
    }
}
/// max is exclusive, i.e for max = 10, [0,10[
fn rnd_source_idx(max: usize) -> usize {
    js_sys::Math::floor(js_sys::Math::random() * max as f64) as usize
}
