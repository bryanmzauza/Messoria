//! Windows that take the cursor away from the world, such as the backpack,
//! shops, chests, the notice board and the game menu. At most one is open
//! at a time.
//!
//! Escape closes the open window, or opens the game menu when none is; from
//! the options it goes back to the menu.

use bevy::prelude::*;

pub(crate) struct PanelsPlugin;

impl Plugin for PanelsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OpenPanel>()
            .add_systems(PreUpdate, answer_escape);
    }
}

/// The window that is open, if any.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpenPanel {
    #[default]
    None,
    Backpack,
    /// The shop whose stall is this entity.
    Shop(Entity),
    /// The chest that is this entity.
    Chest(Entity),
    /// The village notice board that is this entity.
    Board(Entity),
    /// A word with the villager that is this entity.
    Talk(Entity),
    /// The list of villagers.
    Villagers,
    /// The wardrobe that is this entity.
    Wardrobe(Entity),
    /// The character creator, on first arrival.
    Creator,
    /// The card of controls.
    Controls,
    /// The game menu, as opened with Escape.
    Menu,
    Options,
}

impl OpenPanel {
    pub(crate) fn is_open(self) -> bool {
        self != Self::None
    }

    /// Whether keys are being typed into the panel, so the game's own keys
    /// must wait.
    pub(crate) fn takes_typing(self) -> bool {
        self == Self::Creator
    }
}

fn answer_escape(keys: Res<ButtonInput<KeyCode>>, mut panel: ResMut<OpenPanel>) {
    if keys.just_pressed(KeyCode::Escape) {
        *panel = match *panel {
            OpenPanel::None | OpenPanel::Options => OpenPanel::Menu,
            // Nobody sets out without a name.
            OpenPanel::Creator => OpenPanel::Creator,
            OpenPanel::Backpack
            | OpenPanel::Shop(_)
            | OpenPanel::Chest(_)
            | OpenPanel::Board(_)
            | OpenPanel::Talk(_)
            | OpenPanel::Villagers
            | OpenPanel::Wardrobe(_)
            | OpenPanel::Controls
            | OpenPanel::Menu => OpenPanel::None,
        };
    }
}
