use ast;
use cortexm4;
// use adc;
use dma;
use flashcalw;
use gpio;
use i2c;
use kernel::Chip;
use kernel::common::{RingBuffer, Queue};
use nvic;
use spi;
use usart;

pub struct Sam4l {
    pub mpu: cortexm4::mpu::MPU,
    pub systick: &'static cortexm4::systick::SysTick,
}

const IQ_SIZE: usize = 100;
static mut IQ_BUF: [nvic::NvicIdx; IQ_SIZE] = [nvic::NvicIdx::HFLASHC; IQ_SIZE];
pub static mut INTERRUPT_QUEUE: Option<RingBuffer<'static, nvic::NvicIdx>> = None;


impl Sam4l {
    pub unsafe fn new() -> Sam4l {
        INTERRUPT_QUEUE = Some(RingBuffer::new(&mut IQ_BUF));

        usart::USART2.set_dma(&mut dma::DMAChannels[0], dma::DMAPeripheral::USART2_TX);
        dma::DMAChannels[0].client = Some(&mut usart::USART2);

        usart::USART3.set_dma(&mut dma::DMAChannels[1], dma::DMAPeripheral::USART3_TX);
        dma::DMAChannels[1].client = Some(&mut usart::USART3);

        spi::SPI.set_dma(&mut dma::DMAChannels[2], &mut dma::DMAChannels[3]);
        dma::DMAChannels[2].client = Some(&mut spi::SPI);
        dma::DMAChannels[3].client = Some(&mut spi::SPI);

        i2c::I2C2.set_dma(&dma::DMAChannels[4]);
        dma::DMAChannels[4].client = Some(&mut i2c::I2C2);

        i2c::I2C1.set_dma(&dma::DMAChannels[5]);
        dma::DMAChannels[5].client = Some(&mut i2c::I2C1);

        Sam4l {
            mpu: cortexm4::mpu::MPU::new(),
            systick: cortexm4::systick::SysTick::new(),
        }
    }
}

impl Chip for Sam4l {
    type MPU = cortexm4::mpu::MPU;
    type SysTick = cortexm4::systick::SysTick;

    fn service_pending_interrupts(&mut self) {
        use nvic::NvicIdx::*;

        unsafe {
            let iq = INTERRUPT_QUEUE.as_mut().unwrap();
            while let Some(interrupt) = iq.dequeue() {
                match interrupt {
                    ASTALARM => ast::AST.handle_interrupt(),

                    USART2 => usart::USART2.handle_interrupt(),
                    USART3 => usart::USART3.handle_interrupt(),

                    PDCA0 => dma::DMAChannels[0].handle_interrupt(),
                    PDCA1 => dma::DMAChannels[1].handle_interrupt(),
                    PDCA2 => dma::DMAChannels[2].handle_interrupt(),
                    PDCA3 => dma::DMAChannels[3].handle_interrupt(),
                    PDCA4 => dma::DMAChannels[4].handle_interrupt(),
                    PDCA5 => dma::DMAChannels[5].handle_interrupt(),

                    GPIO0 => gpio::PA.handle_interrupt(),
                    GPIO1 => gpio::PA.handle_interrupt(),
                    GPIO2 => gpio::PA.handle_interrupt(),
                    GPIO3 => gpio::PA.handle_interrupt(),
                    GPIO4 => gpio::PB.handle_interrupt(),
                    GPIO5 => gpio::PB.handle_interrupt(),
                    GPIO6 => gpio::PB.handle_interrupt(),
                    GPIO7 => gpio::PB.handle_interrupt(),
                    GPIO8 => gpio::PC.handle_interrupt(),
                    GPIO9 => gpio::PC.handle_interrupt(),
                    GPIO10 => gpio::PC.handle_interrupt(),
                    GPIO11 => gpio::PC.handle_interrupt(),

                    TWIM0 => i2c::I2C0.handle_interrupt(),
                    TWIM1 => i2c::I2C1.handle_interrupt(),
                    TWIM2 => i2c::I2C2.handle_interrupt(),
                    TWIM3 => i2c::I2C3.handle_interrupt(),

                    HFLASHC => flashcalw::flash_controller.handle_interrupt(),
                    // NvicIdx::ADCIFE   => self.adc.handle_interrupt(),
                    _ => {}
                }
                nvic::enable(interrupt);
            }
        }
    }

    fn has_pending_interrupts(&self) -> bool {
        unsafe { INTERRUPT_QUEUE.as_mut().unwrap().has_elements() }
    }

    fn mpu(&self) -> &cortexm4::mpu::MPU {
        &self.mpu
    }

    fn systick(&self) -> &cortexm4::systick::SysTick {
        self.systick
    }
}
