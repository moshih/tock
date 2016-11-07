use chip;
use core::cell::Cell;
use core::mem;
use kernel::common::VolatileCell;
use kernel::common::take_cell::TakeCell;
use kernel::hil::uart;
use nvic;
use peripheral_interrupts::NvicIdx;

#[repr(C, packed)]
pub struct Registers {
    pub task_startrx: VolatileCell<u32>,
    pub task_stoprx: VolatileCell<u32>,
    pub task_starttx: VolatileCell<u32>,
    pub task_stoptx: VolatileCell<u32>,
    _reserved1: [u32; 3],
    pub task_suspend: VolatileCell<u32>,
    _reserved2: [u32; 56],
    pub event_cts: VolatileCell<u32>,
    pub event_ncts: VolatileCell<u32>,
    pub event_rxdrdy: VolatileCell<u32>,
    _reserved3: [u32; 4],
    pub event_txdrdy: VolatileCell<u32>,
    _reserved4: [u32; 1],
    pub event_error: VolatileCell<u32>,
    _reserved5: [u32; 7],
    pub event_rxto: VolatileCell<u32>,
    _reserved6: [u32; 46],
    pub shorts: VolatileCell<u32>,
    _reserved7: [u32; 64],
    pub intenset: VolatileCell<u32>,
    pub intenclr: VolatileCell<u32>,
    _reserved8: [u32; 93],
    pub errorsrc: VolatileCell<u32>,
    _reserved9: [u32; 31],
    pub enable: VolatileCell<u32>,
    _reserved10: [u32; 1],
    pub pselrts: VolatileCell<u32>,
    pub pseltxd: VolatileCell<u32>,
    pub pselcts: VolatileCell<u32>,
    pub pselrxd: VolatileCell<u32>,
    pub rxd: VolatileCell<u32>,
    pub txd: VolatileCell<u32>,
    _reserved11: [u32; 1],
    pub baudrate: VolatileCell<u32>,
    _reserved12: [u32; 17],
    pub config: VolatileCell<u32>,
    _reserved13: [u32; 675],
    pub power: VolatileCell<u32>,
}

const UART_BASE: u32 = 0x40002000;

pub struct UART {
    regs: *mut Registers,
    client: Option<&'static uart::Client>,
    buffer: TakeCell<&'static mut [u8]>,
    len: Cell<usize>,
    index: Cell<usize>,
}

#[derive(Copy, Clone)]
pub struct UARTParams {
    pub baud_rate: u32,
}

pub static mut UART0: UART = UART::new();

// This UART implementation uses pins 8-11:
//   pin  8: RTS
//   pin  9: TX
//   pin 10: CTS
//   pin 11: RX
impl UART {
    pub const fn new() -> UART {
        UART {
            regs: UART_BASE as *mut Registers,
            client: None,
            buffer: TakeCell::empty(),
            len: Cell::new(0),
            index: Cell::new(0),
        }
    }

    fn configure(&mut self, baud_rate: u32) {
        let regs: &mut Registers = unsafe { mem::transmute(self.regs) };
        regs.enable.set(0b100);
        self.set_baud_rate(baud_rate);
        regs.pselrts.set(8);
        regs.pseltxd.set(9);
        regs.pselcts.set(10);
        regs.pselrxd.set(11);
    }

    pub fn set_client<C: uart::Client>(&mut self, client: &'static C) {
        self.client = Some(client);
    }

    fn set_baud_rate(&self, baud_rate: u32) {
        let regs: &mut Registers = unsafe { mem::transmute(self.regs) };
        match baud_rate {
            1200 => regs.baudrate.set(0x0004F000),
            2400 => regs.baudrate.set(0x0009D000),
            4800 => regs.baudrate.set(0x0013B000),
            9600 => regs.baudrate.set(0x00275000),
            14400 => regs.baudrate.set(0x003B0000),
            19200 => regs.baudrate.set(0x004EA000),
            28800 => regs.baudrate.set(0x0075F000),
            38400 => regs.baudrate.set(0x009D5000),
            57600 => regs.baudrate.set(0x00EBF000),
            76800 => regs.baudrate.set(0x013A9000),
            115200 => regs.baudrate.set(0x01D7E000),
            230400 => regs.baudrate.set(0x03AFB000),
            250000 => regs.baudrate.set(0x04000000),
            460800 => regs.baudrate.set(0x075F7000),
            1000000 => regs.baudrate.set(0x10000000),
            _ => regs.baudrate.set(0x01D7E000), //setting default to 115200
        }
    }

