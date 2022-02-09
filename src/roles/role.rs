use std::{collections::HashMap, hash::Hash};

use log::*;
use screeps::{
    game, prelude::*, ObjectId, Part, Position, ResourceType, ReturnCode, Source, Store, Structure,
    StructureObject, StructureSpawn, StructureType,
};
use serde::{Deserialize, Serialize};
use std::fmt::Display;

use crate::creep::*;

#[derive(PartialEq, Eq, Hash, Clone, Debug, Serialize, Deserialize)]
pub enum Role {
    Harvester,
    Hauler,
    Claimer,
    Warrior,
    Healer,
    Builder,
    Free,
    Tank,
    General,
}

pub trait Movable {
    fn move_to<T>(&self, target: T)
    where
        T: HasPosition;
}
pub trait CanHarvest {
    fn harvest(&self, source_id: &Source) -> bool;
}

pub struct Deposit {
    obj: StructureObject,
    position: Position,
    amount: u32,
    is_storage: bool,
}
impl<'a> HasPosition for Deposit {
    fn pos(&self) -> Position {
        self.position
    }
}
impl Deposit {
    pub fn new(o: StructureObject, amount: u32) -> Self {
        let pos = o.pos();
        let is_storage = o.structure_type() == StructureType::Storage;
        Self {
            obj: o,
            position: pos,
            amount,
            is_storage,
        }
    }
    pub fn transferable(&self) -> Box<&dyn Transferable> {
        Box::new(self.obj.as_transferable().unwrap())
    }
    pub fn withdrawable(&self) -> Box<&dyn Withdrawable> {
        Box::new(self.obj.as_withdrawable().unwrap())
    }
    pub fn store(&self) -> Store {
        self.obj.as_has_store().unwrap().store()
    }
    pub fn amount(&self) -> u32 {
        self.amount
    }
    pub fn is_storage(&self) -> bool {
        self.is_storage
    }
}
pub trait CanDeposit {
    fn find_closest_depositable(&self, danger: bool) -> Option<Deposit>;
    fn find_closest_container(&self) -> Option<Deposit>;
    fn deposit(&self, target: Deposit) -> DepositCode;
}

#[derive(PartialEq)]
pub enum DepositCode {
    Done = 0,
    NotNear = 1,
    Full = 2,
    Error = 3,
    NotDone = 4,
}

const MOVE_POS: usize = 0;
const WORK_POS: usize = 1;
const CARRY_POS: usize = 2;
const RANGED_ATTACK_POS: usize = 5;
const ATTACK_POS: usize = 4;
const TOUGH_POS: usize = 5;
const HEAL_POS: usize = 6;
const CLAIM_POS: usize = 7;

const HARVESTER_POS: usize = 0;
const HAULER_POS: usize = 1;
const CLAIMER_POS: usize = 2;
const WARRIOR_POS: usize = 3;
const HEALER_POS: usize = 4;
const BUILDER_POS: usize = 5;
const FREE_POS: usize = 6;
const TANK_POS: usize = 7;
const GENERAL_POS: usize = 8;

impl Role {
    pub fn to_string(&self) -> &str {
        match self {
            Role::Harvester => "HARVESTER",
            Role::Hauler => "HAULER",
            Role::Claimer => "CLAIMER",
            Role::Warrior => "WARRIOR",
            Role::Healer => "HEALER",
            Role::Builder => "BUILDER",
            Role::Free => "WILDLING",
            Role::Tank => "TANK",
            Role::General => "GENERAL",
        }
    }
    pub fn find_role(c: &screeps::Creep) -> Option<Role> {
        let index_to_role: HashMap<usize, Role> = [
            (WORK_POS, Role::Harvester),
            (CARRY_POS, Role::Hauler),
            (HEAL_POS, Role::Healer),
            (ATTACK_POS, Role::Warrior),
            (CLAIM_POS, Role::Claimer),
            (TOUGH_POS, Role::Tank),
        ]
        .iter()
        .cloned()
        .collect();

        let mut counters = [0; 8];
        for p in c.body().iter() {
            match p.part() {
                screeps::Part::Move => {
                    counters[MOVE_POS] += 1;
                }
                screeps::Part::Work => {
                    counters[WORK_POS] += 1;
                }
                screeps::Part::Carry => {
                    counters[CARRY_POS] += 1;
                }
                screeps::Part::Attack => {
                    counters[ATTACK_POS] += 1;
                }
                screeps::Part::RangedAttack => {
                    counters[RANGED_ATTACK_POS] += 1;
                }
                screeps::Part::Tough => {
                    counters[TOUGH_POS] += 1;
                }
                screeps::Part::Heal => {
                    counters[HEAL_POS] += 1;
                }
                screeps::Part::Claim => {
                    counters[CARRY_POS] += 1;
                }
                part => {
                    warn!("did not expect this part {:?}", part);
                }
            }
        }
        if counters[MOVE_POS] == 1 {
            return Some(Role::Harvester);
        };
        if c.body().len() == 15 {
            let rnd_number = rnd_source_idx(5);
            if rnd_number < 1 {
                return Some(Role::Hauler);
            } else {
                return Some(Role::Builder);
            }
        }
        if counters[WORK_POS] > 1 {
            return Some(Role::Builder);
        };
        Some(Role::Hauler)
    }

