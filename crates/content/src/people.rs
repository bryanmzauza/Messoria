//! The characters file: the figure characters are built as, the skins they
//! are dressed in and how they hold things.

use std::path::Path;

use serde::Deserialize;

use crate::error::Problem;

/// How characters are built and dressed.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Characters {
    /// Meters per figure pixel, the unit every length of the figure is in.
    pub pixel: f32,
    /// Skin texels per figure pixel.
    pub texels: u32,
    /// Width and height of every skin, in texels.
    pub skin_size: (u32, u32),
    pub limbs: Limbs,
    /// Where the front of the head is painted for each expression.
    pub expressions: Expressions,
    /// Where a held item sits in the right hand.
    pub grip: Grip,
    /// What players are dressed in; each player gets one of each.
    pub wardrobe: Wardrobe,
}

/// The limbs of a figure. The body and the thighs turn at the hips, measured
/// from the feet; the head and the upper arms at the neck and the shoulders,
/// measured from the body's joint; the forearms at the elbows, measured from
/// the shoulders, and the shins at the knees, measured from the hips.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limbs {
    pub body: Limb,
    pub head: Limb,
    pub right_arm: Limb,
    pub left_arm: Limb,
    pub right_leg: Limb,
    pub left_leg: Limb,
    pub right_forearm: Limb,
    pub left_forearm: Limb,
    pub right_shin: Limb,
    pub left_shin: Limb,
}

impl Limbs {
    /// Every limb, in the order of [`Limbs`]'s fields.
    pub fn all(&self) -> [&Limb; 10] {
        [
            &self.body,
            &self.head,
            &self.right_arm,
            &self.left_arm,
            &self.right_leg,
            &self.left_leg,
            &self.right_forearm,
            &self.left_forearm,
            &self.right_shin,
            &self.left_shin,
        ]
    }
}

/// Where the front of the head (the head's first box) is painted for each
/// expression other than the resting one, as the picture's top left corner,
/// in texels. The resting face is painted where the box's own unwrap puts it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expressions {
    /// Eyes shut, for blinking and for sleep.
    pub blink: (u32, u32),
    pub smile: (u32, u32),
    pub surprise: (u32, u32),
    /// Straining, as when swinging a tool.
    pub effort: (u32, u32),
}

impl Expressions {
    pub fn all(&self) -> [(u32, u32); 4] {
        [self.blink, self.smile, self.surprise, self.effort]
    }
}

/// A part of a figure that turns as one, and the boxes it is made of.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limb {
    /// Where the limb turns, from its parent's joint, in figure pixels.
    pub joint: (f32, f32, f32),
    pub boxes: Vec<Cube>,
}

/// A box of a limb, painted from the skin.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cube {
    /// Its lowest corner, from the limb's joint, in figure pixels.
    pub from: (f32, f32, f32),
    pub size: (f32, f32, f32),
    /// Where its faces are unwrapped in the skin, in texels.
    pub uv: (u32, u32),
    /// How far it grows on every side beyond its size, such as hair over a
    /// head, without changing how it is painted.
    #[serde(default)]
    pub inflate: f32,
}

impl Cube {
    /// Width and height of the box's unwrapped faces, in texels.
    pub fn unwrapped(&self, texels: u32) -> (f32, f32) {
        #[expect(clippy::cast_precision_loss, reason = "a few texels per pixel")]
        let texels = texels as f32;
        let (width, height, depth) = self.size;
        (2.0 * (width + depth) * texels, (height + depth) * texels)
    }
}

/// Where a held item sits in the right hand, from the forearm's joint, and
/// how large it is drawn there.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grip {
    /// In figure pixels.
    pub at: (f32, f32, f32),
    /// How tools are turned in the hand, around x, y and z, in degrees, in
    /// that order; anything else is held upright.
    pub turn: (f32, f32, f32),
    /// The longest side of a held tool, and of anything else held, in
    /// meters: models come in every size, and are fitted to these.
    pub tool_size: f32,
    pub item_size: f32,
}

/// The skins players are dressed in, as files in the skins folder.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wardrobe {
    pub bodies: Vec<String>,
    pub outfits: Vec<String>,
    /// Painted in grays, to be tinted with a hair color.
    pub hair: Vec<String>,
    /// In sRGB, from 0 to 1.
    pub hair_colors: Vec<(f32, f32, f32)>,
}

