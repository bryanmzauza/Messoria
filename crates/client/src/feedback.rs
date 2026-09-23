//! What the server tells the player about actions: notices of why one did
//! nothing, shown above the hotbar, and happenings in the world, handed on
//! for sounds and particles.

use std::time::Duration;

use bevy::{ecs::message::Message, prelude::*};
use lightyear::prelude::*;
use messoria_shared::protocol::{Happening, Notice};

use crate::{inventory::HOTBAR_BOTTOM, ui::TEXT_COLOR};

/// How long a notice stays, the last part of it fading out.
const NOTICE_TIME: Duration = Duration::from_millis(3_000);
const NOTICE_FADE: Duration = Duration::from_millis(600);
const NOTICE_BACKGROUND: Color = Color::srgba(0.35, 0.08, 0.05, 0.8);
/// Above the hotbar and the sleep status.
const NOTICE_BOTTOM: f32 = HOTBAR_BOTTOM + 130.0;

pub(crate) struct FeedbackPlugin;

impl Plugin for FeedbackPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Told>()
            .add_message::<Witnessed>()
            .add_systems(Startup, spawn_notice)
            .add_systems(PreUpdate, receive_feedback.after(MessageSystems::Receive))
            .add_systems(Update, show_notice);
    }
}

/// The server told the local player why something did not happen.
#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct Told(pub Notice);

/// Something happened in the world.
#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct Witnessed(pub Happening);

/// The notice on screen, and when it was shown.
#[derive(Component, Default)]
struct NoticeView {
    shown_at: Option<Duration>,
}

#[derive(Component)]
struct NoticeText;

fn spawn_notice(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Notice"),
            Node {
                position_type: PositionType::Absolute,
                bottom: px(NOTICE_BOTTOM),
                width: percent(100),
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_child((
            NoticeView::default(),
            Node {
                padding: UiRect::axes(px(14), px(6)),
                border_radius: BorderRadius::all(px(6)),
                ..default()
            },
            BackgroundColor(NOTICE_BACKGROUND),
            Visibility::Hidden,
            children![(
                NoticeText,
                Text::new(""),
                TextFont::from_font_size(16.0),
                TextColor(TEXT_COLOR),
            )],
        ));
}

/// The lightyear receivers appear with the first message of their kind.
fn receive_feedback(
    mut notices: Query<&mut MessageReceiver<Notice>, With<Client>>,
    mut happenings: Query<&mut MessageReceiver<Happening>, With<Client>>,
    mut told: MessageWriter<Told>,
    mut witnessed: MessageWriter<Witnessed>,
) {
    for mut receiver in &mut notices {
        told.write_batch(receiver.receive().map(Told));
    }
    for mut receiver in &mut happenings {
        witnessed.write_batch(receiver.receive().map(Witnessed));
    }
}

fn show_notice(
    time: Res<Time>,
    mut told: MessageReader<Told>,
    view: Single<(&mut NoticeView, &mut Visibility, &mut BackgroundColor)>,
    mut text: Single<(&mut Text, &mut TextColor), With<NoticeText>>,
) {
    let (mut notice, mut visibility, mut background) = view.into_inner();
    let now = time.elapsed();
    if let Some(Told(latest)) = told.read().last() {
        text.0.0 = latest.to_string();
        notice.shown_at = Some(now);
    }
    let Some(shown_at) = notice.shown_at else {
        return;
    };
    let left = NOTICE_TIME.saturating_sub(now.saturating_sub(shown_at));
    if left.is_zero() {
        notice.shown_at = None;
        *visibility = Visibility::Hidden;
        return;
    }
    let opacity = (left.as_secs_f32() / NOTICE_FADE.as_secs_f32()).min(1.0);
    *visibility = Visibility::Inherited;
    background.0 = NOTICE_BACKGROUND.with_alpha(NOTICE_BACKGROUND.alpha() * opacity);
    text.1.0 = TEXT_COLOR.with_alpha(opacity);
}
