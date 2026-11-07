//! Bots introduce themselves like players do: with a name and a look
//! picked from the wardrobe, when the server says they are new.

use bevy::prelude::*;
use lightyear::prelude::*;
use messoria_shared::{
    content::Content,
    protocol::{ActionChannel, Introduce, Newcomer},
};

pub(crate) struct IntroducePlugin {
    pub name: String,
    pub seed: u64,
}

impl Plugin for IntroducePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Identity {
            name: self.name.clone(),
            seed: self.seed,
        })
        .add_systems(PreUpdate, introduce.after(MessageSystems::Receive));
    }
}

#[derive(Resource)]
struct Identity {
    name: String,
    seed: u64,
}

fn introduce(
    content: Res<Content>,
    identity: Res<Identity>,
    mut newcomers: Query<&mut MessageReceiver<Newcomer>, With<Client>>,
    mut senders: Query<&mut MessageSender<Introduce>, With<Client>>,
) {
    for mut receiver in &mut newcomers {
        for Newcomer in receiver.receive() {
            if let Ok(mut sender) = senders.single_mut() {
                sender.send::<ActionChannel>(Introduce {
                    name: identity.name.clone(),
                    look: content.characters().look_for(identity.seed),
                });
            }
        }
    }
}
