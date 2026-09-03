//! System call handling and userspace transition for AArch64.

use crate::arch::traits::ArchSyscall;

pub struct ArmSyscall;

impl ArchSyscall for ArmSyscall {
    #[inline]
    fn init() {
        // Will configure EL0 transition in T18.4
    }
}
