use crate::model::Model;
use crate::state::NetworkState;
use crate::timestep::Timestep;
use crate::PywrError;

pub mod clp;

pub trait Solver {
    fn setup(&mut self, model: &Model) -> Result<(), PywrError>;
    fn solve(
        &mut self,
        model: &Model,
        timestep: &Timestep,
        network_state: &NetworkState,
        parameter_state: &[f64],
    ) -> Result<NetworkState, PywrError>;
}