    pub fn find_role_to_spawn(roles: &Vec<Role>, num_of_creeps: u32) -> Option<Role> {
        let ordered_roles = vec![
            Role::Harvester,
            Role::Hauler,
            Role::Warrior,
            Role::Healer,
            Role::Builder,
            Role::Tank,
            Role::General,
            Role::Claimer,
        ];
        let role_to_desired_num: HashMap<Role, usize> = [
            (Role::Harvester, 2),
            (Role::Hauler, 5),
            (Role::Warrior, 0),
            (Role::Healer, 0),
            (Role::Builder, 1),
            (Role::Tank, 0),
            (Role::General, 0),
            (Role::Claimer, 0),
            // (Role::Free, 0),
        ]
        .iter()
        .cloned()
        .collect();
        let mut counters = [0 as usize; 9];
        for role in roles.iter() {
            match role {
                Role::Harvester => {
                    counters[HARVESTER_POS] += 1;
                }
                Role::Hauler => {
                    counters[HAULER_POS] += 1;
                }
                Role::Claimer => {
                    counters[CLAIMER_POS] += 1;
                }
                Role::Warrior => {
                    counters[WARRIOR_POS] += 1;
                }
                Role::Healer => {
                    counters[HEALER_POS] += 1;
                }
                Role::Builder => {
                    counters[BUILDER_POS] += 1;
                }
                Role::Free => {
                    counters[FREE_POS] += 1;
                }
                Role::Tank => {
                    counters[TANK_POS] += 1;
                }
                Role::General => {
                    counters[GENERAL_POS] += 1;
                }
            }
        }
        info!("counters: {:?}", counters);
        for r in ordered_roles.iter() {
            let desired_num = role_to_desired_num.get(r).unwrap();
            match r {
                Role::Harvester => {
                    if *desired_num > counters[HARVESTER_POS] && num_of_creeps > 2 {
                        return Some(r.clone());
                    }
                }
                Role::Hauler => {
                    if *desired_num > counters[HAULER_POS] {
                        return Some(r.clone());
                    }
                }
                Role::Claimer => {
                    if *desired_num > counters[CLAIMER_POS] {
                        return Some(r.clone());
                    }
                }
                Role::Warrior => {
                    if *desired_num > counters[WARRIOR_POS] {
                        return Some(r.clone());
                    }
                }
                Role::Healer => {
                    if *desired_num > counters[HEALER_POS] {
                        return Some(r.clone());
                    }
                }
                Role::Builder => {
                    if *desired_num > counters[BUILDER_POS] {
                        return Some(r.clone());
                    }
                }
                Role::Free => {
                    if *desired_num > counters[FREE_POS] {
                        return Some(r.clone());
                    }
                }
                Role::Tank => {
                    if *desired_num > counters[TANK_POS] {
                        return Some(r.clone());
                    }
                }
                Role::General => {
                    if *desired_num > counters[GENERAL_POS] {
                        return Some(r.clone());
                    }
                }
            }
        }

        None
    }

    pub fn get_body(
        &self,
        energy_available: u32,
        capacity: u32,
        num_creeps: u32,
    ) -> Option<Vec<Part>> {
        if energy_available < 300 {
            return None;
        }

        let mut energy_to_use = energy_available;
        if capacity > energy_available && num_creeps > 3 {
            energy_to_use = capacity;
        }

        match self {
            Role::Harvester => {
                let mut parts = [Part::Work, Part::Work, Part::Move].to_vec();
                let missing_parts = (energy_to_use - 250) / 100;
                for _ in 0..missing_parts {
                    parts.push(Part::Work);
                }
                Some(parts)
            }
            Role::Hauler => {
                let mut parts = [Part::Carry, Part::Move, Part::Move].to_vec();
                let missing_parts = (energy_to_use - 150) / 150;
                for _ in 0..missing_parts {
                    parts.push(Part::Carry);
                    parts.push(Part::Move);
                    parts.push(Part::Move);
                }
                Some(parts)
            }
            Role::Builder | _ => {
                let mut parts = [Part::Carry, Part::Move, Part::Work].to_vec();
                let missing_parts = (energy_to_use - 200) / 200;
                for _ in 0..missing_parts {
                    parts.push(Part::Carry);
                    parts.push(Part::Work);
                    parts.push(Part::Move);
                }
                Some(parts)
            }
        }
    }
}
fn rnd_source_idx(max: usize) -> usize {
    js_sys::Math::floor(js_sys::Math::random() * max as f64) as usize
}
