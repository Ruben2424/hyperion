use std::time::Instant;

use bvh::aabb::Aabb;
use derive_more::{Deref, Display, From};
use evenio::component::Component;
use glam::Vec3;

use crate::{
    components::vitals::{Absorption, Regeneration},
    global::Global,
};

pub mod pose;
pub mod vitals;

#[derive(Component, Deref, From, Display)]
pub struct InGameName(Box<str>);

#[derive(Component, Default)]
pub struct KeepAlive {
    pub last_sent: Option<Instant>,
    /// Set to true if a keep alive has been sent to the client and the client hasn't responded.
    pub unresponded: bool,
}

/// A component that represents a Player. In the future, this should be broken up into multiple components.
///
/// Why should it be broken up? The more things are broken up, the more we can take advantage of Rust borrowing rules.
#[derive(Component, Debug)]
pub struct Player;

#[derive(Component, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum LoginState {
    Handshake,
    Status,
    Login,
    TransitioningPlay,
    Play,
    Terminate,
}

#[derive(Component)]
pub struct HasInvincibility(pub bool);

#[derive(Copy, Clone, PartialEq, Debug, Component)]
pub enum Vitals {
    /// If the player is alive
    Alive {
        /// Measured in half hearts
        health: f32,

        /// The absorption effect
        absorption: Absorption,
        /// The regeneration effect
        regeneration: Regeneration,
    },
    /// If the player is dead
    Dead {
        /// The tick the player will be respawned
        respawn_tick: i64,
    },
}

impl Vitals {
    pub const ALIVE: Self = Self::Alive {
        health: 20.0,
        absorption: Absorption::DEFAULT,
        regeneration: Regeneration::DEFAULT,
    };
}

#[derive(Component, Debug, Eq, PartialEq, Default)]
pub struct ImmuneStatus {
    pub until: i64,
}

impl ImmuneStatus {
    pub const fn is_invincible(&self, global: &Global) -> bool {
        global.tick < self.until
    }
}

impl Vitals {
    /// Heal the player by a given amount.
    pub fn heal(&mut self, amount: f32) {
        assert!(amount.is_finite());
        assert!(amount > 0.0);

        let Self::Alive { health, .. } = self else {
            return;
        };

        *health += amount;
        *health = health.min(20.0);
    }

    /// Hurt the player by a given amount.
    pub fn hurt(&mut self, global: &Global, mut amount: f32, immune: &mut ImmuneStatus) {
        debug_assert!(amount.is_finite());
        debug_assert!(amount >= 0.0);

        let tick = global.tick;

        if tick < immune.until {
            return;
        }

        let max_hurt_resistant_time = global.max_hurt_resistant_time;

        immune.until = tick + i64::from(max_hurt_resistant_time) / 2;

        let Self::Alive {
            health, absorption, ..
        } = self
        else {
            return;
        };

        if tick < absorption.end_tick {
            if amount > absorption.bonus_health {
                amount -= absorption.bonus_health;
                absorption.bonus_health = 0.0;
            } else {
                absorption.bonus_health -= amount;
                return;
            }
        }

        *health -= amount;

        if *health <= 0.0 {
            *self = Self::Dead {
                respawn_tick: tick + 100,
            }
        }
    }
}

/// A UUID component. Generally speaking, this tends to be tied to entities with a [`Player`] component.
#[derive(Component, Copy, Clone, Debug, Deref, From)]
pub struct Uuid(pub uuid::Uuid);

/// Any living minecraft entity that is NOT a player.
///
/// Example: zombie, skeleton, etc.
#[derive(Component, Debug)]
pub struct MinecraftEntity;

/// The running multiplier of the entity. This defaults to 1.0.
#[derive(Component, Debug, Copy, Clone)]
pub struct RunningSpeed(pub f32);

impl Default for RunningSpeed {
    fn default() -> Self {
        Self(0.1)
    }
}

/// If the entity can be targeted by non-player entities.
#[derive(Component)]
pub struct AiTargetable;

/// The full pose of an entity. This is used for both [`Player`] and [`MinecraftEntity`].
#[derive(Component, Copy, Clone, Debug)]
pub struct FullEntityPose {
    /// The (x, y, z) position of the entity.
    /// Note we are using [`Vec3`] instead of [`glam::DVec3`] because *cache locality* is important.
    /// However, the Notchian server uses double precision floating point numbers for the position.
    pub position: Vec3,

    /// The yaw of the entity. (todo: probably need a separate component for head yaw, perhaps separate this out)
    pub yaw: f32,

    /// The pitch of the entity.
    pub pitch: f32,

    /// The bounding box of the entity.
    pub bounding: Aabb,
}

impl FullEntityPose {
    pub fn player() -> Self {
        let position = Vec3::new(0.0, 70.0, 0.0);

        Self {
            position,
            yaw: 0.0,
            pitch: 0.0,
            bounding: Aabb::create(position, 0.6, 1.8),
        }
    }
}

impl FullEntityPose {
    /// Move the pose by the given vector.
    pub fn move_by(&mut self, vec: Vec3) {
        self.position += vec;
        self.bounding = self.bounding.move_by(vec);
    }

    /// Teleport the pose to the given position.
    pub fn move_to(&mut self, pos: Vec3) {
        self.bounding = self.bounding.move_to(pos);
        self.position = pos;
    }
}

/// The reaction of an entity, in particular to collisions as calculated in `entity_detect_collisions`.
///
/// Why is this useful?
///
/// - We want to be able to detect collisions in parallel.
/// - Since we are accessing bounding boxes in parallel,
/// we need to be able to make sure the bounding boxes are immutable (unless we have something like a
/// [`Arc`] or [`std::sync::RwLock`], but this is not efficient).
/// - Therefore, we have an [`EntityReaction`] component which is used to store the reaction of an entity to collisions.
/// - Later we can apply the reaction to the entity's [`FullEntityPose`] to move the entity.
#[derive(Component, Default, Debug)]
pub struct EntityReaction {
    /// The velocity of the entity.
    pub velocity: Vec3,
}
