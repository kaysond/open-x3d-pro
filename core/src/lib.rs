//! Shared core of Open X3D Pro: the profile/config types (CONTRACT §3), the float
//! reference pipeline and config blob (§4), the HID report descriptor (§2.1) and
//! the test vector generator the C driver is checked against (§4.3).

pub mod blob;
pub mod config;
pub mod hid;
pub mod pipeline;
pub mod vectors;
