use anyhow::{Context, Result};
use frame_core::ControllerPhase;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControllerStateSnapshot {
    pub generation: u64,
    pub phase: ControllerPhase,
}

struct ControllerState {
    generation: u64,
    phase: ControllerPhase,
}

impl Default for ControllerState {
    fn default() -> Self {
        Self {
            generation: 0,
            phase: ControllerPhase::NotStarted,
        }
    }
}

fn controller_state() -> &'static Mutex<ControllerState> {
    static CONTROLLER_STATE: OnceLock<Mutex<ControllerState>> = OnceLock::new();
    CONTROLLER_STATE.get_or_init(|| Mutex::new(ControllerState::default()))
}

pub fn set_controller_phase(phase: ControllerPhase) -> Result<()> {
    let mut state = controller_state()
        .lock()
        .map_err(|_| anyhow::anyhow!("controller state lock poisoned"))
        .context("failed to update controller state")?;
    state.generation = state.generation.wrapping_add(1);
    state.phase = phase;
    Ok(())
}

pub fn controller_state_snapshot() -> Result<ControllerStateSnapshot> {
    let state = controller_state()
        .lock()
        .map_err(|_| anyhow::anyhow!("controller state lock poisoned"))
        .context("failed to read controller state")?;
    Ok(ControllerStateSnapshot {
        generation: state.generation,
        phase: state.phase.clone(),
    })
}
