//! Windows that take the cursor away from the world, such as the backpack,
//! shops, chests and the game menu. At most one is open at a time.
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
    /// The game menu, as opened with Escape.
    Menu,
    Options,
}

impl OpenPanel {
    pub(crate) fn is_open(self) -> bool {
        self != Self::None
    }
}

fn answer_escape(keys: Res<ButtonInput<KeyCode>>, mut panel: ResMut<OpenPanel>) {
    if keys.just_pressed(KeyCode::Escape) {
        *panel = match *panel {
            OpenPanel::None | OpenPanel::Options => OpenPanel::Menu,
            OpenPanel::Backpack | OpenPanel::Shop(_) | OpenPanel::Chest(_) | OpenPanel::Menu => {
                OpenPanel::None
            }
        };
    }
}
