//! Scenery on screen: each prop the server scattered, drawn with its model.

use bevy::prelude::*;
use messoria_shared::{content::Content, protocol::Prop};

use crate::art::Models;

pub(crate) struct SceneryPlugin;

impl Plugin for SceneryPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(show_prop);
    }
}

fn show_prop(
    trigger: On<Add, Prop>,
    props: Query<&Prop>,
    content: Res<Content>,
    models: Res<Models>,
    mut commands: Commands,
) {
    let Ok(prop) = props.get(trigger.entity) else {
        return;
    };
    let definition = content.prop(prop.kind);
    let model = definition
        .models
        .get(usize::from(prop.model))
        .or_else(|| definition.models.first());
    let Some(scene) = model.and_then(|model| models.scene(model)) else {
        return;
    };
    commands.entity(trigger.entity).insert((
        Name::new(definition.name.clone()),
        Transform::from_translation(prop.position)
            .with_rotation(Quat::from_rotation_y(prop.turn))
            .with_scale(Vec3::splat(prop.scale)),
        scene,
    ));
}
