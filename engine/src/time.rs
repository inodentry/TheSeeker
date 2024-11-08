use bevy::ecs::schedule::ScheduleLabel;
use bevy::ecs::system::SystemState;

use crate::prelude::*;

pub struct GameTimePlugin;

impl Plugin for GameTimePlugin {
    fn build(&self, app: &mut App) {
        app.init_schedule(GameTickUpdate);
        app.init_schedule(GameTickPost);
        app.init_resource::<GameTime>();
        app.add_systems(Update, run_gametickupdate_schedule);
        app.configure_sets(
            Update,
            GameTickSet::Pre.before(run_gametickupdate_schedule),
        );
        app.configure_sets(
            Update,
            GameTickSet::Post.after(run_gametickupdate_schedule),
        );
    }
}

/// Apply this to anything that relies on `GameTime`
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameTickSet {
    /// Runs after `GameTime` is updated, but before the `GameTickUpdate` schedule
    Pre,
    /// Runs after the `GameTickUpdate` schedule
    Post,
}

/// This is when old "game tick events" are cleared (in `GameTickUpdate` schedule)
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameTickEventClearSet;

pub trait GameTimeAppExt {
    fn add_gametick_event<T: Event>(&mut self) -> &mut Self;
}

impl GameTimeAppExt for App {
    fn add_gametick_event<T: Event>(&mut self) -> &mut Self {
        if !self.world.contains_resource::<Events<T>>() {
            self.init_resource::<Events<T>>();
            self.add_systems(
                GameTickUpdate,
                minimal_event_update_system::<T>.in_set(GameTickEventClearSet),
            );
        } else {
            warn!("Attempted to add a Game Tick event type that had already been added as an event before!");
        }
        self
    }
}

fn minimal_event_update_system<T: Event>(mut events: ResMut<Events<T>>) {
    events.update();
}

/// Our alternative to `FixedUpdate`
#[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameTickUpdate;

/// Run after `GameTickUpdate`
/// used for running systems which update input state between GameTickUpdate runs
#[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameTickPost;

#[derive(Resource, Debug)]
pub struct GameTime {
    /// The time base rate
    step: Duration,
    tick: u64,
    overstep: f64,
    last_update: Duration,
    total_time: Duration,
}

impl Default for GameTime {
    fn default() -> Self {
        Self::new(96.0)
    }
}

impl GameTime {
    /// Create with a non-default tick rate
    pub fn new(hz: f64) -> Self {
        Self {
            step: Duration::from_secs_f64(1.0 / hz),
            tick: 0,
            overstep: 0.0,
            last_update: Duration::new(0, 0),
            total_time: Duration::new(0, 0),
        }
    }

    /// Get the current tick number to be simulated
    ///
    /// Increments with every run of the `GameTickUpdate` schedule
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Get the leftover partial tick to be carried over to the next Bevy frame update
    pub fn overstep(&self) -> f64 {
        self.overstep
    }

    /// Get the "elapsed" time when we last updated
    pub fn last_update(&self) -> Duration {
        self.last_update
    }

    /// Reset tick counters to zero, set the last update to now, keep the tickrate
    fn reset(&mut self, now: Duration) {
        *self = Self {
            step: self.step,
            last_update: now,
            tick: 0,
            overstep: 0.0,
            total_time: Duration::default(),
        };
    }

    pub fn time(&self) -> Duration {
        self.total_time
    }

    pub fn time_in_seconds(&self) -> f64 {
        self.total_time.as_secs_f64()
    }

    /// Convenience function to save you the math
    pub fn seconds_per_tick(&self) -> f64 {
        self.step.as_secs_f64()
    }

    /// Get the current tick rate
    pub fn hz(&self) -> f64 {
        1.0 / self.step.as_secs_f64()
    }
}

#[derive(Event)]
pub enum GameTickManageEvent {
    ResetTickCounter,
    SetDefaultStep,
    SetExactRate(f64),
    SetExactStep(Duration),
    SetTickRateDefaultFraction(f64),
}

#[derive(Event)]
pub struct GameTickResetEvent;

fn game_tick_manage_events(
    time: Res<Time>,
    mut gt: ResMut<GameTime>,
    mut evr_manage: EventReader<GameTickManageEvent>,
    mut evw_reset: EventWriter<GameTickResetEvent>,
    mut default_step: Local<Duration>,
) {
    if *default_step == Duration::default() {
        *default_step = gt.step;
    }

    for manage in evr_manage.read() {
        match manage {
            GameTickManageEvent::ResetTickCounter => {
                gt.reset(time.elapsed());
                evw_reset.send(GameTickResetEvent);
            },
            GameTickManageEvent::SetDefaultStep => {
                if gt.step != *default_step {
                    gt.step = *default_step;
                    evw_reset.send(GameTickResetEvent);
                }
            },
            GameTickManageEvent::SetExactStep(step) => {
                if gt.step != *step {
                    gt.step = *step;
                    evw_reset.send(GameTickResetEvent);
                }
            },
            GameTickManageEvent::SetExactRate(hz) => {
                let step = Duration::from_secs_f64(1.0 / *hz);
                if gt.step != step {
                    gt.step = step;
                    evw_reset.send(GameTickResetEvent);
                }
            },
            GameTickManageEvent::SetTickRateDefaultFraction(frac) => {
                let step =
                    Duration::from_secs_f64(default_step.as_secs_f64() / *frac);
                if gt.step != step {
                    gt.step = step;
                    evw_reset.send(GameTickResetEvent);
                }
            },
        }
    }
}

/// Our alternative to Bevy's fixed timestep, based on `GameTime`
pub fn run_gametickupdate_schedule(
    world: &mut World,
    ss: &mut SystemState<(Res<Time>, ResMut<GameTime>)>,
) {
    loop {
        {
            let (time, mut gametime) = ss.get_mut(world);
            if gametime.total_time + gametime.step > time.elapsed() {
                break;
            }
        }
        world.run_schedule(GameTickUpdate);
        world.run_schedule(GameTickPost);
        {
            let mut gametime = world.resource_mut::<GameTime>();
            let step = gametime.step;
            gametime.total_time += step;
            gametime.tick += 1;
        }
    }
    let (time, mut gametime) = ss.get_mut(world);
    gametime.last_update = time.elapsed();
}

/// Run condition to run something "every N ticks"
pub fn at_tick_multiples(quant: Quant) -> impl FnMut(Res<GameTime>) -> bool {
    move |gametime: Res<GameTime>| {
        (gametime.tick() + quant.offset as u64) % quant.n as u64 == 0
    }
}
