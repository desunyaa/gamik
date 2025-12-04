use crate::ServerMessage;
use bincode::{Decode, Encode};
use egui::ahash::HashMapExt;
use iroh::EndpointId;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Human {
    //  health: HumanHealth,
    //wearing: HumanWearing,
    body: HumanBody,
}

impl Human {
    pub fn new() -> Self {
        Human {
            body: HumanBody {
                skin_color: SkinColor::Bronze,
                hair_color: HairColor::Brunette,
                eye_color: EyeColor::Hazel,
                body_mods: Vec::new(),
            },
        }
    }
}

pub type BodyMod = (BodyPart, BodyAccesory);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum BodyAccesory {
    Piercing,
    Tattoo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum BodyPart {
    Ear(BodySide),
    Lip(BodyVertical),
    Arm(BodySide),
    Hand(BodySide),
    Leg(BodySide),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum BodySide {
    Left,
    Right,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum BodyVertical {
    Upper,
    Lower,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum SkinColor {
    Bronze,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum EyeColor {
    Hazel,
    Gray,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum HairColor {
    Brunette,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HumanBody {
    skin_color: SkinColor,
    hair_color: HairColor,
    eye_color: EyeColor,
    body_mods: Vec<BodyMod>,
}
