#[allow(clippy::unreadable_literal)]
#[allow(clippy::too_many_arguments)]
#[allow(clippy::match_same_arms)]
#[allow(clippy::type_complexity)]
mod auto;
pub use auto::*;

mod utils;

mod aggregator;
mod aggregator_pad;

pub mod prelude {
    pub use gst::glib::prelude::*;
    pub use gst::prelude::*;

    pub use super::aggregator::AggregatorExtManual;
    pub use super::aggregator_pad::AggregatorPadExtManual;
    pub use super::auto::traits::*;
}

pub mod subclass;

mod ffi;

pub const AGGREGATOR_FLOW_NEED_DATA: gst::FlowError = gst::FlowError::CustomError;
