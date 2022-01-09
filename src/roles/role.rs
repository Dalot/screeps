use std::{collections::HashMap, hash::Hash};

use log::*;
use screeps::{
    prelude::*, ObjectId, Part, Position, ResourceType, ReturnCode, Source, Structure,
    StructureObject, StructureSpawn,
};
use serde::{Deserialize, Serialize};

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
    fn move_to<T>(&self, target: T) -> ReturnCode
    where
        T: HasPosition;
}
pub trait CanHarvest {
    fn harvest(&self, source_id: &Source) -> bool;
}

pub struct Deposit<'a> {
    target: Box<&'a dyn Transferable>,
    position: Position,
    amount: u32,
}
impl<'a> HasPosition for Deposit<'a> {
    fn pos(&self) -> Position {
        self.position
    }
}
impl<'a> Deposit<'a> {
    pub fn target(&self) -> &Box<&'a dyn Transferable> {
        &self.target
    }
    pub fn amount(&self) -> u32 {
        self.amount
    }
}
pub trait CanDeposit {
    fn find_closest_depositable(&self, including_containers: bool) -> Option<Deposit>;
    fn deposit(&self, target: Deposit) -> DepositCode;
    fn find_closest_bank(&self) -> Option<Deposit>;
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
const WARRIOR_POS: usize = 5;
const HEALER_POS: usize = 4;
const BUILDER_POS: usize = 5;
const FREE_POS: usize = 6;
const TANK_POS: usize = 7;
const GENERAL_POS: usize = 8;

impl Role {
    pub fn find_role(c: &screeps::Creep) -> Option<Role> {
        let index_to_role: HashMap<usize, Role> = [
            (MOVE_POS, Role::Hauler),
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
        let index_of_max = counters
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.cmp(b))
            .map(|(index, _)| index);

        if let Some(i) = index_of_max {
            if let Some(r) = index_to_role.get(&i) {
                Some(r.to_owned())
            } else {
                // This means that the creep can be a Harvest, Builder, or Free
                if i == WORK_POS {
                    // Here we can see that creep is an Harvester or a Builder
                    if counters[MOVE_POS] > 0 {
                        Some(Role::Builder)
                    } else {
                        Some(Role::Harvester)
                    }
                } else {
                    Some(Role::General)
                }
            }
        } else {
            None
        }
    }

    pub fn find_role_to_spawn(creeps_role: &HashMap<String, Role>) -> Option<Role> {
        let role_to_desired_num: HashMap<Role, usize> = [
            (Role::Harvester, 1),
            (Role::Hauler, 1),
            (Role::Warrior, 0),
            (Role::Healer, 0),
            (Role::Builder, 0),
            (Role::Tank, 0),
            (Role::General, 0),
            (Role::Claimer, 0),
            // (Role::Free, 0),
        ]
        .iter()
        .cloned()
        .collect();
        let mut counters = [0 as usize; 8];
        for (_, role) in creeps_role {
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
        for (r, num) in role_to_desired_num.iter() {
            match r {
                Role::Harvester => {
                    if *num < counters[HARVESTER_POS] {
                        return Some(r.clone());
                    }
                }
                Role::Hauler => {
                    if *num < counters[HAULER_POS] {
                        return Some(r.clone());
                    }
                }
                Role::Claimer => {
                    if *num < counters[CLAIMER_POS] {
                        return Some(r.clone());
                    }
                }
                Role::Warrior => {
                    if *num < counters[WARRIOR_POS] {
                        return Some(r.clone());
                    }
                }
                Role::Healer => {
                    if *num < counters[HEALER_POS] {
                        return Some(r.clone());
                    }
                }
                Role::Builder => {
                    if *num < counters[BUILDER_POS] {
                        return Some(r.clone());
                    }
                }
                Role::Free => {
                    if *num < counters[FREE_POS] {
                        return Some(r.clone());
                    }
                }
                Role::Tank => {
                    if *num < counters[TANK_POS] {
                        return Some(r.clone());
                    }
                }
                Role::General => {
                    if *num < counters[HARVESTER_POS] {
                        return Some(r.clone());
                    }
                }
            }
        }

        None
    }

    pub fn get_body(&self) -> Vec<Part> {
        match self {
            Role::Harvester => [
                Part::Work,
                Part::Work,
                Part::Work,
                Part::Work,
                Part::Work,
                Part::Work,
                Part::Work,
                Part::Move,
            ]
            .to_vec(),
            Role::Hauler => [
                Part::Carry,
                Part::Carry,
                Part::Carry,
                Part::Carry,
                Part::Move,
                Part::Move,
                Part::Move,
                Part::Move,
                Part::Move,
                Part::Move,
                Part::Move,
                Part::Move,

            ]
                .to_vec(),
            Role::Builder => [
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
            ]
                .to_vec(),
            _ => [
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
            ].to_vec()
            // Role::Claimer => todo!(),
            // Role::Warrior => todo!(),
            // Role::Healer => todo!(),
            // Role::Free => todo!(),
            // Role::Tank => todo!(),
            // Role::General => todo!(),
        }
    }
}
