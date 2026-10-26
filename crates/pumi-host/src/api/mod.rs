pub mod files;
pub mod fs;
pub mod keymap;
pub mod menu;
pub mod options;
pub mod plugin;
pub mod registry;
pub mod requests;
pub mod select;
pub mod tab_call;
pub mod tabs;
pub mod views;
pub fn link() {
    #[cfg(feature = "core-system")]
    pumi_os::link();
}
use crate::kernel::Kernel;
use pumi_api::{Caller, Cx, Result};
pub(crate) fn kernel<'a>(cx: &'a mut Cx) -> Result<(Caller, &'a mut Kernel)> {
    let caller = cx.caller().clone();
    let k = cx.host::<Kernel>()?;
    k.sync();
    Ok((caller, k))
}
