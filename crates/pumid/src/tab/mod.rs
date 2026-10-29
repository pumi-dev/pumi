pub mod file;
pub mod generic;
pub mod terminal;
use crate::kernel::Kernel;
use std::rc::Rc;
pub fn register(kernel: &mut Kernel) -> pumi_api::Result<()> {
    kernel.register_block(Rc::new(file::FileType))
}
