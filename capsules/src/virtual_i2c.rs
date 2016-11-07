use core::cell::Cell;
use kernel::common::{List, ListLink, ListNode};
use kernel::common::take_cell::TakeCell;
use kernel::hil::i2c::{self, I2CClient, Error};

pub struct MuxI2C<'a> {
    i2c: &'a i2c::I2CController,
    devices: List<'a, I2CDevice<'a>>,
    enabled: Cell<usize>,
    inflight: TakeCell<&'a I2CDevice<'a>>,
}

impl<'a> I2CClient for MuxI2C<'a> {
    fn command_complete(&self, buffer: &'static mut [u8], error: Error) {
        self.inflight.take().map(move |device| {
            device.command_complete(buffer, error);
        });
        self.do_next_op();
    }
}

impl<'a> MuxI2C<'a> {
    pub const fn new(i2c: &'a i2c::I2CController) -> MuxI2C<'a> {
        MuxI2C {
            i2c: i2c,
            devices: List::new(),
            enabled: Cell::new(0),
            inflight: TakeCell::empty(),
        }
    }

    fn enable(&self) {
        let enabled = self.enabled.get();
        self.enabled.set(enabled + 1);
        if enabled == 0 {
            self.i2c.enable();
        }
    }

    fn disable(&self) {
        let enabled = self.enabled.get();
        self.enabled.set(enabled - 1);
        if enabled == 1 {
            self.i2c.disable();
        }
    }

    fn do_next_op(&self) {
        if self.inflight.is_none() {
            let mnode = self.devices.iter().find(|node| node.operation.get() != Op::Idle);
            mnode.map(|node| {
                node.buffer.take().map(|buf| {
                    match node.operation.get() {
                        Op::Write(len) => self.i2c.write(node.addr, buf, len),
                        Op::Read(len) => self.i2c.read(node.addr, buf, len),
                        Op::WriteRead(wlen, rlen) => {
                            self.i2c.write_read(node.addr, buf, wlen, rlen)
                        }
                        Op::Idle => {} // Can't get here...
                    }
                });
                node.operation.set(Op::Idle);
                self.inflight.replace(node);
            });
        }
    }
}

#[derive(Copy, Clone,PartialEq)]
enum Op {
    Idle,
    Write(u8),
    Read(u8),
    WriteRead(u8, u8),
}

pub struct I2CDevice<'a> {
    mux: &'a MuxI2C<'a>,
    addr: u8,
    enabled: Cell<bool>,
    buffer: TakeCell<&'static mut [u8]>,
    operation: Cell<Op>,
    next: ListLink<'a, I2CDevice<'a>>,
    client: Cell<Option<&'a I2CClient>>,
}

impl<'a> I2CDevice<'a> {
    pub const fn new(mux: &'a MuxI2C<'a>, addr: u8) -> I2CDevice<'a> {
        I2CDevice {
            mux: mux,
            addr: addr,
            enabled: Cell::new(false),
            buffer: TakeCell::empty(),
            operation: Cell::new(Op::Idle),
            next: ListLink::empty(),
            client: Cell::new(None),
        }
    }

    pub fn set_client(&'a self, client: &'a I2CClient) {
        self.mux.devices.push_head(self);
        self.client.set(Some(client));
    }
}

impl<'a> I2CClient for I2CDevice<'a> {
    fn command_complete(&self, buffer: &'static mut [u8], error: Error) {
        self.client.get().map(move |client| {
            client.command_complete(buffer, error);
        });
    }
}

impl<'a> ListNode<'a, I2CDevice<'a>> for I2CDevice<'a> {
    fn next(&'a self) -> &'a ListLink<'a, I2CDevice<'a>> {
        &self.next
    }
}

impl<'a> i2c::I2CDevice for I2CDevice<'a> {
    fn enable(&self) {
        if !self.enabled.get() {
            self.enabled.set(true);
            self.mux.enable();
        }
    }

    fn disable(&self) {
        if self.enabled.get() {
            self.enabled.set(false);
            self.mux.disable();
        }
    }

    fn write_read(&self, data: &'static mut [u8], write_len: u8, read_len: u8) {
        self.buffer.replace(data);
        self.operation.set(Op::WriteRead(write_len, read_len));
        self.mux.do_next_op();
    }

    fn write(&self, data: &'static mut [u8], len: u8) {
        self.buffer.replace(data);
        self.operation.set(Op::Write(len));
        self.mux.do_next_op();
    }

    fn read(&self, buffer: &'static mut [u8], len: u8) {
        self.buffer.replace(buffer);
        self.operation.set(Op::Read(len));
        self.mux.do_next_op();
    }
}