/// How one character is dressed: a skin laid in layers, a body with an
/// outfit over it and hair over both.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Look {
    pub body: String,
    pub outfit: String,
    pub hair: String,
    pub hair_color: (f32, f32, f32),
}

impl Look {
    /// The skin files it is laid from, bottom layer first.
    pub fn layers(&self) -> [&str; 3] {
        [&self.body, &self.outfit, &self.hair]
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if let Some(file) = self.layers().into_iter().find(|file| !is_skin(file)) {
            return Err(format!(
                "`{file}` is not a .png file inside the skins folder"
            ));
        }
        if !in_unit_range(self.hair_color) {
            return Err("its hair color has a channel outside 0 to 1".to_owned());
        }
        Ok(())
    }
}

impl Characters {
    /// The look of the player whose id is `seed`: the same on every screen,
    /// and spread over the whole wardrobe.
    pub fn look_for(&self, seed: u64) -> Look {
        let wardrobe = &self.wardrobe;
        let mut bits = mix(seed);
        let mut pick = |count: usize| {
            // Both conversions are lossless: a count fits in u64, and what is
            // picked is below it.
            let picked = bits % u64::try_from(count).unwrap_or(u64::MAX);
            bits = mix(bits);
            usize::try_from(picked).unwrap_or_default()
        };
        Look {
            body: wardrobe.bodies[pick(wardrobe.bodies.len())].clone(),
            outfit: wardrobe.outfits[pick(wardrobe.outfits.len())].clone(),
            hair: wardrobe.hair[pick(wardrobe.hair.len())].clone(),
            hair_color: wardrobe.hair_colors[pick(wardrobe.hair_colors.len())],
        }
    }

    /// Every skin file the wardrobe uses.
    pub fn skins_used(&self) -> impl Iterator<Item = &String> {
        let wardrobe = &self.wardrobe;
        wardrobe
            .bodies
            .iter()
            .chain(&wardrobe.outfits)
            .chain(&wardrobe.hair)
    }

    pub(crate) fn validate(&self) -> Result<(), Problem> {
        let problem = |reason: String| Err(Problem::InvalidCharacters(reason));
        if !(f32::MIN_POSITIVE..).contains(&self.pixel)
            || !(f32::MIN_POSITIVE..).contains(&self.grip.tool_size)
            || !(f32::MIN_POSITIVE..).contains(&self.grip.item_size)
            || self.texels == 0
            || self.skin_size.0 == 0
            || self.skin_size.1 == 0
        {
            return problem(
                "the pixel, texels, skin size and held sizes must be above 0".to_owned(),
            );
        }
        for (index, limb) in self.limbs.all().into_iter().enumerate() {
            if let Err(reason) = self.check_limb(limb) {
                return problem(format!("limb {} {reason}", index + 1));
            }
        }
        let Some(head) = self.limbs.head.boxes.first() else {
            return problem("the head has no box".to_owned());
        };
        #[expect(clippy::cast_precision_loss, reason = "skins are small images")]
        let (texels, skin_width, skin_height) = (
            self.texels as f32,
            self.skin_size.0 as f32,
            self.skin_size.1 as f32,
        );
        let (face_width, face_height) = (head.size.0 * texels, head.size.1 * texels);
        #[expect(clippy::cast_precision_loss, reason = "skins are small images")]
        let beyond = |(u, v): (u32, u32)| {
            u as f32 + face_width > skin_width || v as f32 + face_height > skin_height
        };
        if self.expressions.all().into_iter().any(beyond) {
            return problem("an expression is painted beyond the skin".to_owned());
        }
        let wardrobe = &self.wardrobe;
        if wardrobe.bodies.is_empty()
            || wardrobe.outfits.is_empty()
            || wardrobe.hair.is_empty()
            || wardrobe.hair_colors.is_empty()
        {
            return problem(
                "the wardrobe needs a body, an outfit, hair and a hair color".to_owned(),
            );
        }
        if let Some(file) = self.skins_used().find(|file| !is_skin(file)) {
            return problem(format!(
                "`{file}` is not a .png file inside the skins folder"
            ));
        }
        if !wardrobe.hair_colors.iter().copied().all(in_unit_range) {
            return problem("a hair color has a channel outside 0 to 1".to_owned());
        }
        Ok(())
    }

