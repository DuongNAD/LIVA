//! Geometry, DPI scaling, and coordinate normalization primitives.

pub mod dpi;
pub mod lparam;
pub mod virtualdesk;

pub use dpi::{
    dpi_scale_factor, get_dpi_for_system, get_dpi_for_window, init_per_monitor_v2_dpi_awareness,
    logical_to_physical_pt, physical_to_logical_pt, rescale_dimension, STANDARD_DPI,
};
pub use lparam::{
    pack_lparam, pack_lparam_clamped, pack_xy, unpack_x, unpack_xy, unpack_y, LParamPackError,
};
pub use virtualdesk::{from_virtualdesk_absolute, to_virtualdesk_absolute, VirtualDesktopMetrics};
