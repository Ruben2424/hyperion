//! Hyperion

#![feature(split_at_checked)]
#![feature(type_alias_impl_trait)]
#![feature(lint_reasons)]
#![feature(io_error_more)]
#![expect(clippy::type_complexity, reason = "evenio uses a lot of complex types")]

mod chunk;
mod singleton;

use std::{
    collections::VecDeque,
    net::ToSocketAddrs,
    sync::{atomic::AtomicU32, Arc},
    time::{Duration, Instant},
};

use anyhow::Context;
use evenio::prelude::*;
use libc::{getrlimit, setrlimit, RLIMIT_NOFILE};
use ndarray::s;
use signal_hook::iterator::Signals;
use singleton::bounding_box;
use spin::Lazy;
use tracing::{debug, error, info, instrument, trace, warn};
use valence_protocol::CompressionThreshold;

use crate::{
    components::Vitals,
    events::{Egress, Gametick, StatsEvent},
    global::Global,
    net::{Server, ServerDef},
    singleton::{
        broadcast::BroadcastBuf, buffer_allocator::BufferAllocator, fd_lookup::FdLookup,
        player_aabb_lookup::PlayerBoundingBoxes, player_id_lookup::PlayerIdLookup,
        player_uuid_lookup::PlayerUuidLookup,
    },
};

mod components;
mod events;

mod global;
mod net;

mod packets;
mod system;

mod bits;

mod tracker;

mod config;

/// History size for sliding average.
const MSPT_HISTORY_SIZE: usize = 100;

/// on macOS, the soft limit for the number of open file descriptors is often 256. This is far too low
/// to test 10k players with.
/// This attempts to the specified `recommended_min` value.
pub fn adjust_file_limits(recommended_min: u64) -> std::io::Result<()> {
    let mut limits = libc::rlimit {
        rlim_cur: 0, // Initialize soft limit to 0
        rlim_max: 0, // Initialize hard limit to 0
    };

    if unsafe { getrlimit(RLIMIT_NOFILE, &mut limits) } == 0 {
        info!("Current file handle soft limit: {}", limits.rlim_cur);
        info!("Current file handle hard limit: {}", limits.rlim_max);
    } else {
        error!("Failed to get the current file handle limits");
        return Err(std::io::Error::last_os_error());
    };

    if limits.rlim_max < recommended_min {
        warn!(
            "Could only set file handle limit to {}. Recommended minimum is {}",
            limits.rlim_cur, recommended_min
        );
    }

    limits.rlim_cur = limits.rlim_max;
    info!("Setting soft limit to: {}", limits.rlim_cur);

    if unsafe { setrlimit(RLIMIT_NOFILE, &limits) } != 0 {
        error!("Failed to set the file handle limits");
        return Err(std::io::Error::last_os_error());
    }

    Ok(())
}

/// The central [`Game`] struct which owns and manages the entire server.
pub struct Game {
    /// The shared state between the ECS framework and the I/O thread.
    shared: Arc<global::Shared>,
    /// The manager of the ECS framework.
    world: World,
    /// Data for what time the last ticks occurred.
    last_ticks: VecDeque<Instant>,
    /// Data for how many milliseconds previous ticks took.
    last_ms_per_tick: VecDeque<f64>,
    /// The tick of the game. This is incremented every 50 ms.
    tick_on: u64,
}

impl Game {
    /// Get the [`World`] which is the core part of the ECS framework.
    pub const fn world(&self) -> &World {
        &self.world
    }

    /// Get all shared data that is shared between the ECS framework and the IO thread.
    pub const fn shared(&self) -> &Arc<global::Shared> {
        &self.shared
    }

    /// See [`Game::world`].
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// # Panics
    /// This function will panic if the game is already shutdown.
    pub const fn shutdown(&self) {
        // TODO
    }

