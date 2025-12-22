/*



use bitcode::{Decode, Encode};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Human {
    //  health: HumanHealth,
    //wearing: HumanWearing,
    body: HumanBody,
}
//should add a from_rng() function for saving space on npcs
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
    // ========== HEAD ==========
    Scalp,
    Forehead,
    Temple(BodySide),
    Eyebrow(BodySide),
    Eyelid(BodyVertical), // Upper / Lower
    Eye(BodySide),
    Ear(BodySide),
    Cheek(BodySide),
    Nose,
    Nostril(BodySide),
    Lip(BodyVertical), // Upper / Lower
    Teeth,             // general
    Tongue,
    Jaw,
    Chin,

    // ========== NECK ==========
    Throat,
    Nape, // back of neck
    NeckSide(BodySide),

    // ========== TORSO (FRONT) ==========
    Chest,
    Breast(BodySide),
    Sternum,
    Abdomen,
    Navel,
    Pelvis,

    // ========== TORSO (BACK) ==========
    UpperBack,
    LowerBack,
    ShoulderBlade(BodySide),
    Spine,

    // ========== TORSO (SIDES) ==========
    Rib(BodySide),
    Waist(BodySide),

    // ========== ARMS ==========
    Shoulder(BodySide),
    UpperArm(BodySide),
    Elbow(BodySide),
    Forearm(BodySide),
    Wrist(BodySide),

    // Hands (detailed)
    Hand(BodySide),
    Palm(BodySide),
    Finger(BodySide, FingerType), // new
    Knuckle(BodySide, FingerType),

    // ========== LEGS ==========
    Hip(BodySide),
    Thigh(BodySide),
    Knee(BodySide),
    Shin(BodySide),
    Calf(BodySide),
    Ankle(BodySide),

    // Feet (detailed)
    Foot(BodySide),
    Heel(BodySide),
    Sole(BodySide),
    Toe(BodySide, ToeType), // new
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum FingerType {
    Thumb,
    Index,
    Middle,
    Ring,
    Pinky,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum ToeType {
    Big,
    Second,
    Middle,
    Fourth,
    Little,
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

*/
