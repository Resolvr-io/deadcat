pub mod contract;
pub mod params;
#[cfg(any(test, feature = "testing"))]
pub mod pset;
#[cfg(not(any(test, feature = "testing")))]
pub(crate) mod pset;
#[cfg(any(test, feature = "testing"))]
pub mod taproot;
#[cfg(not(any(test, feature = "testing")))]
pub(crate) mod taproot;
#[cfg(any(test, feature = "testing"))]
pub mod witness;
#[cfg(not(any(test, feature = "testing")))]
pub(crate) mod witness;
