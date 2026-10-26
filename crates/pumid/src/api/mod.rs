pub mod agents;
pub mod blocks;
pub mod events;
pub mod objects;
pub mod options;
pub mod requests;
pub mod server;
pub mod tabs;
pub mod traits;
pub mod ui;
pub mod wait;
pub fn link() {}
use crate::kernel::Kernel;
use pumi_api::{Caller, Cx, Result};
pub(crate) fn kernel<'a>(cx: &'a mut Cx) -> Result<(Caller, &'a mut Kernel)> {
    let caller = cx.caller().clone();
    Ok((caller, cx.host::<Kernel>()?))
}
