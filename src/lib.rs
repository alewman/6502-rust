//! A Rust NMOS 6502 core transcribed from
//! [6502-python](https://github.com/alewman/6502-python) at `741419a` and
//! proven equivalent to it at every processor boundary.
//!
//! Each module is the transcription of one `sixfiveohtwo/*.py` module, with
//! the same handler names, the same explicit dispatch table and the hardware
//! comments carried over: [`frame`] (the 65xx-family base, `CPUFrame`),
//! `core`, `alu`, `control`, `loads`, `shifts`, `undocumented`, `dispatch`,
//! `cpu`, [`state`] and [`trace`]. Where Rust forced a difference, the module
//! header says so.
//!
//! ```
//! use sixfiveohtwo::{Bus, Frame, Mos6502};
//!
//! struct Ram(Vec<u8>);
//! impl Bus for Ram {
//!     fn read(&mut self, address: u16, _: &mut Frame) -> u8 {
//!         self.0[address as usize]
//!     }
//!     fn write(&mut self, address: u16, value: u8, _: &mut Frame) {
//!         self.0[address as usize] = value;
//!     }
//! }
//!
//! let mut ram = Ram(vec![0; 0x10000]);
//! ram.0[0x0200] = 0xE8; // INX
//! let mut cpu = Mos6502::new(ram);
//! cpu.pc = 0x0200;
//! assert_eq!(cpu.step(), 2);
//! assert_eq!(cpu.x, 1);
//! ```

mod alu;
mod control;
mod core;
mod cpu;
mod dispatch;
pub mod frame;
mod loads;
mod shifts;
pub mod state;
pub mod trace;
mod undocumented;

pub use crate::core::{Bus, Mos6502, B, C, D, N, STACK, U, V, Z};
pub use crate::dispatch::{Mode, Op, OPCODES};
pub use crate::frame::{Frame, Part, PollFields, I, IRQ_LINE, NMI_LATCH, NO_POLL};
pub use crate::state::CpuState;
