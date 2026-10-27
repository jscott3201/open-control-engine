//! BUILD and TICK tests for `oce-graph`.

mod build_schedule;
mod common;
mod edge_counter_feedthrough;
mod feedthrough_stateful;
mod integrator_loop_cut;
mod mixed_feedthrough_block_loop;
mod pid_feedthrough;
mod tick_eval;
mod topo_oracle;

use common::*;
