//! Bursts of small bits where things happen: soil flying from a shovel,
//! chips from an axe, water splashing, leaves shaken off a harvest.
//!
//! Particles are tiny cubes thrown upward and pulled down again, shrinking
//! away over their short lives. They are only for show and never touch the
//! simulation.

use std::collections::HashMap;

use bevy::{light::NotShadowCaster, prelude::*};
use messoria_content::Tool;
use messoria_shared::protocol::Happened;
use messoria_voxel::Material;

use crate::feedback::Witnessed;

const GRAVITY: f32 = 9.0;

pub(crate) struct ParticlesPlugin;

impl Plugin for ParticlesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, prepare_particles)
            .add_systems(Update, (burst, fly).chain());
    }
}

/// A kind of burst and how it looks.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Burst {
    Soil,
    Stone,
    Chips,
    Sand,
    Water,
    Leaves,
    Coins,
    Fertilizer,
}

struct Style {
    color: Color,
    count: u32,
    /// Size of each bit, in meters.
    size: f32,
    /// Upward and sideways speed, in meters per second.
    rise: f32,
    spread: f32,
    /// Seconds each bit lasts.
    life: f32,
}

impl Burst {
    const ALL: [Self; 8] = [
        Self::Soil,
        Self::Stone,
        Self::Chips,
        Self::Sand,
        Self::Water,
        Self::Leaves,
        Self::Coins,
        Self::Fertilizer,
    ];

    fn of(happened: Happened) -> Option<Self> {
        match happened {
            // Grass is a thin layer; what flies off the shovel is soil.
            Happened::Dug(Material::Grass | Material::Soil)
            | Happened::Raised
            | Happened::Tilled => Some(Self::Soil),
            Happened::Dug(Material::Stone) | Happened::Struck(Tool::Pickaxe) => Some(Self::Stone),
            Happened::Struck(Tool::Axe) => Some(Self::Chips),
            Happened::Dug(Material::Sand) => Some(Self::Sand),
            Happened::Watered => Some(Self::Water),
            Happened::Harvested => Some(Self::Leaves),
            Happened::Traded => Some(Self::Coins),
            Happened::Fertilized | Happened::Planted => Some(Self::Fertilizer),
            Happened::Struck(Tool::Shovel | Tool::Hoe | Tool::WateringCan) | Happened::Ate => None,
        }
    }

    fn style(self) -> Style {
        let earth = |color| Style {
            color,
            count: 16,
            size: 0.09,
            rise: 3.2,
            spread: 1.4,
            life: 0.7,
        };
        match self {
            Self::Soil => earth(Color::srgb(0.36, 0.25, 0.16)),
            Self::Stone => earth(Color::srgb(0.52, 0.52, 0.5)),
            Self::Chips => Style {
                color: Color::srgb(0.78, 0.62, 0.4),
                count: 10,
                size: 0.07,
                rise: 2.4,
                spread: 1.8,
                life: 0.6,
            },
            Self::Sand => earth(Color::srgb(0.82, 0.73, 0.52)),
            Self::Water => Style {
                color: Color::srgb(0.45, 0.68, 0.95),
                count: 18,
                size: 0.045,
                rise: 1.6,
                spread: 1.1,
                life: 0.55,
            },
            Self::Leaves => Style {
                color: Color::srgb(0.36, 0.62, 0.22),
                count: 12,
                size: 0.08,
                rise: 2.6,
                spread: 1.2,
                life: 0.9,
            },
            Self::Coins => Style {
                color: Color::srgb(0.98, 0.8, 0.3),
                count: 8,
                size: 0.06,
                rise: 2.4,
                spread: 0.7,
                life: 0.6,
            },
            Self::Fertilizer => Style {
                color: Color::srgb(0.25, 0.18, 0.12),
                count: 8,
                size: 0.05,
                rise: 1.4,
                spread: 0.8,
                life: 0.5,
            },
        }
    }
}

#[derive(Resource)]
struct ParticleAssets {
    cube: Handle<Mesh>,
    materials: HashMap<Burst, Handle<StandardMaterial>>,
}

#[derive(Component)]
struct Particle {
    velocity: Vec3,
    size: f32,
    life: f32,
    age: f32,
}

fn prepare_particles(
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let materials = Burst::ALL
        .into_iter()
        .map(|burst| {
            let material = materials.add(StandardMaterial {
                base_color: burst.style().color,
                perceptual_roughness: 0.9,
                ..default()
            });
            (burst, material)
        })
        .collect();
    commands.insert_resource(ParticleAssets {
        cube: meshes.add(Cuboid::from_length(1.0)),
        materials,
    });
}

fn burst(
    assets: Res<ParticleAssets>,
    mut witnessed: MessageReader<Witnessed>,
    mut commands: Commands,
) {
    for Witnessed(happening) in witnessed.read() {
        let Some(burst) = Burst::of(happening.what) else {
            continue;
        };
        let style = burst.style();
        for _ in 0..style.count {
            let angle = rand::random_range(0.0..std::f32::consts::TAU);
            let outward = Vec2::from_angle(angle) * style.spread * rand::random_range(0.3..=1.0);
            let velocity = Vec3::new(
                outward.x,
                style.rise * rand::random_range(0.6..=1.0),
                outward.y,
            );
            commands.spawn((
                Particle {
                    velocity,
                    size: style.size * rand::random_range(0.7..=1.3),
                    life: style.life * rand::random_range(0.7..=1.0),
                    age: 0.0,
                },
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.materials[&burst].clone()),
                Transform::from_translation(happening.at + Vec3::Y * 0.1)
                    .with_rotation(Quat::from_rotation_y(angle))
                    .with_scale(Vec3::splat(style.size)),
                NotShadowCaster,
            ));
        }
    }
}

fn fly(
    time: Res<Time>,
    mut particles: Query<(Entity, &mut Particle, &mut Transform)>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    for (entity, mut particle, mut transform) in &mut particles {
        particle.age += dt;
        if particle.age >= particle.life {
            commands.entity(entity).despawn();
            continue;
        }
        particle.velocity.y -= GRAVITY * dt;
        transform.translation += particle.velocity * dt;
        transform.rotate_local_x(6.0 * dt);
        let left = 1.0 - particle.age / particle.life;
        // Full size for most of the life, then shrinking away.
        transform.scale = Vec3::splat(particle.size * (left * 3.0).min(1.0));
    }
}
