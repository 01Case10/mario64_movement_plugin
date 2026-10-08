//! Events emitted by [`crate::step::tick`] so game code can react without
//! touching simulation internals.

use crate::angles::Angle;
use crate::state::ActionId;

/// One observable thing that happened during a tick.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// The action changed this frame.
    ActionChanged { old: ActionId, new: ActionId },
    /// Touched the ground with the given downward speed.
    Landed { fall_speed: f32 },
    /// Hit a wall; carries the wall normal's yaw.
    WallHit { normal_yaw: Angle },
    /// Left the ground with an upward velocity.
    Jumped { velocity_y: f32 },
    /// Grabbed a ledge.
    LedgeGrab,
    /// Took fall damage; carries the damage amount.
    Damaged { amount: i32 },
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn events_are_comparable() {
        let a = Event::Landed { fall_speed: 10.0 };
        let b = Event::Landed { fall_speed: 10.0 };
        assert_eq!(a, b);
    }
}
