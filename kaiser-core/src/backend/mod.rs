#[cfg(target_os = "windows")]
mod apply;
#[cfg(target_os = "windows")]
mod enumerate;
#[cfg(target_os = "windows")]
mod topology;
#[cfg(target_os = "windows")]
mod win32_types;

#[cfg(target_os = "windows")]
pub use topology::{KaiserBackend, SharedKaiserBackend};
#[cfg(target_os = "windows")]
pub use apply::{apply_display_rotation, apply_clone_source};

#[cfg(not(target_os = "windows"))]
#[derive(Debug, Clone, Default)]
pub struct KaiserBackend;

#[cfg(not(target_os = "windows"))]
impl monarch::DisplayBackend for KaiserBackend {
    fn list_displays(&self) -> Result<Vec<monarch::DisplayInfo>, monarch::ManagerError> {
        Err(monarch::ManagerError::Backend("not supported on this platform".to_string()))
    }
    fn get_layout(&self) -> Result<monarch::Layout, monarch::ManagerError> {
        Err(monarch::ManagerError::Backend("not supported on this platform".to_string()))
    }
    fn apply_layout(&self, _layout: monarch::Layout) -> Result<(), monarch::ManagerError> {
        Err(monarch::ManagerError::Backend("not supported on this platform".to_string()))
    }
}