    pub fn enable_nvic(&self) {
        nvic::enable(NvicIdx::UART0);
    }

    pub fn disable_nvic(&self) {
        nvic::disable(NvicIdx::UART0);
    }

    pub fn enable_rx_interrupts(&self) {
        let regs: &mut Registers = unsafe { mem::transmute(self.regs) };
        regs.intenset.set(1 << 3 as u32);
    }

    pub fn enable_tx_interrupts(&self) {
        let regs: &mut Registers = unsafe { mem::transmute(self.regs) };
        regs.intenset.set(1 << 7 as u32);
    }

    pub fn disable_rx_interrupts(&self) {
        let regs: &mut Registers = unsafe { mem::transmute(self.regs) };
        regs.intenclr.set(1 << 3 as u32);
    }

    pub fn disable_tx_interrupts(&self) {
        let regs: &mut Registers = unsafe { mem::transmute(self.regs) };
        regs.intenclr.set(1 << 7 as u32);
    }

    pub fn handle_interrupt(&mut self) {
        let regs: &Registers = unsafe { mem::transmute(self.regs) };
        let rx = regs.event_rxdrdy.get() != 0;
        let tx = regs.event_txdrdy.get() != 0;

        if rx {
            let val = regs.rxd.get();
            self.client.map(|client| {
                client.read_done(val as u8);
            });
        }
        if tx {
            regs.event_txdrdy.set(0 as u32);

            if self.len.get() == self.index.get() {
                regs.task_stoptx.set(1 as u32);

                // Signal client write done
                self.client.map(|client| {
                    self.buffer.take().map(|buffer| {
                        client.write_done(buffer);
                    });
                });

                return;
            }

            self.buffer.map(|buffer| {
                regs.event_txdrdy.set(0 as u32);
                regs.txd.set(buffer[self.index.get()] as u32);
                let next_index = self.index.get() + 1;
                self.index.set(next_index);
            });
        }
    }
}

impl uart::UART for UART {
    fn init(&mut self, params: uart::UARTParams) {
        self.configure(params.baud_rate);
    }

    fn rx_ready(&self) -> bool {
        let regs: &Registers = unsafe { mem::transmute(self.regs) };
        regs.event_rxdrdy.get() & 0b1 != 0
    }

    fn tx_ready(&self) -> bool {
        let regs: &Registers = unsafe { mem::transmute(self.regs) };
        regs.event_txdrdy.get() == 1
    }

    fn send_byte(&self, byte: u8) {
        let regs: &mut Registers = unsafe { mem::transmute(self.regs) };
        regs.task_starttx.set(1 as u32);
        regs.event_txdrdy.set(0 as u32);
        regs.txd.set(byte as u32);
        while !self.tx_ready() {}
        regs.task_stoptx.set(1 as u32);
    }

    fn send_bytes(&self, bytes: &'static mut [u8], len: usize) {
        let regs: &mut Registers = unsafe { mem::transmute(self.regs) };

        if len == 0 {
            return;
        }

        self.index.set(1);
        self.len.set(len);

        regs.event_txdrdy.set(0 as u32);
        self.enable_tx_interrupts();
        regs.task_starttx.set(1 as u32);
        regs.txd.set(bytes[0] as u32);
        self.buffer.replace(bytes);
        self.enable_nvic();
    }

    fn read_byte(&self) -> u8 {
        let regs: &Registers = unsafe { mem::transmute(self.regs) };
        regs.task_startrx.set(1 as u32);
        while !self.rx_ready() {}
        regs.rxd.get() as u8
    }

    fn enable_rx(&self) {
        let regs: &mut Registers = unsafe { mem::transmute(self.regs) };
        regs.task_startrx.set(1);
    }

    fn disable_rx(&mut self) {
        let regs: &mut Registers = unsafe { mem::transmute(self.regs) };
        regs.task_stoprx.set(1);
    }

    fn enable_tx(&self) {
        let regs: &mut Registers = unsafe { mem::transmute(self.regs) };
        regs.task_starttx.set(1);
    }

    fn disable_tx(&mut self) {
        let regs: &mut Registers = unsafe { mem::transmute(self.regs) };
        regs.task_stoptx.set(1);
    }
}

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn UART0_Handler() {
    use kernel::common::Queue;
    nvic::disable(NvicIdx::UART0);
    chip::INTERRUPT_QUEUE.as_mut().unwrap().enqueue(NvicIdx::UART0);
}
