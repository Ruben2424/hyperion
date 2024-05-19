//! All systems in the ECS framework.

#![allow(clippy::missing_docs_in_private_items, reason = "self-explanatory")]

mod block_update;
mod chat_message;
pub mod chunks;
mod compass;
mod despawn_player;
mod disguise_player;
pub mod effect;
mod egress;
mod entity_detect_collisions;
mod entity_move_logic;
pub mod equipment;
mod generate_egress_packets;
pub mod ingress;
mod init_entity;
mod init_player;
mod inventory_systems;
mod keep_alive;
mod kill_all;
mod pkt_attack;
mod pkt_hand_swing;
mod player_detect_mob_hits;
mod player_join_world;
mod player_kick;
mod pose_update;
mod rebuild_player_location;
mod recalculate_bounding_boxes;
mod set_player_skin;
mod shoved_reaction;
mod speed;
mod stats_message;
mod sync_entity_position;
mod sync_players;
mod teleport;
mod time;
mod update_equipment;
mod update_health;
mod voice_chat;

pub use block_update::block_update;
pub use chat_message::chat_message;
pub use compass::compass;
pub use despawn_player::despawn_player;
pub use disguise_player::disguise_player;
pub use egress::egress;
pub use entity_detect_collisions::entity_detect_collisions;
pub use entity_move_logic::entity_move_logic;
pub use generate_egress_packets::generate_egress_packets;
pub use ingress::generate_ingress_events;
pub use init_entity::init_entity;
pub use init_player::init_player;
pub use inventory_systems::{get_inventory_actions, give_command};
pub use keep_alive::keep_alive;
pub use kill_all::kill_all;
pub use pkt_attack::{check_immunity, pkt_attack_entity, pkt_attack_player};
pub use pkt_hand_swing::pkt_hand_swing;
pub use player_detect_mob_hits::player_detect_mob_hits;
pub use player_join_world::{generate_biome_registry, player_join_world, send_player_info};
pub use player_kick::player_kick;
pub use pose_update::pose_update;
pub use rebuild_player_location::rebuild_player_location;
pub use recalculate_bounding_boxes::recalculate_bounding_boxes;
pub use set_player_skin::set_player_skin;
pub use shoved_reaction::shoved_reaction;
pub use stats_message::stats_message;
pub use sync_entity_position::sync_entity_position;
pub use sync_players::sync_players;
pub use teleport::teleport;
pub use time::{send_time, update_time};
pub use update_equipment::{update_equipment, update_main_hand};
pub use update_health::update_health;
