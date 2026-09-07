//! Sovereign 2D Desktop Shell and Compositor for antOS bare-metal graphics.
//!
//! Provides coordinate geometry, color palettes, double-buffered surface rendering,
//! floating modal Intent HUD, terminal window components, and desktop composition.

pub mod color;
pub mod compositor;
pub mod cursor;
pub mod hud;
pub mod rect;
pub mod statusbar;
pub mod surface;
pub mod terminal_window;

pub use color::{palette, Color};
pub use compositor::{dispatch_pending_inputs, init, render_desktop, DesktopCompositor, FocusTarget, COMPOSITOR};
pub use cursor::MouseCursor;
pub use hud::IntentHud;
pub use rect::{Point, Rect, Size};
pub use statusbar::StatusBar;
pub use surface::Surface;
pub use terminal_window::TerminalWindow;