    fn check_limb(&self, limb: &Limb) -> Result<(), &'static str> {
        if limb.boxes.is_empty() {
            return Err("has no box");
        }
        #[expect(clippy::cast_precision_loss, reason = "skins are small images")]
        let (skin_width, skin_height) = (self.skin_size.0 as f32, self.skin_size.1 as f32);
        for cube in &limb.boxes {
            let (width, height, depth) = cube.size;
            if width <= 0.0 || height <= 0.0 || depth <= 0.0 || cube.inflate < 0.0 {
                return Err("has a box without volume");
            }
            let (unwrapped_width, unwrapped_height) = cube.unwrapped(self.texels);
            #[expect(clippy::cast_precision_loss, reason = "skins are small images")]
            let (u, v) = (cube.uv.0 as f32, cube.uv.1 as f32);
            if u + unwrapped_width > skin_width || v + unwrapped_height > skin_height {
                return Err("has a box painted beyond the skin");
            }
        }
        Ok(())
    }
}

/// Whether `file` names a PNG image inside the skins folder.
fn is_skin(file: &str) -> bool {
    let png = Path::new(file)
        .extension()
        .is_some_and(|extension| extension == "png");
    png && !file.contains('\\') && file.split('/').all(|part| !part.is_empty() && part != "..")
}

fn in_unit_range((red, green, blue): (f32, f32, f32)) -> bool {
    [red, green, blue]
        .iter()
        .all(|channel| (0.0..=1.0).contains(channel))
}

/// Scrambles `value` so that nearby seeds give unrelated picks (`SplitMix64`'s
/// finalizer).
fn mix(value: u64) -> u64 {
    let mut z = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn characters() -> Characters {
        let cube = Cube {
            from: (0.0, 0.0, 0.0),
            size: (2.0, 2.0, 2.0),
            uv: (0, 0),
            inflate: 0.0,
        };
        let limb = Limb {
            joint: (0.0, 0.0, 0.0),
            boxes: vec![cube],
        };
        Characters {
            pixel: 0.1,
            texels: 2,
            skin_size: (16, 16),
            limbs: Limbs {
                body: limb.clone(),
                head: limb.clone(),
                right_arm: limb.clone(),
                left_arm: limb.clone(),
                right_leg: limb.clone(),
                left_leg: limb.clone(),
                right_forearm: limb.clone(),
                left_forearm: limb.clone(),
                right_shin: limb.clone(),
                left_shin: limb,
            },
            expressions: Expressions {
                blink: (0, 8),
                smile: (4, 8),
                surprise: (8, 8),
                effort: (12, 8),
            },
            grip: Grip {
                at: (0.0, 0.0, 0.0),
                turn: (0.0, 0.0, 0.0),
                tool_size: 1.0,
                item_size: 0.5,
            },
            wardrobe: Wardrobe {
                bodies: vec!["a.png".to_owned(), "b.png".to_owned()],
                outfits: vec!["c.png".to_owned(), "d.png".to_owned(), "e.png".to_owned()],
                hair: vec!["f.png".to_owned(), "g.png".to_owned()],
                hair_colors: vec![(0.1, 0.1, 0.1), (0.9, 0.6, 0.2)],
            },
        }
    }

    #[test]
    fn a_player_always_gets_the_same_look() {
        let characters = characters();
        assert_eq!(characters.look_for(42), characters.look_for(42));
    }

    #[test]
    fn players_are_dressed_from_the_whole_wardrobe() {
        let characters = characters();
        let looks: Vec<Look> = (0..200).map(|seed| characters.look_for(seed)).collect();
        let outfits: HashSet<&str> = looks.iter().map(|look| look.outfit.as_str()).collect();
        let bodies: HashSet<&str> = looks.iter().map(|look| look.body.as_str()).collect();
        assert_eq!(outfits.len(), 3);
        assert_eq!(bodies.len(), 2);
    }

    #[test]
    fn a_box_painted_beyond_the_skin_is_refused() {
        let mut characters = characters();
        characters.limbs.head.boxes[0].uv = (10, 0);
        assert!(characters.validate().is_err());
        characters.limbs.head.boxes[0].uv = (0, 8);
        assert!(characters.validate().is_ok());
    }

    #[test]
    fn an_expression_painted_beyond_the_skin_is_refused() {
        let mut characters = characters();
        characters.expressions.smile = (13, 8);
        assert!(characters.validate().is_err());
    }
}