    /// Initialize the server.
    pub fn init(address: impl ToSocketAddrs + Send + Sync + 'static) -> anyhow::Result<Self> {
        info!("Starting hyperion");
        Lazy::force(&config::CONFIG);

        let current_threads = rayon::current_num_threads();
        let max_threads = rayon::max_num_threads();

        info!("rayon: current threads: {current_threads}, max threads: {max_threads}");

        let mut signals = Signals::new([signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM])
            .context("failed to create signal handler")?;

        std::thread::spawn({
            move || {
                for _ in signals.forever() {
                    warn!("Shutting down...");
                    SHUTDOWN.store(true, std::sync::atomic::Ordering::Relaxed);
                    // TODO:
                }
            }
        });

        let shared = Arc::new(global::Shared {
            player_count: AtomicU32::new(0),
            compression_level: CompressionThreshold(256),
        });

        let mut world = World::new();

        let server = world.spawn();
        let mut server_def = Server::new(address)?;

        let buffer_alloc = world.spawn();
        world.insert(buffer_alloc, BufferAllocator::new(&mut server_def));
        world.insert(server, server_def);

        world.add_handler(system::ingress);
        world.add_handler(system::init_player);
        world.add_handler(system::player_join_world);
        world.add_handler(system::player_kick);
        world.add_handler(system::init_entity);
        world.add_handler(system::entity_move_logic);
        world.add_handler(system::entity_detect_collisions);
        world.add_handler(system::sync_entity_position);
        world.add_handler(system::reset_bounding_boxes);
        world.add_handler(system::update_time);
        world.add_handler(system::update_health);
        world.add_handler(system::sync_players);
        world.add_handler(system::rebuild_player_location);
        world.add_handler(system::player_detect_mob_hits);

        world.add_handler(system::pkt_attack);
        world.add_handler(system::pkt_hand_swing);

        world.add_handler(system::generate_egress_packets);

        world.add_handler(system::egress);

        world.add_handler(system::keep_alive);
        world.add_handler(system::stats_message);
        world.add_handler(system::kill_all);

        let global = world.spawn();
        world.insert(global, Global::new(shared.clone()));

        let bounding_boxes = world.spawn();
        world.insert(bounding_boxes, bounding_box::EntityBoundingBoxes::default());

        let uuid_lookup = world.spawn();
        world.insert(uuid_lookup, PlayerUuidLookup::default());

        let player_id_lookup = world.spawn();
        world.insert(player_id_lookup, PlayerIdLookup::default());

        let player_location_lookup = world.spawn();
        world.insert(player_location_lookup, PlayerBoundingBoxes::default());

        let fd_lookup = world.spawn();
        world.insert(fd_lookup, FdLookup::default());

        let encoder = world.spawn();
        world.insert(encoder, BroadcastBuf::new(shared.compression_level));

        let mut game = Self {
            shared,
            world,
            last_ticks: VecDeque::default(),
            last_ms_per_tick: VecDeque::default(),
            tick_on: 0,
        };

        game.last_ticks.push_back(Instant::now());

        Ok(game)
    }

    /// The duration to wait between ticks.
    fn wait_duration(&self) -> Option<Duration> {
        let &first_tick = self.last_ticks.front()?;

        let count = self.last_ticks.len();

        #[expect(clippy::cast_precision_loss, reason = "count is limited to 100")]
        let time_for_20_tps = { first_tick + Duration::from_secs_f64(count as f64 / 20.0) };

        // aim for 20 ticks per second
        let now = Instant::now();

        if time_for_20_tps < now {
            warn!("tick took full 50ms; skipping sleep");
            return None;
        }

        let duration = time_for_20_tps - now;
        let duration = duration.mul_f64(0.8);

        if duration.as_millis() > 47 {
            trace!("duration is long");
            return Some(Duration::from_millis(47));
        }

        // this is a bit of a hack to be conservative when sleeping
        Some(duration)
    }

    /// Run the main game loop at 20 ticks per second.
    pub fn game_loop(&mut self) {
        while !SHUTDOWN.load(std::sync::atomic::Ordering::Relaxed) {
            self.tick();

            if let Some(wait_duration) = self.wait_duration() {
                spin_sleep::sleep(wait_duration);
            }
        }
    }

    /// Run one tick of the game loop.
    #[instrument(skip(self), fields(tick_on = self.tick_on))]
    pub fn tick(&mut self) {
        /// The length of history to keep in the moving average.
        const LAST_TICK_HISTORY_SIZE: usize = 100;

        let now = Instant::now();

        // let mut tps = None;
        if self.last_ticks.len() > LAST_TICK_HISTORY_SIZE {
            let last = self.last_ticks.back().unwrap();

            let ms = last.elapsed().as_nanos() as f64 / 1_000_000.0;
            if ms > 60.0 {
                warn!("tick took too long: {ms}ms");
            }

            self.last_ticks.pop_front().unwrap();
        }

        self.last_ticks.push_back(now);

        self.world.send(Gametick);
        self.world.send(Egress);

        #[expect(
            clippy::cast_precision_loss,
            reason = "realistically, nanoseconds between last tick will not be greater than 2^52 \
                      (~52 days)"
        )]
        let ms = now.elapsed().as_nanos() as f64 / 1_000_000.0;
        self.update_tick_stats(ms);
        // info!("Tick took: {:02.8}ms", ms);
    }

    #[instrument(skip(self))]
    fn update_tick_stats(&mut self, ms: f64) {
        self.last_ms_per_tick.push_back(ms);

        if self.last_ms_per_tick.len() > MSPT_HISTORY_SIZE {
            // efficient
            let arr = ndarray::Array::from_iter(self.last_ms_per_tick.iter().copied().rev());

            // last 1 second (20 ticks) 5 seconds (100 ticks) and 25 seconds (500 ticks)
            let mean_1_second = arr.slice(s![..20]).mean().unwrap();
            let mean_5_seconds = arr.slice(s![..100]).mean().unwrap();

            debug!("ms / tick: {mean_1_second:.2}ms");

            self.world.send(StatsEvent {
                ms_per_tick_mean_1s: mean_1_second,
                ms_per_tick_mean_5s: mean_5_seconds,
            });

            self.last_ms_per_tick.pop_front();
        }

        self.tick_on += 1;
    }
}

// todo: remove static and make this an `Arc` to prevent weird behavior with multiple `Game`s
/// A shutdown atomic which is used to shut down the [`Game`] gracefully.
static SHUTDOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
