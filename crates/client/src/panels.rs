//! Windows that take the cursor away from the world, such as the backpack
//! and shops. At most one is open at a time; Escape closes it.

use bevy::prelude::*;

pub(crate) struct PanelsPlugin;

impl Plugin for PanelsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OpenPanel>()
            .add_systems(PreUpdate, close_on_escape);
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
}

impl OpenPanel {
    pub(crate) fn is_open(self) -> bool {
        self != Self::None
    }
}

fn close_on_escape(keys: Res<ButtonInput<KeyCode>>, mut panel: ResMut<OpenPanel>) {
    if keys.just_pressed(KeyCode::Escape) && panel.is_open() {
        *panel = OpenPanel::None;
    }
}
