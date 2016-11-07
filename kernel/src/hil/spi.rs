//! Traits and parameters for SPI master communication.

use core::option::Option;

/// Values for the ordering of bits
#[derive(Copy, Clone)]
pub enum DataOrder {
    MSBFirst,
    LSBFirst,
}

/// Values for the clock polarity (idle state or CPOL)
#[derive(Copy, Clone, PartialEq)]
pub enum ClockPolarity {
    IdleLow,
    IdleHigh,
}

/// Which clock edge values are sampled on
#[derive(Copy, Clone, PartialEq)]
pub enum ClockPhase {
    SampleLeading,
    SampleTrailing,
}

pub trait SpiMasterClient {
    /// Called when a read/write operation finishes
    fn read_write_done(&self,
                       mut write_buffer: &'static mut [u8],
                       mut read_buffer: Option<&'static mut [u8]>,
                       len: usize);
}
/// The `SpiMaster` trait for interacting with SPI slave
/// devices at a byte or buffer level.
///
/// Using SpiMaster normally involves three steps:
///
/// 1. Configure the SPI bus for a peripheral
///    1a. Call set_chip_select to select which peripheral and
///        turn on SPI
///    1b. Call set operations as needed to configure bus
///    NOTE: You MUST select the chip select BEFORE configuring
///           SPI settings.
/// 2. Invoke read, write, read_write on SpiMaster
/// 3a. Call clear_chip_select to turn off bus, or
/// 3b. Call set_chip_select to choose another peripheral,
///     go to step 1b or 2.
///
/// This interface assumes that the SPI configuration for
/// a particular peripheral persists across chip select. For
/// example, with this set of calls:
///
///   specify_chip_select(1);
///   set_phase(SampleLeading);
///   specify_chip_select(2);
///   set_phase(SampleTrailing);
///   specify_chip_select(1);
///   write_byte(0); // Uses SampleLeading
///
/// If additional chip selects are needed, they can be performed
/// with GPIO and manual re-initialization of settings.
///
///   specify_chip_select(0);
///   set_phase(SampleLeading);
///   pin_a.set();
///   write_byte(0xaa); // Uses SampleLeading
///   pin_a.clear();
///   set_phase(SampleTrailing);
///   pin_b.set();
///   write_byte(0xaa); // Uses SampleTrailing
///
pub trait SpiMaster {
    type ChipSelect: Copy;

    fn set_client(&self, client: &'static SpiMasterClient);

    fn init(&self);
    fn is_busy(&self) -> bool;

    /// Perform an asynchronous read/write operation, whose
    /// completion is signaled by invoking SpiMasterClient on
    /// the initialized client. write_buffer must be Some,
    /// read_buffer may be None. If read_buffer is Some, the
    /// length of the operation is the minimum of the size of
    /// the two buffers.
    fn read_write_bytes(&self,
                        mut write_buffer: &'static mut [u8],
                        mut read_buffer: Option<&'static mut [u8]>,
                        len: usize)
                        -> bool;
    fn write_byte(&self, val: u8);
    fn read_byte(&self) -> u8;
    fn read_write_byte(&self, val: u8) -> u8;

    /// Tell the SPI peripheral what to use as a chip select pin.
    /// The type of the argument is based on what makes sense for the
    /// peripheral when this trait is implemented.
    fn specify_chip_select(&self, cs: Self::ChipSelect);

    /// Returns the actual rate set
    fn set_rate(&self, rate: u32) -> u32;
    fn get_rate(&self) -> u32;
    fn set_clock(&self, polarity: ClockPolarity);
    fn get_clock(&self) -> ClockPolarity;
    fn set_phase(&self, phase: ClockPhase);
    fn get_phase(&self) -> ClockPhase;

    // These two functions determine what happens to the chip
    // select line between transfers. If hold_low() is called,
    // then the chip select line is held low after transfers
    // complete. If release_low() is called, then the chip select
    // line is brought high after a transfer completes. A "transfer"
    // is any of the read/read_write calls. These functions
    // allow an application to manually control when the
    // CS line is high or low, such that it can issue multi-byte
    // requests with single byte operations.
    fn hold_low(&self);
    fn release_low(&self);
}

/// SPIMasterDevice provides a chip-specific interface to the SPI Master
/// hardware. The interface wraps the chip select line so that chip drivers
/// cannot communicate with different SPI devices.
pub trait SPIMasterDevice {
    /// Setup the SPI settings and speed of the bus.
    fn configure(&self, cpol: ClockPolarity, cpal: ClockPhase, rate: u32);

    /// Perform an asynchronous read/write operation, whose
    /// completion is signaled by invoking SpiMasterClient.read_write_done on
    /// the provided client. write_buffer must be Some,
    /// read_buffer may be None. If read_buffer is Some, the
    /// length of the operation is the minimum of the size of
    /// the two buffers.
    fn read_write_bytes(&self,
                        mut write_buffer: &'static mut [u8],
                        mut read_buffer: Option<&'static mut [u8]>,
                        len: usize)
                        -> bool;
}
