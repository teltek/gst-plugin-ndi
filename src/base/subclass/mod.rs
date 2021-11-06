// Take a look at the license at the top of the repository in the LICENSE file.

#![allow(clippy::cast_ptr_alignment)]


mod aggregator;
mod aggregator_pad;

pub mod prelude {
    #[doc(hidden)]
    pub use gst::subclass::prelude::*;

    pub use super::aggregator::{AggregatorImpl, AggregatorImplExt};
    pub use super::aggregator_pad::{AggregatorPadImpl, AggregatorPadImplExt};
}
